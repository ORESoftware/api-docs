// Streaming RPC client. A chain terminates in stream(); this surface has no
// makeCall() and never aggregates the response into a single value.
//
// The returned handle is both an RxJS Observable source and an async iterable,
// so `for await (const item of handle)` and `handle.observable.pipe(...)` are
// two views of the same subscription.

import { Observable, defer, from, throwError, timer } from "rxjs";
import {
  catchError,
  debounceTime,
  finalize,
  retry,
  sampleTime,
  share,
  switchMap,
  takeUntil,
  tap,
  throttleTime,
  timeout as timeoutOperator,
} from "rxjs/operators";

import {
  INSPECT,
  RpcChainState,
  RpcOptionError,
  installMethods,
  wireHeadersFor,
} from "./fluent-core.js";
import { DEFAULT_RPC_PATH } from "./options.generated.js";

export { RpcOptionError };

export class RpcStreamTransportError extends Error {
  constructor(message, reason, code, options) {
    super(message, options);
    this.name = "RpcStreamTransportError";
    this.reason = reason;
    this.code = code;
  }
}

export class RpcStreamTimeoutError extends Error {
  constructor(key, kind, millis) {
    super(`RPC stream ${key} exceeded its ${kind} timeout of ${millis}ms`);
    this.name = "RpcStreamTimeoutError";
    this.kind = kind;
    this.millis = millis;
  }
}

export class RpcStreamOverflowError extends Error {
  constructor(key, capacity) {
    super(`RPC stream ${key} overflowed its ${capacity}-item buffer`);
    this.name = "RpcStreamOverflowError";
  }
}

/** A live stream: observable, async-iterable, and cancellable. */
export class RpcStreamHandle {
  constructor(observable, live, context) {
    this.observable = observable;
    this.live = live;
    this.context = context;
  }

  async cancel() {
    if (this.context.ended || this.context.cancelled) return;
    try {
      await this.live.session?.cancel?.();
      this.context.cancelled = true;
    } catch (cause) {
      throw new RpcStreamTransportError(
        `${this.context.key}: stream cancellation failed`,
        "carrier",
        undefined,
        { cause },
      );
    }
  }

  /**
   * Bridge the observable to an async iterator with an explicit bounded queue,
   * so the declared backpressure strategy governs a slow consumer instead of
   * unbounded memory growth.
   */
  async *[Symbol.asyncIterator]() {
    const capacity = this.context.bufferCapacity;
    const strategy = this.context.backpressure;
    const queue = [];
    let resolveNext;
    let finished = false;
    let failure;

    const wake = () => {
      const resolve = resolveNext;
      resolveNext = undefined;
      resolve?.();
    };

    const subscription = this.observable.subscribe({
      next: (item) => {
        if (queue.length >= capacity) {
          if (strategy === "drop_newest") return;
          if (strategy === "drop_oldest") {
            queue.shift();
          } else if (strategy === "error") {
            failure = new RpcStreamOverflowError(this.context.key, capacity);
            finished = true;
            wake();
            return;
          }
        }
        queue.push(item);
        wake();
      },
      error: (error) => {
        failure = error;
        finished = true;
        wake();
      },
      complete: () => {
        this.context.ended = true;
        finished = true;
        wake();
      },
    });

    try {
      while (true) {
        while (queue.length > 0) yield queue.shift();
        if (finished) break;
        await new Promise((resolve) => {
          resolveNext = resolve;
        });
      }
      while (queue.length > 0) yield queue.shift();
      if (failure) throw failure;
    } finally {
      subscription.unsubscribe();
    }
  }
}

/**
 * The concrete path a stream opens: `{name}` placeholders filled from the
 * chain's path fields. A placeholder with no field, or a field with no
 * placeholder, is an error — either way something the caller wrote would
 * silently not be sent.
 */
function expandPath(template, fields, key) {
  const given = fields ?? {};
  const used = new Set();
  const path = template.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_match, name) => {
    if (!Object.prototype.hasOwnProperty.call(given, name)) {
      throw new RpcOptionError(
        `stream ${key}: path placeholder {${name}} has no value; supply it with addPathField`,
      );
    }
    used.add(name);
    return encodeURIComponent(String(given[name]));
  });
  for (const name of Object.keys(given)) {
    if (!used.has(name)) {
      throw new RpcOptionError(
        `stream ${key}: path field ${name} has no {${name}} placeholder in ${template}`,
      );
    }
  }
  return path;
}

export class RpcStreamCallBuilder {
  constructor(state, framedStream, decode, idPrefix, request) {
    this.state = state;
    this.framedStream = framedStream;
    this.decode = decode;
    this.idPrefix = idPrefix;
    this.request = request;
    installMethods(
      this,
      state,
      (next) =>
        new RpcStreamCallBuilder(next, framedStream, decode, idPrefix, request),
    );
    Object.freeze(this);
  }

  toPlan() {
    return this.state.toPlan();
  }

  /** Serializing or logging a chain shows its redacted plan. See RpcChainState. */
  toJSON() {
    return this.state.toPlan();
  }

  [INSPECT]() {
    return { RpcStreamCall: this.state.toPlan() };
  }

  /** The sole network boundary of the streaming client. */
  async stream() {
    const state = this.state;
    const plan = state.toPlan();
    const wire = state.wirePlan();
    const id = `${this.idPrefix}-${state.key}`;

    const call = {
      v: 1,
      op: "call",
      id,
      key: state.key,
      transport: this.framedStream.carrier,
      method: this.request.method,
      // Built from the chain STATE, the same place the plan is built from. The
      // frame used to read the prepare()-time request instead, so
      // addQueryField()/addPathField() showed up in the plan and were never
      // sent, while a prepare()-time query was sent and never planned.
      path: expandPath(this.request.path, state.request.path, state.key),
      ...(state.request.query === undefined ? {} : { query: state.request.query }),
      ...(state.request.body === undefined ? {} : { body: state.request.body }),
      headers: wireHeadersFor(state),
    };

    const context = {
      id,
      key: state.key,
      carrier: this.framedStream.carrier,
      ended: false,
      cancelled: false,
      attempts: 0,
      bufferCapacity: plan.stream_buffer_capacity ?? 1024,
      backpressure: plan.backpressure ?? "buffer",
    };

    // retry() must reopen one failed attempt, but multiple consumers of ONE
    // RpcStreamHandle must not each open their own carrier. `live` therefore
    // represents the single upstream session shared by all current views.
    const live = { session: undefined };

    // Close one attempt's session, swallowing the carrier's own failure: a
    // cancel that throws must not replace the error that caused it.
    const closeSession = async (session) => {
      if (live.session === session) live.session = undefined;
      try {
        await session.cancel?.();
      } catch {
        // The original failure is the one worth reporting.
      }
    };

    // ONE ATTEMPT: open a carrier, read it, and clean up after itself.
    //
    // Cleanup lives inside the retried unit on purpose. With it outside retry,
    // a failed session stayed open while the next one was opened, so a flapping
    // carrier stacked up live sessions. The error path awaits the close before
    // re-raising, so by the time retry resubscribes the old session is gone;
    // finalize covers the paths with no error to hang that on — a consumer that
    // stops reading, or the total timeout unsubscribing from outside.
    const attempt$ = defer(() => {
      context.attempts += 1;
      // `call` is the frame sent to the server, so it carries no carrier
      // options. Those travel beside it: `plan` (redacted, safe to log) and
      // `wire` (credentials intact — a proxy URL keeps its userinfo there).
      return from(this.framedStream.open(call, { plan, wire }));
    }).pipe(
      switchMap((session) => {
        live.session = session;
        let closed = false;
        const close = () => {
          if (closed) return Promise.resolve();
          closed = true;
          return closeSession(session);
        };
        let frames = framesToObservable(session, context, this.decode);
        if (plan.stream_idle_timeout_millis !== undefined) {
          // Idle is a property of one carrier, so it belongs to the attempt: a
          // stalled session is a reason to reopen, and retry can act on it.
          frames = frames.pipe(
            timeoutOperator({
              each: plan.stream_idle_timeout_millis,
              with: () =>
                throwError(
                  () =>
                    new RpcStreamTimeoutError(state.key, "idle", plan.stream_idle_timeout_millis),
                ),
            }),
          );
        }
        return frames.pipe(
          catchError((error) => from(close()).pipe(switchMap(() => throwError(() => error)))),
          finalize(() => {
            // A clean end or a server cancel already closed the carrier.
            if (!context.ended && !context.cancelled) void close();
          }),
        );
      }),
    );

    let attempts$ = attempt$;
    if (plan.retry_count !== undefined && plan.retry_count > 0) {
      const backoff = plan.retry_backoff;
      const hooks = state.hooks.get("on_retry") ?? [];
      attempts$ = attempt$.pipe(
        retry({
          count: plan.retry_count,
          delay: (error, attemptIndex) => {
            for (const hook of hooks) hook(attemptIndex, error);
            if (!backoff) return timer(0);
            return timer(Math.round(backoff.base_millis * backoff.factor ** (attemptIndex - 1)));
          },
        }),
      );
    }

    // TOTAL timeout, OUTSIDE retry. Inside it, every resubscription started a
    // fresh timer — withTimeout(100) with three retries ran for 400ms — and the
    // timeout error was itself retried. One budget covers every attempt and
    // every backoff wait, and expiring it is terminal.
    //
    // takeUntil(timer(...)) alone would COMPLETE the stream, reporting a
    // timed-out stream as a clean end; erroring through the notifier keeps it a
    // failure. Unsubscribing the attempt runs its finalize, which closes the
    // carrier.
    if (plan.timeout_millis !== undefined) {
      attempts$ = attempts$.pipe(
        takeUntil(
          timer(plan.timeout_millis).pipe(
            switchMap(() =>
              throwError(
                () => new RpcStreamTimeoutError(state.key, "total", plan.timeout_millis),
              ),
            ),
          ),
        ),
      );
    }

    // delay()/addJitter() hold the FIRST open, exactly as they hold a unary
    // call, and are not charged to the timeout. Shaping the item stream instead
    // (auditTime) silently drops items.
    const jitter =
      plan.jitter_millis === undefined ? 0 : Math.floor(Math.random() * plan.jitter_millis);
    const lead = (plan.delay_millis ?? 0) + jitter;
    let frames$ = lead > 0 ? timer(lead).pipe(switchMap(() => attempts$)) : attempts$;

    // Inbound rate shaping. The catalog makes these mutually exclusive.
    if (plan.sample_each_millis !== undefined) {
      frames$ = frames$.pipe(sampleTime(plan.sample_each_millis));
    }
    if (plan.throttle_each_millis !== undefined) {
      frames$ = frames$.pipe(
        throttleTime(plan.throttle_each_millis, undefined, { leading: true, trailing: false }),
      );
    }
    if (plan.debounce_each_millis !== undefined) {
      frames$ = frames$.pipe(debounceTime(plan.debounce_each_millis));
    }

    frames$ = frames$.pipe(
      tap({
        error: (error) => {
          context.error = error;
        },
      }),
      // A handle is one live call. Without share(), every subscription to the
      // cold pipeline above opened another carrier while all subscriptions
      // mutated the SAME `live.session` and context. Observable + async-iterator
      // consumers could therefore create two sockets and cancel only whichever
      // one happened to open last. Multicast one upstream while any view is
      // attached. Terminal completion/error stays terminal for this handle;
      // dropping every view early tears down the carrier and permits a later
      // view to establish a fresh active session.
      share({
        resetOnError: false,
        resetOnComplete: false,
        resetOnRefCountZero: true,
      }),
    );

    return new RpcStreamHandle(frames$, live, context);
  }
}

function framesToObservable(session, context, decode) {
  return new Observable((subscriber) => {
    let cancelled = false;
    (async () => {
      try {
        for await (const frame of session.incoming) {
          if (cancelled) return;
          if (frame?.id !== context.id) {
            throw new RpcStreamTransportError(
              `frame for correlation id ${String(frame?.id)} arrived on the stream for ${context.id}`,
              "protocol",
            );
          }
          switch (frame.t) {
            case "data": {
              if (!Object.prototype.hasOwnProperty.call(frame, "body")) {
                throw new RpcStreamTransportError(
                  "a stream data frame arrived without a body",
                  "protocol",
                );
              }
              subscriber.next(decode(frame.body));
              break;
            }
            case "end":
              context.ended = true;
              subscriber.complete();
              return;
            case "cancel":
              context.cancelled = true;
              subscriber.complete();
              return;
            case "error":
              throw new RpcStreamTransportError(
                frame.message ?? `remote ${frame.code ?? "unknown"}`,
                "remote",
                frame.code,
              );
            case "call":
              throw new RpcStreamTransportError(
                "a call frame cannot arrive inside its response stream",
                "protocol",
              );
            default:
              throw new RpcStreamTransportError(
                `unknown stream frame type ${String(frame.t)}`,
                "protocol",
              );
          }
        }
        if (!context.ended && !context.cancelled) {
          throw new RpcStreamTransportError(
            "stream transport ended without an end or cancel frame",
            "protocol",
          );
        }
        subscriber.complete();
      } catch (error) {
        subscriber.error(error);
      }
    })();
    return () => {
      cancelled = true;
    };
  });
}

export class OresRpcStreamClient {
  constructor({
    framedStream,
    operations,
    rpcPath = DEFAULT_RPC_PATH,
    idPrefix = "ores",
    capabilities = [],
  }) {
    if (!framedStream || typeof framedStream.open !== "function") {
      throw new TypeError("RPC streaming client requires a framed stream carrier");
    }
    this.framedStream = framedStream;
    this.operations = new Set(operations);
    this.rpcPath = rpcPath;
    this.idPrefix = idPrefix;
    this.capabilities = new Set(capabilities);
  }

  prepare(key, request, decode = (value) => value) {
    if (!this.operations.has(key)) {
      throw new Error(`RPC operation not generated for this audience: ${String(key)}`);
    }
    if (!request || typeof request.method !== "string" || typeof request.path !== "string") {
      throw new TypeError("RPC streaming request requires a method and a path");
    }
    // The prepare()-time query is folded into the chain state so there is one
    // query: planned, redacted and sent from the same object.
    const state = new RpcChainState("stream", key, this.rpcPath, { query: request.query });
    for (const capability of this.capabilities) state.capabilities.add(capability);
    return new RpcStreamCallBuilder(state, this.framedStream, decode, this.idPrefix, request);
  }
}
