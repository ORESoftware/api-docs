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
  retry,
  sampleTime,
  switchMap,
  takeUntil,
  throttleTime,
  timeout as timeoutOperator,
} from "rxjs/operators";

import { RpcChainState, RpcOptionError, installMethods, wireHeadersFor } from "./fluent-core.js";
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

  /** The sole network boundary of the streaming client. */
  async stream() {
    const state = this.state;
    const plan = state.toPlan();
    const id = `${this.idPrefix}-${state.key}`;

    const call = {
      v: 1,
      op: "call",
      id,
      key: state.key,
      transport: this.framedStream.carrier,
      method: this.request.method,
      path: this.request.path,
      ...(this.request.query === undefined ? {} : { query: this.request.query }),
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

    // The carrier is opened per subscription, not once up front. retry()
    // resubscribes, and resubscribing to a session that already failed would
    // re-read a dead iterator: the carrier would never be reopened and the real
    // error would be replaced by a bogus "ended without an end frame". The
    // handle tracks whichever session is live so cancel() closes the right one.
    const live = { session: undefined };
    const jitter =
      plan.jitter_millis === undefined ? 0 : Math.floor(Math.random() * plan.jitter_millis);
    const lead = (plan.delay_millis ?? 0) + jitter;
    const open$ = defer(() => {
      context.attempts += 1;
      return from(this.framedStream.open(call));
    }).pipe(
      switchMap((session) => {
        live.session = session;
        return framesToObservable(session, context, this.decode);
      }),
    );
    // delay()/addJitter() hold the OPEN, exactly as they hold a unary call.
    // Shaping the item stream instead (auditTime) silently drops items.
    let frames$ = lead > 0 ? timer(lead).pipe(switchMap(() => open$)) : open$;

    if (plan.stream_idle_timeout_millis !== undefined) {
      frames$ = frames$.pipe(
        timeoutOperator({
          each: plan.stream_idle_timeout_millis,
          with: () =>
            throwError(
              () =>
                new RpcStreamTimeoutError(
                  state.key,
                  "idle",
                  plan.stream_idle_timeout_millis,
                ),
            ),
        }),
      );
    }
    if (plan.timeout_millis !== undefined) {
      // takeUntil(timer(...)) would COMPLETE the stream, which reports a
      // timed-out stream as a clean end and leaves the carrier open. Erroring
      // through the notifier keeps it a failure; the carrier is closed below.
      frames$ = frames$.pipe(
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
    if (plan.retry_count !== undefined && plan.retry_count > 0) {
      const backoff = plan.retry_backoff;
      const hooks = state.hooks.get("on_retry") ?? [];
      frames$ = frames$.pipe(
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

    // Any failure ends the subscription without an end/cancel frame, so the
    // carrier would otherwise stay open. Close it and record why.
    frames$ = frames$.pipe(
      catchError((error) => {
        context.error = error;
        void Promise.resolve(live.session?.cancel?.()).catch(() => {});
        return throwError(() => error);
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
    const state = new RpcChainState("stream", key, this.rpcPath, {});
    for (const capability of this.capabilities) state.capabilities.add(capability);
    return new RpcStreamCallBuilder(state, this.framedStream, decode, this.idPrefix, request);
  }
}
