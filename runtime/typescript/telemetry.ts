/**
 * The telemetry seam. ores-otel plugs in here; this module never imports it.
 *
 * # Direction of the dependency
 *
 * The application depends on `ores-otel` / `next-loggers` and hands this
 * module something that satisfies `RpcTelemetrySink`. Nothing here links an
 * OTel SDK, installs a global provider, owns exporter shutdown, or decides
 * sampling — the same shape `opto-sync-clients` uses for
 * `ProtocolSyncTelemetrySink`, so one application adapter serves both.
 *
 * # Fail-open, always
 *
 * Telemetry that can break a call is worse than no telemetry. A sink that
 * throws or rejects changes nothing about the RPC.
 *
 * # What is deliberately absent
 *
 * No request body, no response body, no path parameter values, no `meta`
 * contents. A route map cannot know which fields are sensitive, so none of the
 * payload crosses this boundary. The operation key, the carrier and the
 * outcome are enough for latency and error-rate signals.
 *
 * # Error paths: log, then rethrow
 *
 * `RpcErrorEvent` is the second shape this seam carries. A dispatcher that
 * fails emits one and then returns the failure — or rethrows the original
 * error, preserving its stack. Logging never replaces propagating.
 *
 * Every error event carries an `oresTraceId`: a static `ores-trace-` literal
 * written inline at the call site that failed, so a log line names one exact
 * branch of one exact function instead of a shared message string. The
 * emitting seam is supplied separately as a closed `RpcLayer`, so an adapter
 * serializing the fleet error-log contract never has to guess `rpc_layer` from
 * an error code or static id.
 */

export type Carrier = "http" | "websocket" | "tcp" | "queue";
export type Outcome = "ok" | "failed" | "transport_error" | "queued";
export type RpcLayer = "handler" | "dispatch" | "transport" | "client";

export interface RpcEvent {
  /** Operation key from the route map — low cardinality, safe as a label. */
  readonly key: string;
  readonly service: string;
  readonly method: string;
  /** The path *template*, never the substituted path: no ids in it. */
  readonly pathTemplate: string;
  readonly carrier: Carrier;
  readonly outcome: Outcome;
  readonly durationMicros: number;
  readonly code?: string;
  /** Frame correlation id, for stitching client to server on a framed transport. */
  readonly correlationId?: string;
  /** Passed through if the caller is already in a trace; never created here. */
  readonly traceId?: string;
  readonly spanId?: string;
}

export interface RpcTelemetrySink {
  emit(event: RpcEvent): void | Promise<void>;
  /**
   * The second argument is the fleet `rpc_layer` classification. Existing
   * sinks that accept only `event` remain valid JavaScript/TypeScript method
   * implementations and simply ignore the extra argument.
   */
  emitError?(event: RpcErrorEvent, layer: RpcLayer): void | Promise<void>;
}

/** Deliver one event without letting it affect the call. */
export function emit(sink: RpcTelemetrySink | undefined, event: RpcEvent): void {
  if (!sink) return;
  try {
    const result = sink.emit(event);
    if (result && typeof (result as Promise<void>).catch === "function") {
      void (result as Promise<void>).catch(() => undefined);
    }
  } catch {
    // A broken exporter is not the caller's problem.
  }
}

/**
 * Where a failure was observed, coarse enough to stay low cardinality.
 *
 * `"decode"` — an envelope, a typed request section or a response body did not
 * decode; the bytes that failed are not carried.
 * `"protocol"` — the transport refused the call: size, unknown key, carrier
 * admission, correlation mismatch.
 * `"operation"` — the operation ran and answered with its own declared error.
 * `"thrown"` — a handler threw across the dispatch boundary. The event is
 * emitted before the error is rethrown, never instead of rethrowing it.
 */
export type ErrorKind = "decode" | "protocol" | "operation" | "thrown";

/**
 * One observed failure, reduced to what is safe to record everywhere.
 *
 * Strictly narrower than `RpcEvent`: there is no message, no detail string and
 * no body, because an error message is the one field most likely to have
 * interpolated a customer identifier, a row, or a decoder's view of the input.
 * A stable `code` plus the static `oresTraceId` identify the branch precisely
 * without quoting anything the caller sent. The layer is an emission-boundary
 * classification, not a correlation key, so it is passed separately to the
 * sink rather than inferred from these fields.
 */
export interface RpcErrorEvent {
  /** Operation key, or `""` when the failure preceded reading one. */
  readonly key: string;
  readonly carrier: Carrier;
  readonly outcome: Outcome;
  readonly kind: ErrorKind;
  /**
   * Stable failure slug — `invalid_rpc_envelope`, `unknown_rpc_key`,
   * `handler_threw`. Chosen from a closed set in the emitting code, never
   * derived from input.
   */
  readonly code: string;
  /**
   * The static `ores-trace-` literal of the failing call site. Written inline
   * where the failure is observed, so one log line names one exact branch.
   * Unrelated to `RpcEvent.traceId`, which is a propagated W3C trace id.
   */
  readonly oresTraceId: string;
}

/**
 * Deliver one client-layer error event without letting it affect the failure it
 * describes. Generated TypeScript clients call this helper, so `client` is an
 * explicit boundary property rather than something adapters infer.
 */
export function emitError(
  sink: RpcTelemetrySink | undefined,
  event: RpcErrorEvent,
): void {
  emitErrorAt(sink, "client", event);
}

/** Deliver an explicitly classified error event for non-client adapters. */
export function emitErrorAt(
  sink: RpcTelemetrySink | undefined,
  layer: RpcLayer,
  event: RpcErrorEvent,
): void {
  if (!sink || typeof sink.emitError !== "function") return;
  try {
    const result = sink.emitError(event, layer);
    if (result && typeof (result as Promise<void>).catch === "function") {
      void (result as Promise<void>).catch(() => undefined);
    }
  } catch {
    // A broken exporter is not the caller's problem.
  }
}
