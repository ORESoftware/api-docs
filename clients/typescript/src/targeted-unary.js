import {
  OresRpcUnaryClient,
  RpcRemoteError,
} from "./fluent-unary.js";

const TARGETS = new Set(["default", "standalone", "lambda"]);

function requireUrl(name, value, optional = false) {
  if (optional && value === undefined) return undefined;
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`RPC ${name} must be a non-empty string`);
  }
  return value;
}

function selectTarget(endpoint = {}, defaultTarget = "standalone") {
  const explicit = endpoint.target ?? "default";
  if (!TARGETS.has(explicit)) {
    throw new TypeError(
      `RPC endpoint target must be default, standalone, or lambda; received ${String(explicit)}`,
    );
  }
  const wantsLambda = endpoint.lambda === true;
  const wantsStandalone = endpoint.standalone === true;
  if (wantsLambda && wantsStandalone) {
    throw new TypeError("RPC endpoint selection cannot enable lambda and standalone together");
  }
  if (wantsLambda && explicit !== "default" && explicit !== "lambda") {
    throw new TypeError("RPC endpoint target conflicts with lambda=true");
  }
  if (wantsStandalone && explicit !== "default" && explicit !== "standalone") {
    throw new TypeError("RPC endpoint target conflicts with standalone=true");
  }
  const target = wantsLambda ? "lambda" : wantsStandalone ? "standalone" : explicit;
  return target === "default" ? defaultTarget : target;
}

function wrapBuilder(builder, endpointTarget) {
  let proxy;
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

  proxy = new Proxy(builder, {
    get(target, property) {
      if (property === "endpointTarget") return endpointTarget;
      if (property === "makeCall") return terminal.makeCall;
      if (property === "makeCallOrThrow") return terminal.makeCallOrThrow;
      const value = Reflect.get(target, property, target);
      if (typeof value !== "function") return value;
      return (...args) => {
        const next = value.apply(target, args);
        // Fluent option methods return another immutable builder. Non-builder
        // methods such as toPlan() return ordinary values and must pass through.
        return next && typeof next === "object" && typeof next.makeCall === "function"
          ? wrapBuilder(next, endpointTarget)
          : next;
      };
    },
  });
  return proxy;
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
    this.standaloneBaseUrl = requireUrl("standaloneBaseUrl", standaloneBaseUrl);
    this.lambdaBaseUrl = requireUrl("lambdaBaseUrl", lambdaBaseUrl, true);
    if (defaultTarget !== "standalone" && defaultTarget !== "lambda") {
      throw new TypeError("RPC defaultTarget must be standalone or lambda");
    }
    if (defaultTarget === "lambda" && this.lambdaBaseUrl === undefined) {
      throw new TypeError("RPC defaultTarget=lambda requires lambdaBaseUrl");
    }
    this.defaultTarget = defaultTarget;

    const common = { rpcPath, operations, fetchImpl, capabilities };
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
    const target = selectTarget(endpoint, this.defaultTarget);
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
