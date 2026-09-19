// Unary RPC client. A chain terminates in makeCall(); this surface has no
// stream() and cannot open a long-lived carrier.
//
// Resilience, flow control and queueing are expressed as RxJS operator
// pipelines rather than hand-rolled timers, so retry/backoff/throttle/debounce
// semantics are the library's and are shared with the streaming client.

import {
  Subject,
  asyncScheduler,
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
  debounceTime,
  throttleTime,
  delay as delayOperator,
  map,
  retry,
  tap,
  timeout as timeoutOperator,
} from "rxjs/operators";

import {
  INSPECT,
  RpcChainState,
  RpcOptionError,
  executionDigest,
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

export class RpcTimeoutError extends Error {
  constructor(key, kind, millis) {
    super(`RPC ${key} exceeded its ${kind} timeout of ${millis}ms`);
    this.name = "RpcTimeoutError";
    this.kind = kind;
    this.millis = millis;
  }
}

/**
 * Upper bound on every scheduler map. Shape and concurrency keys are
 * caller-controlled, so without a bound a caller that mints a fresh key per
 * request grows these maps for the life of the client.
 */
const MAX_SCHEDULER_ENTRIES = 1024;

/** Per-client scheduling state shared by every chain the client produces. */
class Scheduler {
  constructor() {
    this.inFlight = new Map(); // execution digest -> Promise
    this.queues = new Map(); // concurrency key -> tail Promise
    this.throttles = new Map(); // shape key -> throttle gate
    this.debounces = new Map(); // shape key -> debounce gate
    this.cache = new Map(); // execution digest -> { expiresAt, staleUntil, outcome }
    this.refreshes = new Map(); // execution digest -> background revalidation
    this.queueDepth = 0;
  }

  /**
   * A private copy of a cached outcome, or undefined when there is nothing
   * usable. `stale` says the entry is past its TTL but inside its
   * stale-while-revalidate window.
   */
  recall(digest) {
    const entry = this.cache.get(digest);
    if (!entry) return undefined;
    const now = Date.now();
    if (entry.staleUntil <= now) {
      this.cache.delete(digest);
      return undefined;
    }
    return { outcome: structuredClone(entry.outcome), stale: entry.expiresAt <= now };
  }

  /**
   * Insert with a bound: drop dead entries, then the oldest, to make room.
   *
   * Only a successful answer from the server is worth remembering. Caching an
   * error pins a transient 503 for the whole TTL, and caching a fallback turns
   * "the network was down once" into "the network is down" for every later
   * caller.
   *
   * The cache keeps its OWN copy. Storing the object the caller was handed
   * meant one caller sorting a result in place reordered it for everyone after.
   * An outcome that cannot be cloned is not cached at all: no cache is a
   * slowdown, a shared mutable cache is a bug.
   */
  remember(digest, outcome, ttlMillis, staleMillis) {
    const [, ctx] = outcome;
    if (ctx.ok !== true || ctx.fallback === true) return;
    let copy;
    try {
      copy = structuredClone(outcome);
    } catch {
      return;
    }
    const now = Date.now();
    for (const [key, cached] of this.cache) {
      if (cached.staleUntil <= now) this.cache.delete(key);
    }
    this.cache.delete(digest); // re-insert, so a refreshed entry is the newest
    while (this.cache.size >= MAX_SCHEDULER_ENTRIES) {
      this.cache.delete(this.cache.keys().next().value);
    }
    const expiresAt = now + ttlMillis;
    this.cache.set(digest, { expiresAt, staleUntil: expiresAt + staleMillis, outcome: copy });
  }

  /** Make room in a gate map by closing the oldest gate. */
  makeRoom(gates) {
    while (gates.size >= MAX_SCHEDULER_ENTRIES) {
      const oldest = gates.keys().next().value;
      gates.get(oldest).close("evicted to bound scheduler memory");
      gates.delete(oldest);
    }
  }
}

function shapeKey(state) {
  return `${state.key}::${state.plan.concurrency_key ?? ""}`;
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

  /** Serializing or logging a chain shows its redacted plan. See RpcChainState. */
  toJSON() {
    return this.state.toPlan();
  }

  [INSPECT]() {
    return { RpcUnaryCall: this.state.toPlan() };
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
    // Cache and dedupe are keyed by the execution DIGEST: a hash of the
    // pre-redaction call document. Never by the plan, and never by the plan plus
    // some hand-picked extras — redaction decides what two principals' plans
    // have in common, so only the unredacted document can tell them apart.
    const caches = plan.cache_ttl_seconds !== undefined;
    const digest = caches || plan.dedupe === true ? await executionDigest(state) : undefined;

    if (caches && plan.skip_local_cache !== true) {
      const hit = this.scheduler.recall(digest);
      if (hit) {
        // Past its TTL but inside the stale window: answer now, refresh behind.
        if (hit.stale) this.#revalidate(plan, digest);
        return hit.outcome;
      }
    }
    if (plan.dedupe === true) {
      const existing = this.scheduler.inFlight.get(digest);
      if (existing) return existing;
      const started = this.#enqueue(plan, digest).finally(() => {
        this.scheduler.inFlight.delete(digest);
      });
      this.scheduler.inFlight.set(digest, started);
      return started;
    }
    return this.#enqueue(plan, digest);
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
  #enqueue(plan, digest) {
    const run = () => this.#execute(plan, digest);
    const key = plan.concurrency_key;
    if (key === undefined) return run();

    const queues = this.scheduler.queues;
    const tail = queues.get(key) ?? Promise.resolve();
    const next = tail.then(run, run);
    const settled = next.then(
      () => undefined,
      () => undefined,
    );
    queues.set(key, settled);
    // Forget the key once its queue drains. Concurrency keys are
    // caller-controlled, so a tail kept forever is a leak per distinct key.
    void settled.then(() => {
      if (queues.get(key) === settled) queues.delete(key);
    });
    return next;
  }

  /**
   * The RxJS pipeline. Order is the contract:
   *
   *   attempt -> retry/backoff -> TOTAL timeout -> lead delay -> absolute deadline
   *
   * The timeout sits OUTSIDE retry. Applied per attempt, every resubscription
   * started a fresh timer, so withTimeout(100) with three retries ran for 400ms
   * — and the timeout error itself was retried. One budget covers every attempt
   * and every backoff wait, and expiring it is terminal.
   */
  #execute(plan, digest) {
    const state = this.state;
    const scheduler = this.scheduler;
    scheduler.queueDepth += 1;

    let attempt$ = defer(() =>
      from(
        this.transport({
          key: state.key,
          rpcPath: state.rpcPath,
          // `plan` is redacted and safe to log. `wire` is the same document
          // with credentials intact: a transport executes from it (a proxy URL
          // keeps its userinfo there) and must never log it.
          plan,
          wire: state.wirePlan(),
          headers: wireHeadersFor(state),
          path: state.request.path,
          query: state.request.query,
          body: state.request.body,
          serialStrategy: plan.serial_strategy,
        }),
      ),
    );
    if (plan.debug === true) {
      attempt$ = attempt$.pipe(
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

    let attempts$ = attempt$;
    if (plan.retry_count !== undefined && plan.retry_count > 0) {
      const backoff = plan.retry_backoff;
      const hooks = state.hooks.get("on_retry") ?? [];
      attempts$ = attempt$.pipe(
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
    // The budget starts when the first attempt is sent. A deliberate delay() is
    // not the call being slow, so it is not charged to the timeout; an absolute
    // deadline, below, does include it.
    if (plan.timeout_millis !== undefined) {
      attempts$ = attempts$.pipe(
        timeoutOperator({
          first: plan.timeout_millis,
          with: () =>
            throwError(() => new RpcTimeoutError(state.key, "total", plan.timeout_millis)),
        }),
      );
    }

    const jitter =
      plan.jitter_millis === undefined ? 0 : Math.floor(Math.random() * plan.jitter_millis);
    const lead = (plan.delay_millis ?? 0) + jitter;
    let pipeline$ = lead > 0 ? timer(lead).pipe(concatMap(() => attempts$)) : attempts$;

    if (plan.deadline_unix_millis !== undefined) {
      const remaining = plan.deadline_unix_millis - Date.now();
      pipeline$ =
        remaining <= 0
          ? throwError(() => new RpcTimeoutError(state.key, "deadline", 0))
          : pipeline$.pipe(
              timeoutOperator({
                first: remaining,
                with: () =>
                  throwError(() => new RpcTimeoutError(state.key, "deadline", remaining)),
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
        scheduler.remember(
          digest,
          outcome,
          plan.cache_ttl_seconds * 1000,
          (plan.stale_while_revalidate_seconds ?? 0) * 1000,
        );
        return outcome;
      });
    }
    return settled;
  }

  /**
   * Refresh a stale entry in the background. One refresh per digest however
   * many callers hit the stale entry, and a refresh that fails is dropped: the
   * stale answer stays until its window closes, which is the point of the
   * option. #execute stores the fresh outcome itself.
   */
  #revalidate(plan, digest) {
    const refreshes = this.scheduler.refreshes;
    if (refreshes.has(digest)) return;
    const refresh = this.#enqueue(plan, digest)
      .catch(() => undefined)
      .finally(() => refreshes.delete(digest));
    refreshes.set(digest, refresh);
  }

  /**
   * Leading-edge throttle, expressed as an RxJS gate rather than a timestamp
   * comparison. throttleTime emits its leading value synchronously inside
   * next(), so a request that was not admitted by the time next() returns has
   * been dropped by the operator.
   */
  #admitThrottle(windowMillis, key) {
    const gates = this.scheduler.throttles;
    let gate = gates.get(key);
    if (gate && gate.windowMillis !== windowMillis) {
      // A changed window is a changed policy, and each call is judged by its
      // OWN window against the last call that was let through. Replacing the
      // gate unconditionally let the new call straight in — a fresh gate always
      // admits its first value — so changing the number bypassed the throttle,
      // the same way it once bypassed the debounce.
      if (asyncScheduler.now() - gate.lastAdmittedAt < windowMillis) return false;
      gate.close();
      gates.delete(key);
      gate = undefined;
    }
    if (!gate) {
      this.scheduler.makeRoom(gates);
      const subject = new Subject();
      const admit = subject
        .pipe(throttleTime(windowMillis, undefined, { leading: true, trailing: false }))
        .subscribe((request) => {
          request.admitted = true;
          gate.lastAdmittedAt = asyncScheduler.now();
        });
      // Once a full window passes with no calls the gate has nothing left to
      // remember, so it removes itself instead of living for the client's life.
      const idle = subject.pipe(debounceTime(windowMillis)).subscribe(() => {
        if (gates.get(key) === gate) gates.delete(key);
        gate.close();
      });
      gate = {
        subject,
        windowMillis,
        lastAdmittedAt: 0,
        close: () => {
          admit.unsubscribe();
          idle.unsubscribe();
        },
      };
      gates.set(key, gate);
    }
    const request = { admitted: false };
    gate.subject.next(request);
    return request.admitted;
  }

  /**
   * Trailing-edge debounce via debounceTime. Each new call supersedes the
   * pending one, which is rejected; the operator releases only the last call
   * once the key has been quiet for the interval.
   *
   * Supersession holds across an interval change too. Replacing the gate without
   * settling its pending call let BOTH calls through — the old gate still fired
   * on its own timer — which is the opposite of debouncing.
   */
  #awaitDebounce(quietMillis, key) {
    const gates = this.scheduler.debounces;
    const stateKey = this.state.key;
    let gate = gates.get(key);
    if (gate && gate.quietMillis !== quietMillis) {
      gate.close("superseded by a later call");
      gates.delete(key);
      gate = undefined;
    }
    if (!gate) {
      this.scheduler.makeRoom(gates);
      const subject = new Subject();
      const created = { subject, quietMillis, pending: undefined, close: undefined };
      const release = subject.pipe(debounceTime(quietMillis)).subscribe((request) => {
        if (created.pending === request) created.pending = undefined;
        request.resolve();
        // Quiet and empty: nothing left to remember for this key.
        if (created.pending === undefined && gates.get(key) === created) {
          gates.delete(key);
          release.unsubscribe();
        }
      });
      // Closing a gate always settles its pending call. A gate is never dropped
      // with a promise still hanging off it.
      created.close = (reason) => {
        release.unsubscribe();
        created.pending?.reject(new RpcDroppedError(stateKey, reason));
        created.pending = undefined;
      };
      gate = created;
      gates.set(key, gate);
    }
    const active = gate;
    return new Promise((resolve, reject) => {
      active.pending?.reject(new RpcDroppedError(stateKey, "superseded by a later call"));
      const request = { resolve, reject };
      active.pending = request;
      active.subject.next(request);
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
