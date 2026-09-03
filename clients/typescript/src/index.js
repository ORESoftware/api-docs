/**
 * TypeScript surface for the same route map.
 *
 * A key can be an annotation-like const (`route(...)`), param + return types
 * on a function, a named function type (`UnaryFn<Req, Res>`), or a combination.
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

const here = dirname(fileURLToPath(import.meta.url));
const schemaRoot = join(here, "../../../json-schema");

export const SCHEMA_VERSION = "1.0.0";
export const OPTO_SYNC_SCOPE = "ores.api-docs.route-map";

export function route(key, path, methods) {
  return Object.freeze({ key, path, methods });
}

export const apiDocs = Object.freeze({
  createMatter: route("create_matter", "/v1/matters", ["POST"]),
  getMatter: route("get_matter", "/v1/matters/{matterId}", ["GET"]),
  updateMatter: route("update_matter", "/v1/matters/{matterId}", ["PATCH"]),
  walkMatter: route("walk_matter", "/v1/matters/{matterId}/walk", ["POST"]),
});

const routeMapSchema = JSON.parse(
  readFileSync(join(schemaRoot, "route-map.schema.json"), "utf8"),
);
const rpcCallSchema = JSON.parse(
  readFileSync(join(schemaRoot, "rpc-call.schema.json"), "utf8"),
);
const rpcReceiptSchema = JSON.parse(
  readFileSync(join(schemaRoot, "rpc-receipt.schema.json"), "utf8"),
);

const ajv = new Ajv2020({ allErrors: true, strict: true });
const validators = Object.freeze({
  routeMap: ajv.compile(routeMapSchema),
  rpcCall: ajv.compile(rpcCallSchema),
  rpcReceipt: ajv.compile(rpcReceiptSchema),
});

export function validateRouteMap(value) {
  if (validators.routeMap(value)) return value;
  throw validationError("route-map", validators.routeMap.errors);
}

export function validateRpcCall(value) {
  if (validators.rpcCall(value)) return value;
  throw validationError("rpc-call", validators.rpcCall.errors);
}

export function validateRpcReceipt(value) {
  if (validators.rpcReceipt(value)) return value;
  throw validationError("rpc-receipt", validators.rpcReceipt.errors);
}

export function loadRouteMap(path) {
  return validateRouteMap(JSON.parse(readFileSync(path, "utf8")));
}

export class ApiDocsClient {
  constructor({ baseUrl, fetchImpl = globalThis.fetch, routeMap = { map: apiDocs } }) {
    if (!baseUrl) throw new TypeError("baseUrl is required");
    if (typeof fetchImpl !== "function") throw new TypeError("fetchImpl must be a function");
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.fetchImpl = fetchImpl;
    this.routeMap = normalizeRouteMap(routeMap);
  }

  async call(key, options = {}) {
    const operation = this.routeMap[key];
    if (!operation) throw new Error(`unknown operation key: ${key}`);

    const method = options.method ?? operation.methods[0];
    if (!operation.methods.includes(method)) {
      throw new Error(`method ${method} is not allowed for ${key}`);
    }

    const path = fillPath(operation.path, options.path ?? {});
    const url = new URL(`${this.baseUrl}${path}`);
    for (const [name, value] of Object.entries(options.query ?? {})) {
      if (value === undefined || value === null) continue;
      if (Array.isArray(value)) {
        for (const item of value) url.searchParams.append(name, String(item));
      } else {
        url.searchParams.set(name, String(value));
      }
    }

    const headers = { accept: "application/json", ...(options.headers ?? {}) };
    const init = { method, headers };
    if (options.body !== undefined) {
      headers["content-type"] ??= "application/json";
      init.body = JSON.stringify(options.body);
    }

    const response = await this.fetchImpl(url, init);
    const contentType = response.headers.get("content-type") ?? "";
    const body = contentType.includes("application/json")
      ? await response.json()
      : await response.text();

    if (!response.ok) {
      const error = new Error(`API call ${key} failed with ${response.status}`);
      error.status = response.status;
      error.body = body;
      throw error;
    }

    return { status: response.status, body };
  }

  createMatter(body, options = {}) {
    return this.call("create_matter", { ...options, method: "POST", body });
  }

  getMatter(matterId, options = {}) {
    return this.call("get_matter", {
      ...options,
      method: "GET",
      path: { ...(options.path ?? {}), matterId },
    });
  }

  updateMatter(matterId, body, options = {}) {
    return this.call("update_matter", {
      ...options,
      method: "PATCH",
      path: { ...(options.path ?? {}), matterId },
      body,
    });
  }

  walkMatter(matterId, body, options = {}) {
    return this.call("walk_matter", {
      ...options,
      method: "POST",
      path: { ...(options.path ?? {}), matterId },
      body,
    });
  }
}

export function createClient(options) {
  return new ApiDocsClient(options);
}

function normalizeRouteMap(document) {
  const parsed = validateRouteMap(document);
  const aliases = parsed.aliases ?? {};
  const resolved = { ...parsed.map };
  for (const [alias, target] of Object.entries(aliases)) {
    if (!resolved[target]) throw new Error(`alias ${alias} points at unknown operation ${target}`);
    resolved[alias] = resolved[target];
  }
  return resolved;
}

function fillPath(template, params) {
  const consumed = new Set();
  const value = template.replace(/\{([A-Za-z][A-Za-z0-9_]*)\}/g, (_match, name) => {
    if (!(name in params)) throw new Error(`missing path parameter: ${name}`);
    consumed.add(name);
    return encodeURIComponent(String(params[name]));
  });
  const unknown = Object.keys(params).filter((name) => !consumed.has(name));
  if (unknown.length) throw new Error(`unknown path parameter(s): ${unknown.join(", ")}`);
  return value;
}

function validationError(name, errors) {
  const detail = (errors ?? [])
    .map((entry) => `${entry.instancePath || "/"} ${entry.message}`)
    .join("; ");
  return new TypeError(`${name} validation failed: ${detail}`);
}

export {
  Correlator as RpcV1Correlator,
  LENGTH_PREFIX_BYTES as RPC_V1_LENGTH_PREFIX_BYTES,
  MAX_FRAME_BYTES as RPC_V1_MAX_FRAME_BYTES,
  RPC_VERSION as RPC_V1_VERSION,
  RpcV1Error,
  assertReceiptForCall,
  callFromNdjson,
  callToNdjson,
  decodeCall,
  decodeReceipt,
  encodeCall,
  encodeLengthPrefixed,
  encodeReceipt,
  receiptFromNdjson,
  receiptToNdjson,
  splitLengthPrefixed,
  toNdjson,
  validateCall as validateRpcV1Call,
  validateReceipt as validateRpcV1Receipt,
} from "./rpc.js";
