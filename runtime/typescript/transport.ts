/**
 * `RpcTransport` over HTTP, WebSocket and TCP.
 *
 * Generated code produces a request that already carries everything all three
 * need — key, method, substituted path, template, query, body, delivery — so
 * the transport is a choice at the edge and the same generated call works over
 * any of them without regeneration.
 *
 * HTTP needs no envelope. WebSocket and TCP carry the frame envelope from
 * `./frame.ts`; `FramedConnection` is one method wide, so multiplexing,
 * reconnect and auth stay in the application and this module stays testable
 * without a socket.
 *
 * Streaming operations are declared and validated in the route map. The shared
 * stream runtime below is deliberately deferred: preparing a call performs no
 * I/O and RpcStreamCallBuilder.stream() is the sole open boundary. Emitters do
 * not yet expose streaming operations, so they remain withheld rather than
 * falling back to unary.
 */

import { Correlator, type Frame, callFrame } from "./frame.ts";
import { emit, type Carrier, type RpcEvent, type RpcTelemetrySink } from "./telemetry.ts";

/** Structurally compatible with the generated request. */
export interface RidlRequest {
  readonly key: string;
  readonly method: string;
  readonly path: string;
  readonly pathTemplate: string;
  readonly query: ReadonlyArray<readonly [string, string]>;
  readonly body?: string;
  readonly delivery: "direct" | "opto_sync_queued";
}

export interface RpcTransport {
  call(request: RidlRequest): Promise<string>;
}

/** A plain online request. The app owns URLs, auth, retries and TLS. */
export interface HttpCall {
  call(request: RidlRequest): Promise<string>;
}

/** One framed exchange: send the call, resolve with the frames answering it. */
export interface FramedConnection {
  exchange(call: Frame): Promise<Frame[]>;
  readonly carrier: Extract<Carrier, "websocket" | "tcp">;
}

/** One opened logical RPC stream. The transport owns the socket and demultiplexing. */
export interface FramedStreamSession {
  readonly incoming: AsyncIterable<Frame>;
  cancel?(): Promise<void>;
}

/** The I/O seam used by streaming RPC builders. */
export interface FramedStream {
  readonly carrier: Extract<Carrier, "websocket" | "tcp">;
  open(call: Frame): Promise<FramedStreamSession>;
}

export interface RpcStreamContext {
  readonly id: string;
  readonly key: string;
  readonly carrier: Extract<Carrier, "websocket" | "tcp">;
  readonly ended: boolean;
  readonly cancelled: boolean;
  readonly error?: RpcTransportError;
}

export class RpcTransportError extends Error {
  readonly reason: "carrier" | "remote" | "protocol";
  readonly code?: string;

  constructor(
    message: string,
    reason: "carrier" | "remote" | "protocol",
    code?: string,
    options?: { cause?: unknown },
  ) {
    super(message, options);
    this.name = "RpcTransportError";
    this.reason = reason;
    this.code = code;
  }
}

export class RpcStreamClient<T> implements AsyncIterable<T> {
  readonly #session: FramedStreamSession;
  readonly #id: string;
  readonly #key: string;
  readonly #carrier: Extract<Carrier, "websocket" | "tcp">;
  readonly #decode: (value: unknown) => T;
  #ended = false;
  #cancelled = false;
  #error?: RpcTransportError;

  constructor(
    session: FramedStreamSession,
    id: string,
    key: string,
    carrier: Extract<Carrier, "websocket" | "tcp">,
    decode: (value: unknown) => T,
  ) {
    this.#session = session;
    this.#id = id;
    this.#key = key;
    this.#carrier = carrier;
    this.#decode = decode;
  }

  get context(): RpcStreamContext {
    return {
      id: this.#id,
      key: this.#key,
      carrier: this.#carrier,
      ended: this.#ended,
      cancelled: this.#cancelled,
      error: this.#error,
    };
  }

  async cancel(): Promise<void> {
    if (this.#ended || this.#cancelled) return;
    try {
      await this.#session.cancel?.();
      this.#cancelled = true;
    } catch (cause) {
      const error = new RpcTransportError(
        `${this.#key}: stream cancellation failed`,
        "carrier",
        undefined,
        { cause },
      );
      this.#error = error;
      throw error;
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    try {
      for await (const frame of this.#session.incoming) {
        if (frame.id !== this.#id) {
          throw new RpcTransportError(
            `frame for correlation id ${frame.id} arrived on the stream for ${this.#id}`,
            "protocol",
          );
        }
        switch (frame.t) {
          case "data": {
            if (!frame.hasBody) {
              throw new RpcTransportError("a stream data frame arrived without a body", "protocol");
            }
            try {
              yield this.#decode(frame.body);
            } catch (cause) {
              throw new RpcTransportError(
                `${this.#key}: stream data failed typed decoding`,
                "protocol",
                undefined,
                { cause },
              );
            }
            break;
          }
          case "end":
            this.#ended = true;
            return;
          case "error": {
            const error = new RpcTransportError(
              frame.message ?? `remote ${frame.code ?? "unknown"}`,
              "remote",
              frame.code,
            );
            this.#error = error;
            throw error;
          }
          case "cancel":
            this.#cancelled = true;
            return;
          case "call":
            throw new RpcTransportError("a call frame cannot arrive inside its response stream", "protocol");
        }
      }
      if (!this.#ended && !this.#cancelled) {
        throw new RpcTransportError(
          "stream transport ended without an end or cancel frame",
          "protocol",
        );
      }
    } catch (cause) {
      const error =
        cause instanceof RpcTransportError
          ? cause
          : new RpcTransportError(`${this.#key}: stream failed`, "carrier", undefined, { cause });
      this.#error = error;
      throw error;
    }
  }
}

export class RpcStreamCallBuilder<T> {
  readonly #stream: FramedStream;
  readonly #request: RidlRequest;
  readonly #correlator: Correlator;
  readonly #decode: (value: unknown) => T;
  #opened = false;

  constructor(
    stream: FramedStream,
    request: RidlRequest,
    correlator: Correlator,
    decode: (value: unknown) => T,
  ) {
    this.#stream = stream;
    this.#request = request;
    this.#correlator = correlator;
    this.#decode = decode;
  }

  /**
   * Sole stream I/O boundary. Building/configuring this call performs no I/O;
   * the transport is not opened until this method is invoked.
   */
  async stream(): Promise<RpcStreamClient<T>> {
    if (this.#opened) {
      throw new RpcTransportError("a stream call builder can only be opened once", "protocol");
    }
    this.#opened = true;

    const id = this.#correlator.take();
    let body: { value: unknown } | undefined;
    if (this.#request.body !== undefined) {
      try {
        body = { value: JSON.parse(this.#request.body) };
      } catch (cause) {
        throw new RpcTransportError(
          `${this.#request.key}: request body is not JSON`,
          "protocol",
          undefined,
          { cause },
        );
      }
    }
    const call = callFrame(
      id,
      this.#request.key,
      this.#request.method,
      this.#request.path,
      this.#request.query,
      body,
    );

    let session: FramedStreamSession;
    try {
      session = await this.#stream.open(call);
    } catch (cause) {
      throw new RpcTransportError(
        `${this.#request.key}: opening stream failed`,
        "carrier",
        undefined,
        { cause },
      );
    }
    return new RpcStreamClient(
      session,
      id,
      this.#request.key,
      this.#stream.carrier,
      this.#decode,
    );
  }
}

/**
 * Shared streaming runtime. Generated service clients should subclass or
 * compose this type and expose operation-specific builders.
 */
export class FramedStreamTransport {
  readonly #stream: FramedStream;
  readonly #correlator: Correlator;

  constructor(stream: FramedStream, idPrefix = "") {
    this.#stream = stream;
    this.#correlator = new Correlator(idPrefix);
  }

  prepare<T>(
    request: RidlRequest,
    decode: (value: unknown) => T = (value) => value as T,
  ): RpcStreamCallBuilder<T> {
    return new RpcStreamCallBuilder(this.#stream, request, this.#correlator, decode);
  }
}

function observe(
  sink: RpcTelemetrySink | undefined,
  request: RidlRequest,
  service: string,
  carrier: Carrier,
  startedMs: number,
  correlationId: string | undefined,
  error: unknown,
): void {
  let outcome: RpcEvent["outcome"] = "ok";
  let code: string | undefined;
  if (error instanceof RpcTransportError) {
    outcome = error.reason === "carrier" ? "transport_error" : "failed";
    code = error.code ?? (error.reason === "protocol" ? "protocol" : undefined);
  } else if (error !== undefined) {
    outcome = "transport_error";
  }
  emit(sink, {
    key: request.key,
    service,
    method: request.method,
    pathTemplate: request.pathTemplate,
    carrier,
    outcome,
    durationMicros: Math.round((performance.now() - startedMs) * 1000),
    code,
    correlationId,
  });
}

export class HttpTransport implements RpcTransport {
  readonly #http: HttpCall;
  readonly #service: string;
  readonly #telemetry?: RpcTelemetrySink;

  constructor(http: HttpCall, service: string, telemetry?: RpcTelemetrySink) {
    this.#http = http;
    this.#service = service;
    this.#telemetry = telemetry;
  }

  async call(request: RidlRequest): Promise<string> {
    const started = performance.now();
    try {
      const body = await this.#http.call(request);
      observe(this.#telemetry, request, this.#service, "http", started, undefined, undefined);
      return body;
    } catch (cause) {
      const error =
        cause instanceof RpcTransportError
          ? cause
          : new RpcTransportError(`${request.key}: http call failed`, "carrier", undefined, { cause });
      observe(this.#telemetry, request, this.#service, "http", started, undefined, error);
      throw error;
    }
  }
}

export class FramedTransport implements RpcTransport {
  readonly #correlator: Correlator;
  readonly #conn: FramedConnection;
  readonly #service: string;
  readonly #telemetry?: RpcTelemetrySink;

  constructor(
    conn: FramedConnection,
    service: string,
    idPrefix = "",
    telemetry?: RpcTelemetrySink,
  ) {
    this.#conn = conn;
    this.#service = service;
    this.#telemetry = telemetry;
    this.#correlator = new Correlator(idPrefix);
  }

  async call(request: RidlRequest): Promise<string> {
    const started = performance.now();
    const id = this.#correlator.take();
    let body: { value: unknown } | undefined;
    if (request.body !== undefined) {
      try {
        body = { value: JSON.parse(request.body) };
      } catch (cause) {
        throw new RpcTransportError(`${request.key}: request body is not JSON`, "protocol", undefined, { cause });
      }
    }

    try {
      const frames = await this.#conn.exchange(
        callFrame(id, request.key, request.method, request.path, request.query, body),
      );
      const answer = unaryAnswer(id, frames);
      observe(this.#telemetry, request, this.#service, this.#conn.carrier, started, id, undefined);
      return answer;
    } catch (cause) {
      const error =
        cause instanceof RpcTransportError
          ? cause
          : new RpcTransportError(`${request.key}: framed call failed`, "carrier", undefined, { cause });
      observe(this.#telemetry, request, this.#service, this.#conn.carrier, started, id, error);
      throw error;
    }
  }
}

/**
 * Reduce the frames answering a unary call to its response body.
 *
 * Strict on shape: a stream of data frames arriving for a unary operation is a
 * contract violation on the server's side, and saying so beats quietly keeping
 * the first one.
 */
export function unaryAnswer(id: string, frames: readonly Frame[]): string {
  let body: string | undefined;
  let ended = false;
  for (const frame of frames) {
    if (frame.id !== id) {
      throw new RpcTransportError(
        `frame for correlation id ${frame.id} arrived on the exchange for ${id}`,
        "protocol",
      );
    }
    switch (frame.t) {
      case "data":
        if (body !== undefined) {
          throw new RpcTransportError("a unary operation answered with more than one data frame", "protocol");
        }
        if (!frame.hasBody) throw new RpcTransportError("a data frame arrived without a body", "protocol");
        body = JSON.stringify(frame.body);
        break;
      case "end":
        ended = true;
        break;
      case "error":
        throw new RpcTransportError(frame.message ?? `remote ${frame.code}`, "remote", frame.code);
      case "cancel":
        throw new RpcTransportError("the peer cancelled the exchange", "protocol");
      case "call":
        throw new RpcTransportError("a call frame cannot answer a call", "protocol");
    }
  }
  if (body === undefined) throw new RpcTransportError("the exchange ended without a response body", "protocol");
  if (!ended) throw new RpcTransportError("the exchange delivered a body but never ended", "protocol");
  return body;
}
