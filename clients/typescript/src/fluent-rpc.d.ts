export type RpcJsonObject = Record<string, unknown>;
export type RpcEndpointTarget = "default" | "standalone" | "lambda";

export interface RpcEndpointSelection {
  /** Canonical closed selector. */
  target?: RpcEndpointTarget;
  /** Compatibility shorthand for target="lambda". */
  lambda?: boolean;
  /** Compatibility shorthand for target="standalone". */
  standalone?: boolean;
}

export interface RpcCallArgs {
  path?: RpcJsonObject;
  query?: RpcJsonObject;
  headers?: RpcJsonObject;
  body?: unknown;
  traceId?: string;
  spanId?: string;
}

export interface RpcContext<E = RpcJsonObject> {
  ok: boolean;
  status: number;
  id: string;
  key: string;
  transport: "http" | "tcp" | "websocket" | "nats";
  /** Local deployment endpoint used for this HTTP RPC call. */
  endpointTarget: "standalone" | "lambda";
  headers: RpcJsonObject;
  trailers: RpcJsonObject;
  errors: E[];
  traceId?: string;
  traceIds: string[];
  spanId?: string;
}

export type RpcOutcome<T, E = RpcJsonObject> = readonly [
  T | undefined,
  RpcContext<E>,
];

export class RpcRemoteError<E = RpcJsonObject> extends Error {
  readonly ctx: RpcContext<E>;
  constructor(ctx: RpcContext<E>);
}

export class RpcCallBuilder<T = unknown, E = RpcJsonObject> {
  constructor(
    baseUrl: string,
    rpcPath: string,
    fetchImpl: typeof fetch,
    key: string,
    args?: RpcCallArgs,
    endpointTarget?: "standalone" | "lambda",
  );
  addHeader(name: string, value: unknown): this;
  addHeaders(values: RpcJsonObject): this;
  addPathField(name: string, value: unknown): this;
  addQueryField(name: string, value: unknown): this;
  addBodyField(name: string, value: unknown): this;
  withBody(body: unknown): this;
  withTraceId(traceId: string): this;
  withSpanId(spanId: string): this;
  makeCall(): Promise<RpcOutcome<T, E>>;
  makeCallOrThrow(): Promise<T>;
}

export interface OresRpcClientConfig<K extends string> {
  /** Backward-compatible standalone/default endpoint. */
  readonly baseUrl: string;
  readonly standaloneBaseUrl?: string;
  readonly lambdaBaseUrl?: string;
  readonly defaultTarget?: "standalone" | "lambda";
  readonly rpcPath?: string;
  readonly operations: Iterable<K>;
  readonly fetchImpl?: typeof fetch;
}

export class OresRpcClient<K extends string = string> {
  constructor(config: OresRpcClientConfig<K>);
  prepare<T = unknown, E = RpcJsonObject>(
    key: K,
    args?: RpcCallArgs,
    endpoint?: RpcEndpointSelection,
  ): RpcCallBuilder<T, E>;
  call<T = unknown, E = RpcJsonObject>(
    key: K,
    args?: RpcCallArgs,
    endpoint?: RpcEndpointSelection,
  ): RpcCallBuilder<T, E>;
}
