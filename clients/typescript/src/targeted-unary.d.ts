import type {
  RpcCallArgs,
  RpcContext,
  RpcJsonObject,
} from "./fluent-types.js";
import type { RpcUnaryCallBuilder } from "./options.generated.js";

export type RpcEndpointTarget = "default" | "standalone" | "lambda";

export interface RpcEndpointSelection {
  readonly target?: RpcEndpointTarget;
  readonly lambda?: boolean;
  readonly standalone?: boolean;
}

export interface TargetedRpcContext<E = RpcJsonObject> extends RpcContext<E> {
  readonly endpointTarget: "standalone" | "lambda";
}

export interface TargetedRpcUnaryClientConfig<K extends string> {
  readonly baseUrl: string;
  readonly standaloneBaseUrl?: string;
  readonly lambdaBaseUrl?: string;
  readonly defaultTarget?: "standalone" | "lambda";
  readonly rpcPath?: string;
  readonly operations: Iterable<K>;
  readonly fetchImpl?: typeof fetch;
  readonly capabilities?: Iterable<string>;
  readonly standaloneTransport?: (request: unknown) => Promise<unknown>;
  readonly lambdaTransport?: (request: unknown) => Promise<unknown>;
}

export type TargetedRpcUnaryCallBuilder<
  T = unknown,
  E = RpcJsonObject,
> = RpcUnaryCallBuilder<T, E> & {
  readonly endpointTarget: "standalone" | "lambda";
  makeCall(): Promise<readonly [T | undefined, TargetedRpcContext<E>]>;
};

export class OresTargetedRpcUnaryClient<K extends string = string> {
  constructor(config: TargetedRpcUnaryClientConfig<K>);
  prepare<T = unknown, E = RpcJsonObject>(
    key: K,
    args?: RpcCallArgs,
    endpoint?: RpcEndpointSelection,
  ): TargetedRpcUnaryCallBuilder<T, E>;
  call<T = unknown, E = RpcJsonObject>(
    key: K,
    args?: RpcCallArgs,
    endpoint?: RpcEndpointSelection,
  ): TargetedRpcUnaryCallBuilder<T, E>;
}
