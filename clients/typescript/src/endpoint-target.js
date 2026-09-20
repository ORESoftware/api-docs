const TARGETS = new Set(["default", "standalone", "lambda"]);
const ENDPOINT_FIELDS = new Set(["target", "lambda", "standalone"]);

export function requireEndpointUrl(name, value, { optional = false } = {}) {
  if (optional && value === undefined) return undefined;
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`RPC ${name} must be a non-empty string`);
  }
  return value;
}

/**
 * Normalize the local HTTP-ingress placement selector.
 *
 * Endpoint placement is intentionally client-local policy. It never enters the
 * RpcV1Call/request-plan wire envelope and `lambda` never means provider direct
 * invocation. The selector is a closed object so typos cannot silently fall
 * back to the default endpoint.
 */
export function selectEndpointTarget(endpoint = {}, defaultTarget = "standalone") {
  if (endpoint === null || typeof endpoint !== "object" || Array.isArray(endpoint)) {
    throw new TypeError("RPC endpoint selection must be an object");
  }
  for (const field of Object.keys(endpoint)) {
    if (!ENDPOINT_FIELDS.has(field)) {
      throw new TypeError(`unknown RPC endpoint selection field ${JSON.stringify(field)}`);
    }
  }
  for (const field of ["lambda", "standalone"]) {
    if (Object.hasOwn(endpoint, field) && typeof endpoint[field] !== "boolean") {
      throw new TypeError(`RPC endpoint ${field} selector must be boolean`);
    }
  }

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
  const selected = wantsLambda ? "lambda" : wantsStandalone ? "standalone" : explicit;
  return selected === "default" ? defaultTarget : selected;
}
