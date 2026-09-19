// Hand-authored base types shared by the generated type-state builders.
// The generated `options.generated.d.ts` imports from here; keep this file
// free of anything the catalog owns.

export type RpcJsonObject = Record<string, unknown>;

export interface RpcContext<E = RpcJsonObject> {
  readonly ok: boolean;
  readonly status: number;
  readonly id: string;
  readonly key: string;
  readonly transport: "http" | "tcp" | "websocket" | "nats";
  readonly headers: RpcJsonObject;
  readonly trailers: RpcJsonObject;
  readonly errors: E[];
  readonly traceId?: string;
  readonly traceIds: string[];
  readonly spanId?: string;
  /** Present when the outcome came from `withFallback` rather than the server. */
  readonly fallback?: true;
}

export type RpcOutcome<T, E = RpcJsonObject> = readonly [T | undefined, RpcContext<E>];

/** Canonical plan, validated by `json-schema/rpc-request-plan.schema.json`. */
export interface RpcRequestPlan {
  readonly plan_version: "1.0.0";
  readonly kind: "unary" | "stream";
  readonly key: string;
  readonly rpc_path: string;
  readonly serial_strategy: "json" | "message_pack" | "protobuf";
  readonly [field: string]: unknown;
}

export interface RpcStreamContext {
  readonly id: string;
  readonly key: string;
  readonly carrier: "websocket" | "tcp";
  ended: boolean;
  cancelled: boolean;
  readonly bufferCapacity: number;
  readonly backpressure: "buffer" | "drop_oldest" | "drop_newest" | "error";
}

/** Minimal structural view of an RxJS Observable, to avoid a type-only dep. */
export interface RpcObservable<T> {
  subscribe(observer: {
    next?: (value: T) => void;
    error?: (error: unknown) => void;
    complete?: () => void;
  }): { unsubscribe(): void };
}

export interface RpcStreamHandle<T> extends AsyncIterable<T> {
  readonly observable: RpcObservable<T>;
  readonly context: RpcStreamContext;
  cancel(): Promise<void>;
}

export interface RpcStreamRequest {
  readonly method: string;
  readonly path: string;
  readonly query?: RpcJsonObject;
}

export interface RpcCallArgs {
  path?: RpcJsonObject;
  query?: RpcJsonObject;
  headers?: RpcJsonObject;
  body?: unknown;
  traceId?: string;
  spanId?: string;
}

/**
 * What a unary transport is called with.
 *
 * `plan` is for logs and `wire` is for the network: the same document, the
 * first redacted and the second not. A transport that reads a credential-bearing
 * field from `plan` gets the placeholder — `viaProxy("http://user:pw@proxy")`
 * arrives there as `http://redacted@proxy` — so execute from `wire` and never
 * log it. `headers` and `query` are wire values too.
 */
export interface RpcUnaryTransportRequest {
  readonly key: string;
  readonly rpcPath: string;
  readonly plan: RpcRequestPlan;
  readonly wire: RpcRequestPlan;
  readonly headers: RpcJsonObject;
  readonly path?: RpcJsonObject;
  readonly query?: RpcJsonObject;
  readonly body?: unknown;
  readonly serialStrategy: RpcRequestPlan["serial_strategy"];
}

export type RpcUnaryTransport = (request: RpcUnaryTransportRequest) => Promise<unknown>;

/**
 * Passed to a stream carrier's `open` beside the call frame. The frame goes to
 * the server, so carrier options (proxy, TLS, keep-alive) never travel in it.
 */
export interface RpcStreamCarrierOptions {
  readonly plan: RpcRequestPlan;
  readonly wire: RpcRequestPlan;
}
