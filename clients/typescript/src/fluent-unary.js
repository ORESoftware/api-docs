// Unary RPC client. A chain terminates in makeCall(); this surface has no
// stream() and cannot open a long-lived carrier.
//
// Resilience, flow control and queueing are expressed as RxJS operator
// pipelines rather than hand-rolled timers, so retry/backoff/throttle/debounce
// semantics are the library's and are shared with the streaming client.

import {
  defer,
  firstValueFrom,
  from,
  of,
  throwError,
  timer,
} from "rxjs";
import {
  catchError,
  concatMap,
  delay as delayOperator,
  map,
  retry,
  tap,
  timeout as timeoutOperator,
} from "rxjs/operators";

import {
  RpcChainState,
  RpcOptionError,
  installMethods,
  redactedHeaders,
  wireHeadersFor,
} from "./fluent-core.js";
import { DEFAULT_RPC_PATH } from "./options.generated.js";

export { RpcOptionError };

export class RpcRemoteError extends Error {
  constructor(ctx) {
    super(`RPC ${ctx.key} failed with status ${ctx.status}`);
    this.name = "RpcRemoteError";
    this.ctx = ctx;
  }
}

export class RpcDroppedError extends Error {
  constructor(key, reason) {
    super(`RPC ${key} was dropped: ${reason}`);
    this.name = "RpcDroppedError";
    this.reason = reason;
  }
}

/** Per-client scheduling state shared by every chain the client produces. */
class Scheduler {
  constructor() {
    this.inFlight = new Map(); // dedupe key -> Promise
    this.queues = new Map(); // concurrency key -> tail Promise
    this.lastEmitted = new Map(); // throttle key -> timestamp
    this.pendingDebounce = new Map(); // debounce key -> timer handle
    this.cache = new Map(); // cache key -> { expiresAt, outcome }
    this.queueDepth = 0;
  }
}

function shapeKey(state) {
  return `${state.key}::${state.plan.concurrency_key ?? ""}`;
}

function cacheKey(state) {
  return JSON.stringify(state.toPlan());
}

export class RpcUnaryCallBuilder {
  constructor(state, transport, scheduler) {
    this.state = state;
    this.transport = transport;
    this.scheduler = scheduler;
    installMethods(
      this,
      state,
      (next) => new RpcUnaryCallBuilder(next, transport, scheduler),
    );
    Object.freeze(this);
  }

  /** Canonical request plan. No network, no credentials. */
  toPlan() {
    return this.state.toPlan();
  }

  /**
   * The sole network boundary of the unary client.
   *
   * Everything before the transport call is scheduling; the transport itself is
   * invoked exactly once per attempt.
   */
  async makeCall() {
    const state = this.state;
    const plan = state.toPlan();

    if (plan.debounce_millis !== undefined) {
      await this.#awaitDebounce(plan.debounce_millis, shapeKey(state));
    }
    if (plan.throttle_millis !== undefined && !this.#admitThrottle(plan.throttle_millis, shapeKey(state))) {
      throw new RpcDroppedError(state.key, "throttled");
    }
    if (plan.drop_if_busy === true && this.scheduler.queueDepth > 0) {
      throw new RpcDroppedError(state.key, "queue busy");
    }
    if (plan.cache_ttl_seconds !== undefined && plan.skip_local_cache !== true) {
      const hit = this.scheduler.cache.get(cacheKey(state));
      if (hit && hit.expiresAt > Date.now()) return hit.outcome;
    }
    if (plan.dedupe === true) {
      const key = cacheKey(state);
      const existing = this.scheduler.inFlight.get(key);
      if (existing) return existing;
      const started = this.#enqueue(plan).finally(() => {
        this.scheduler.inFlight.delete(key);
      });
      this.scheduler.inFlight.set(key, started);
      return started;
    }
    return this.#enqueue(plan);
  }

  async makeCallOrThrow() {
    const [value, ctx] = await this.makeCall();
    if (!ctx.ok) throw new RpcRemoteError(ctx);
    if (value === undefined) {
      throw new Error(`RPC ${ctx.key} succeeded without a body`);
    }
    return value;
  }

  /** Serialize calls that share a concurrency key, then run the pipeline. */
  #enqueue(plan) {
    const run = () => this.#execute(plan);
    const key = plan.concurrency_key;
    if (key === undefined) return run();

    const tail = this.scheduler.queues.get(key) ?? Promise.resolve();
    const next = tail.then(run, run);
    this.scheduler.queues.set(
      key,
      next.then(
        () => undefined,
        () => undefined,
      ),
    );
    return next;
  }

  /** The RxJS pipeline: delay, jitter, timeout, retry/backoff, fallback. */
  #execute(plan) {
    const state = this.state;
    const scheduler = this.scheduler;
    scheduler.queueDepth += 1;

    const attempt$ = defer(() =>
      from(
        this.transport({
          key: state.key,
          rpcPath: state.rpcPath,
          plan,
          headers: wireHeadersFor(state),
          path: state.request.path,
          query: state.request.query,
          body: state.request.body,
          serialStrategy: plan.serial_strategy,
        }),
      ),
    );

    const jitter = plan.jitter_millis === undefined
      ? 0
      : Math.floor(Math.random() * plan.jitter_millis);
    const lead = (plan.delay_millis ?? 0) + jitter;

    let pipeline$ = lead > 0 ? timer(lead).pipe(concatMap(() => attempt$)) : attempt$;

    if (plan.timeout_millis !== undefined) {
      pipeline$ = pipeline$.pipe(timeoutOperator({ each: plan.timeout_millis }));
    }
    if (plan.deadline_unix_millis !== undefined) {
      const remaining = plan.deadline_unix_millis - Date.now();
      pipeline$ = remaining <= 0
        ? throwError(() => new Error(`RPC ${state.key} deadline already elapsed`))
        : pipeline$.pipe(timeoutOperator({ first: remaining }));
    }
    if (plan.retry_count !== undefined && plan.retry_count > 0) {
      const backoff = plan.retry_backoff;
      const hooks = state.hooks.get("on_retry") ?? [];
      pipeline$ = pipeline$.pipe(
        retry({
          count: plan.retry_count,
          delay: (error, attemptIndex) => {
            for (const hook of hooks) hook(attemptIndex, error);
            if (!backoff) return of(0);
            const wait = backoff.base_millis * backoff.factor ** (attemptIndex - 1);
            return timer(Math.round(wait));
          },
        }),
      );
    }
    if (plan.debug === true) {
      pipeline$ = pipeline$.pipe(
        tap({
          subscribe: () =>
            console.debug("[rpc] ->", state.key, {
              plan,
              headers: redactedHeaders(state),
            }),
          next: (receipt) => console.debug("[rpc] <-", state.key, receipt),
          error: (error) => console.debug("[rpc] !!", state.key, error),
        }),
      );
    }

    let outcome$ = pipeline$.pipe(map((receipt) => toOutcome(receipt, state)));

    if (Object.prototype.hasOwnProperty.call(plan, "fallback")) {
      outcome$ = outcome$.pipe(
        catchError(() =>
          of([
            plan.fallback,
            {
              ok: true,
              status: 0,
              id: "",
              key: state.key,
              transport: "http",
              headers: {},
              trailers: {},
              errors: [],
              traceIds: [],
              fallback: true,
            },
          ]),
        ),
      );
    }

    const settled = firstValueFrom(outcome$).finally(() => {
      scheduler.queueDepth -= 1;
    });

    if (plan.cache_ttl_seconds !== undefined) {
      return settled.then((outcome) => {
        scheduler.cache.set(cacheKey(state), {
          expiresAt: Date.now() + plan.cache_ttl_seconds * 1000,
          outcome,
        });
        return outcome;
      });
    }
    return settled;
  }

  #admitThrottle(windowMillis, key) {
    const now = Date.now();
    const previous = this.scheduler.lastEmitted.get(key);
    if (previous !== undefined && now - previous < windowMillis) return false;
    this.scheduler.lastEmitted.set(key, now);
    return true;
  }

  #awaitDebounce(quietMillis, key) {
    const pending = this.scheduler.pendingDebounce.get(key);
    if (pending) {
      clearTimeout(pending.handle);
      pending.reject(new RpcDroppedError(this.state.key, "superseded by a later call"));
    }
    return new Promise((resolve, reject) => {
      const handle = setTimeout(() => {
        this.scheduler.pendingDebounce.delete(key);
        resolve();
      }, quietMillis);
      this.scheduler.pendingDebounce.set(key, { handle, reject });
    });
  }
}

function toOutcome(receipt, state) {
  if (receipt === null || typeof receipt !== "object" || Array.isArray(receipt)) {
    throw new Error("RPC receipt must be an object");
  }
  if (receipt.v !== 1 || receipt.op !== "receipt") {
    throw new Error("RPC receipt protocol discriminator mismatch");
  }
  if (receipt.key !== state.key) {
    throw new Error("RPC receipt correlation mismatch");
  }
  if (typeof receipt.ok !== "boolean") {
    throw new Error("RPC receipt is missing boolean ok");
  }
  const status = Number.isInteger(receipt.status) ? receipt.status : 0;
  return [
    receipt.body,
    {
      ok: receipt.ok && status < 400,
      status,
      id: receipt.id,
      key: receipt.key,
      transport: typeof receipt.transport === "string" ? receipt.transport : "http",
      headers: receipt.headers ?? {},
      trailers: receipt.trailers ?? {},
      errors: receipt.error === undefined ? [] : [receipt.error],
      traceId: typeof receipt.traceId === "string" ? receipt.traceId : undefined,
      traceIds: typeof receipt.traceId === "string" ? [receipt.traceId] : [],
      spanId: typeof receipt.spanId === "string" ? receipt.spanId : undefined,
    },
  ];
}

export class OresRpcUnaryClient {
  constructor({
    baseUrl,
    rpcPath = DEFAULT_RPC_PATH,
    operations,
    fetchImpl = globalThis.fetch?.bind(globalThis),
    capabilities = [],
    transport,
  }) {
    if (typeof baseUrl !== "string" || baseUrl.length === 0) {
      throw new TypeError("RPC baseUrl must be a non-empty string");
    }
    if (typeof rpcPath !== "string" || !rpcPath.startsWith("/")) {
      throw new TypeError("RPC rpcPath must be an absolute path");
    }
    this.baseUrl = baseUrl;
    this.rpcPath = rpcPath;
    this.operations = new Set(operations);
    this.capabilities = new Set(capabilities);
    this.scheduler = new Scheduler();
    this.transport = transport ?? defaultHttpTransport(baseUrl, fetchImpl);
  }

  prepare(key, args = {}) {
    if (!this.operations.has(key)) {
      throw new Error(`RPC operation not generated for this audience: ${String(key)}`);
    }
    const state = new RpcChainState("unary", key, this.rpcPath, args);
    for (const capability of this.capabilities) state.capabilities.add(capability);
    if (args.traceId !== undefined) state.plan.trace_id = args.traceId;
    if (args.spanId !== undefined) state.plan.span_id = args.spanId;
    return new RpcUnaryCallBuilder(state, this.transport, this.scheduler);
  }
}

/** Default HTTP transport. Every serialization strategy posts to /v1/rpc. */
function defaultHttpTransport(baseUrl, fetchImpl) {
  if (typeof fetchImpl !== "function") {
    return () => {
      throw new TypeError("RPC fetch implementation is required");
    };
  }
  return async ({ key, rpcPath, plan, headers, path, query, body }) => {
    const id =
      globalThis.crypto?.randomUUID?.() ?? `ores-${Date.now()}-${Math.random()}`;
    const envelope = {
      v: 1,
      op: "call",
      id,
      key,
      transport: "http",
      ...(path === undefined ? {} : { path }),
      ...(query === undefined ? {} : { query }),
      ...(Object.keys(headers).length === 0 ? {} : { headers }),
      ...(body === undefined ? {} : { body }),
      ...(plan.trace_id === undefined ? {} : { traceId: plan.trace_id }),
      ...(plan.span_id === undefined ? {} : { spanId: plan.span_id }),
    };
    const response = await fetchImpl(new URL(rpcPath, baseUrl), {
      method: "POST",
      headers: { "content-type": "application/json", ...headers },
      body: JSON.stringify(envelope),
    });
    const receipt = await response.json();
    if (receipt && typeof receipt === "object" && receipt.id !== id) {
      throw new Error("RPC receipt correlation mismatch");
    }
    return receipt;
  };
}
