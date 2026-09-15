import {
  Correlator,
  RpcV1Error,
  assertReceiptForCall,
  encodeCall,
} from "./rpc.js";

const has = (object, name) => Object.prototype.hasOwnProperty.call(object, name);
const TRANSPORTS = new Set(["http", "tcp", "websocket", "nats"]);

function fail(message) {
  throw new RpcV1Error(message);
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Bind one generated route table to one concrete wire transport.
 *
 * The operation key selects the route metadata. HTTP method/path are binding
 * data delivered to the adapter; TCP/WebSocket/NATS carry the same typed call
 * frame without introducing a second route switch.
 */
export function createTypedRpcClient(config) {
  if (!isObject(config)) fail("typed RPC client config must be an object");

  const { routes, transport, invoke } = config;
  if (!isObject(routes)) fail("typed RPC routes must be an object");
  if (!TRANSPORTS.has(transport)) {
    fail(`unknown typed RPC transport ${String(transport)}`);
  }
  if (typeof invoke !== "function") {
    fail("typed RPC invoke must be a function");
  }
  if (has(config, "validateResponse") && typeof config.validateResponse !== "function") {
    fail("typed RPC validateResponse must be a function when provided");
  }

  const correlator = new Correlator(config.correlationPrefix ?? "");

  return Object.freeze({
    transport,

    async call(key, args = {}) {
      if (typeof key !== "string" || !has(routes, key)) {
        fail(`unknown RPC operation ${String(key)}`);
      }
      if (!isObject(args)) fail("typed RPC call args must be an object");

      const route = routes[key];
      if (!isObject(route) || route.key !== key) {
        fail(`route metadata mismatch for ${key}`);
      }
      if (!Array.isArray(route.transports) || !route.transports.includes(transport)) {
        fail(`RPC operation ${key} does not declare transport ${transport}`);
      }
      if (!Array.isArray(route.methods) || route.methods.length === 0) {
        fail(`RPC operation ${key} has no declared method binding`);
      }

      let selectedMethod;
      if (has(args, "method")) {
        if (typeof args.method !== "string" || !route.methods.includes(args.method)) {
          fail(`RPC operation ${key} does not declare method ${String(args.method)}`);
        }
        selectedMethod = args.method;
      } else if (transport === "http") {
        if (route.methods.length !== 1) {
          fail(
            `RPC operation ${key} has ${route.methods.length} HTTP methods; choose method explicitly`,
          );
        }
        [selectedMethod] = route.methods;
      }

      const input = {
        id: has(args, "id") ? args.id : correlator.take(),
        key,
        transport,
      };
      for (const name of ["path", "query", "headers", "body", "traceId", "spanId"]) {
        if (has(args, name)) input[name] = args[name];
      }

      const call = encodeCall(input);
      const receipt = assertReceiptForCall(
        call,
        await invoke(
          call,
          Object.freeze({
            key,
            path: route.path,
            methods: route.methods,
            selectedMethod,
            transports: route.transports,
          }),
        ),
      );

      if (!receipt.ok) {
        const code =
          isObject(receipt.error) && typeof receipt.error.code === "string"
            ? ` (${receipt.error.code})`
            : "";
        fail(`RPC operation ${key} failed${code}`);
      }

      return config.validateResponse
        ? config.validateResponse(key, receipt.body)
        : receipt.body;
    },
  });
}
