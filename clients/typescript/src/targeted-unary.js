import {
  OresRpcUnaryClient,
  RpcRemoteError,
} from "./fluent-unary.js";
import {
  requireEndpointUrl,
  selectEndpointTarget,
} from "./endpoint-target.js";

function wrapBuilder(builder, endpointTarget) {
  const terminal = {
    async makeCall() {
      const [value, context] = await builder.makeCall();
      return [value, { ...context, endpointTarget }];
    },
    async makeCallOrThrow() {
      const [value, context] = await terminal.makeCall();
      if (!context.ok) throw new RpcRemoteError(context);
      if (value === undefined) {
        throw new Error(`RPC ${context.key} succeeded without a body`);
      }
      return value;
    },
  };

  // Do not proxy the builder itself. Its generated option methods are
  // intentionally non-configurable/read-only, and the Proxy invariants forbid
  // substituting wrapper functions for those own properties. An empty facade
  // has no such invariants and can safely delegate into the immutable builder.
  return new Proxy(Object.create(null), {
    get(_facade, property) {
      if (property === "endpointTarget") return endpointTarget;
      if (property === "makeCall") return terminal.makeCall;
      if (property === "makeCallOrThrow") return terminal.makeCallOrThrow;
      const value = Reflect.get(builder, property, builder);
      if (typeof value !== "function") return value;
      return (...args) => {
        const next = value.apply(builder, args);
        return next && typeof next === "object" && typeof next.makeCall === "function"
          ? wrapBuilder(next, endpointTarget)
          : next;
      };
    },
    has(_facade, property) {
      return property === "endpointTarget" || property in builder;
    },
    ownKeys() {
      return [...new Set(["endpointTarget", ...Reflect.ownKeys(builder)])];
    },
    getOwnPropertyDescriptor(_facade, property) {
      if (property === "endpointTarget") {
        return {
          configurable: true,
          enumerable: false,
          writable: false,
          value: endpointTarget,
        };
      }
      const descriptor = Reflect.getOwnPropertyDescriptor(builder, property);
      if (!descriptor) return undefined;
      // Facade descriptors must be configurable because these properties do not
      // physically exist on the empty proxy target.
      return { ...descriptor, configurable: true };
    },
  });
}

/**
 * Endpoint-selecting facade for the full fluent unary client.
 *
 * Each endpoint owns a distinct OresRpcUnaryClient and therefore a distinct
 * scheduler/cache/dedupe/throttle/debounce state. Deployment placement never
 * enters the RpcRequestPlan or RpcV1Call wire envelope.
 */
export class OresTargetedRpcUnaryClient {
  constructor({
    baseUrl,
    standaloneBaseUrl = baseUrl,
    lambdaBaseUrl,
    defaultTarget = "standalone",
    rpcPath,
    operations,
    fetchImpl = globalThis.fetch?.bind(globalThis),
    capabilities = [],
    standaloneTransport,
    lambdaTransport,
  }) {
    this.standaloneBaseUrl = requireEndpointUrl("standaloneBaseUrl", standaloneBaseUrl);
    this.lambdaBaseUrl = requireEndpointUrl("lambdaBaseUrl", lambdaBaseUrl, { optional: true });
    if (defaultTarget !== "standalone" && defaultTarget !== "lambda") {
      throw new TypeError("RPC defaultTarget must be standalone or lambda");
    }
    if (defaultTarget === "lambda" && this.lambdaBaseUrl === undefined) {
      throw new TypeError("RPC defaultTarget=lambda requires lambdaBaseUrl");
    }
    this.defaultTarget = defaultTarget;

    // `Iterable` is deliberately accepted by the underlying client API, so it
    // may be a one-shot generator. Materialize once at this fan-out boundary;
    // constructing the standalone client must never consume inventory before
    // the Lambda client sees it. Freeze the snapshots so both children receive
    // the same stable values for their own Set construction.
    const operationList = Object.freeze([...operations]);
    const capabilityList = Object.freeze([...capabilities]);
    const common = {
      rpcPath,
      operations: operationList,
      fetchImpl,
      capabilities: capabilityList,
    };
    this.clients = new Map([
      [
        "standalone",
        new OresRpcUnaryClient({
          ...common,
          baseUrl: this.standaloneBaseUrl,
          ...(standaloneTransport === undefined ? {} : { transport: standaloneTransport }),
        }),
      ],
    ]);
    if (this.lambdaBaseUrl !== undefined) {
      this.clients.set(
        "lambda",
        new OresRpcUnaryClient({
          ...common,
          baseUrl: this.lambdaBaseUrl,
          ...(lambdaTransport === undefined ? {} : { transport: lambdaTransport }),
        }),
      );
    }
  }

  prepare(key, args = {}, endpoint = {}) {
    const target = selectEndpointTarget(endpoint, this.defaultTarget);
    const client = this.clients.get(target);
    if (!client) {
      throw new Error(`RPC endpoint target ${target} is not configured for ${String(key)}`);
    }
    return wrapBuilder(client.prepare(key, args), target);
  }

  call(key, args = {}, endpoint = {}) {
    return this.prepare(key, args, endpoint);
  }
}
