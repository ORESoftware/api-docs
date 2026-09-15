import type { RpcV1Call, RpcV1Receipt, RpcV1Transport } from "./rpc.js";

/**
 * Structural contract implemented by every generated v1 route metadata table.
 * The operation key remains the stable identity; path + method are HTTP
 * bindings and transports declare which wire adapters may carry the operation.
 */
export interface RpcRouteMetadata {
  readonly key: string;
  readonly path: string;
  readonly methods: readonly string[];
  readonly transports: readonly RpcV1Transport[];
}

export type RpcRouteTable = Readonly<Record<string, RpcRouteMetadata>>;

/** Per-operation request/response surface emitted by the route generator. */
export interface RpcRouteIo {
  readonly path: Readonly<Record<string, unknown>>;
  readonly query: Readonly<Record<string, unknown>>;
  readonly headers: Readonly<Record<string, unknown>>;
  readonly body: unknown;
  readonly response: unknown;
}

export type RpcRouteIoTable<R extends RpcRouteTable> = {
  readonly [K in keyof R]: RpcRouteIo;
};

export type RpcRouteName<R extends RpcRouteTable> = Extract<keyof R, string>;

/** Only operations that explicitly declare P are callable through P. */
export type RpcRouteNameForTransport<
  R extends RpcRouteTable,
  P extends RpcV1Transport,
> = {
  [K in keyof R]: P extends R[K]["transports"][number] ? K : never;
}[keyof R] & string;

type ObjectArg<Name extends string, Value> =
  Value extends Readonly<Record<string, never>>
    ? { readonly [K in Name]?: Value }
    : { readonly [K in Name]: Value };

type BodyArg<Value> = [Value] extends [void]
  ? { readonly body?: never }
  : { readonly body: Value };

/**
 * Compile-time input for one generated operation. The HTTP method, when
 * supplied, is restricted to that operation's declared method set. It is
 * binding metadata and is never serialized into the transport-neutral RPC
 * call frame.
 */
export type TypedRpcCallArgs<
  Io extends RpcRouteIo,
  Route extends RpcRouteMetadata,
> = ObjectArg<"path", Io["path"]> &
  ObjectArg<"query", Io["query"]> &
  ObjectArg<"headers", Io["headers"]> &
  BodyArg<Io["body"]> & {
    readonly id?: string;
    readonly method?: Route["methods"][number];
    readonly traceId?: string;
    readonly spanId?: string;
  };

/**
 * Resolved route binding passed to a concrete HTTP/TCP/WebSocket/NATS adapter.
 * Adapters do not switch on arbitrary paths: they receive the binding selected
 * by the typed operation key.
 */
export interface RpcTransportBinding {
  readonly key: string;
  readonly path: string;
  readonly methods: readonly string[];
  readonly selectedMethod?: string;
  readonly transports: readonly RpcV1Transport[];
}

export type RpcTransportInvoke = (
  call: RpcV1Call,
  binding: RpcTransportBinding,
) => Promise<RpcV1Receipt>;

export interface TypedRpcClient<
  R extends RpcRouteTable,
  Io extends RpcRouteIoTable<R>,
  P extends RpcV1Transport,
> {
  readonly transport: P;

  call<K extends RpcRouteNameForTransport<R, P>>(
    key: K,
    args: TypedRpcCallArgs<Io[K], R[K]>,
  ): Promise<Io[K]["response"]>;
}

export interface TypedRpcClientConfig<
  R extends RpcRouteTable,
  Io extends RpcRouteIoTable<R>,
  P extends RpcV1Transport,
> {
  readonly routes: R;
  readonly transport: P;
  readonly invoke: RpcTransportInvoke;
  readonly correlationPrefix?: string;

  /**
   * Optional runtime validation/decoding boundary. Compile-time typing does not
   * replace JSON Schema/TJSV admission of untrusted wire responses.
   */
  readonly validateResponse?: <K extends RpcRouteNameForTransport<R, P>>(
    key: K,
    value: unknown,
  ) => Io[K]["response"];
}

export function createTypedRpcClient<
  const R extends RpcRouteTable,
  Io extends RpcRouteIoTable<R>,
  const P extends RpcV1Transport,
>(config: TypedRpcClientConfig<R, Io, P>): TypedRpcClient<R, Io, P>;
