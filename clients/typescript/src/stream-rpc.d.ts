export type RpcStreamCarrier = "websocket" | "tcp";

export interface RpcStreamFrame {
  readonly v?: number;
  readonly id: string;
  readonly t: "call" | "data" | "end" | "error" | "cancel";
  readonly body?: unknown;
  readonly code?: string;
  readonly message?: string;
}

export interface RpcStreamSession {
  readonly incoming: AsyncIterable<RpcStreamFrame>;
  cancel?(): Promise<void>;
}

export interface FramedStream {
  readonly carrier: RpcStreamCarrier;
  open(call: Readonly<Record<string, unknown>>): Promise<RpcStreamSession>;
}

export interface RpcStreamContext {
  readonly id: string;
  readonly key: string;
  readonly carrier: RpcStreamCarrier;
  readonly ended: boolean;
  readonly cancelled: boolean;
  readonly error?: RpcStreamTransportError;
}

export interface RpcStreamRequest {
  readonly method: string;
  readonly path: string;
  readonly query?: ReadonlyArray<readonly [string, string]>;
  readonly body?: unknown;
}

export class RpcStreamTransportError extends Error {
  readonly reason: "carrier" | "remote" | "protocol";
  readonly code?: string;
  constructor(
    message: string,
    reason: "carrier" | "remote" | "protocol",
    code?: string,
    options?: ErrorOptions,
  );
}

export class RpcStreamClient<T = unknown> implements AsyncIterable<T> {
  readonly context: RpcStreamContext;
  cancel(): Promise<void>;
  [Symbol.asyncIterator](): AsyncIterator<T>;
}

export class RpcStreamCallBuilder<T = unknown> {
  addQueryField(name: string, value: unknown): this;
  withBody(body: unknown): this;
  addBodyField(name: string, value: unknown): this;
  stream(): Promise<RpcStreamClient<T>>;
}

export interface OresRpcStreamClientConfig<K extends string> {
  readonly framedStream: FramedStream;
  readonly operations: Iterable<K>;
  readonly idPrefix?: string;
}

export class OresRpcStreamClient<K extends string = string> {
  constructor(config: OresRpcStreamClientConfig<K>);
  prepare<T = unknown>(
    key: K,
    request: RpcStreamRequest,
    decode?: (value: unknown) => T,
  ): RpcStreamCallBuilder<T>;
}
