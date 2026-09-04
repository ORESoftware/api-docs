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

function loadSchema(name) {
  return JSON.parse(readFileSync(join(schemaRoot, name), "utf8"));
}

export function compileValidator(schemaName = "route-map.schema.json") {
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  const schema = loadSchema(schemaName);
  return ajv.compile(schema);
}

export function parseRouteMap(json) {
  const value = typeof json === "string" ? JSON.parse(json) : json;
  const validate = compileValidator();
  if (!validate(value)) {
    const msg = (validate.errors || [])
      .map((e) => `${e.instancePath} ${e.message}`)
      .join("; ");
    throw new Error(`route-map schema: ${msg}`);
  }
  return value;
}

export function inferMethods(key) {
  if (/^[A-Z]/.test(key)) return ["POST"];
  const lower = key.toLowerCase();
  if (lower.startsWith("delete")) return ["DELETE"];
  if (lower.startsWith("put") || lower.startsWith("update") || lower.startsWith("replace")) {
    return ["PUT"];
  }
  if (lower.startsWith("patch")) return ["PATCH"];
  if (
    lower.includes("create") ||
    lower.includes("walk") ||
    lower.includes("check") ||
    lower.includes("ask") ||
    lower.startsWith("post") ||
    lower.startsWith("submit")
  ) {
    return ["POST"];
  }
  return ["GET"];
}

export function inferTransports(key, path) {
  const lower = String(key || "").toLowerCase();
  if (path === "/ws" || path === "/websocket" || lower.includes("websocket")) {
    return ["websocket"];
  }
  return ["http"];
}

export function encodeCall({ id, key, transport, path, query, headers, body, traceId, spanId }) {
  const frame = { v: 1, op: "call", id, key };
  if (transport) frame.transport = transport;
  if (path) frame.path = path;
  if (query) frame.query = query;
  if (headers) frame.headers = headers;
  if (body !== undefined) frame.body = body;
  if (traceId) frame.traceId = traceId;
  if (spanId) frame.spanId = spanId;
  return frame;
}

export function encodeReceipt({ id, key, ok, status, body, error, transport, traceId, spanId }) {
  const frame = { v: 1, op: "receipt", id, key, ok };
  if (transport) frame.transport = transport;
  if (status !== undefined) frame.status = status;
  if (body !== undefined) frame.body = body;
  if (error !== undefined) frame.error = error;
  if (traceId) frame.traceId = traceId;
  if (spanId) frame.spanId = spanId;
  return frame;
}

export function callToNdjson(frame) {
  return `${JSON.stringify(frame)}\n`;
}

export const MAX_FRAME_BYTES = 8 * 1024 * 1024;

export function encodeLengthPrefixed(frame) {
  const payload = Buffer.from(JSON.stringify(frame), "utf8");
  if (payload.length > MAX_FRAME_BYTES) {
    throw new Error(`declared frame length ${payload.length} is over the ${MAX_FRAME_BYTES} limit`);
  }
  const out = Buffer.alloc(4 + payload.length);
  out.writeUInt32BE(payload.length, 0);
  payload.copy(out, 4);
  return new Uint8Array(out);
}

export function splitLengthPrefixed(buf) {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  const frames = [];
  let offset = 0;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  while (bytes.length - offset >= 4) {
    const len = view.getUint32(offset, false);
    if (len > MAX_FRAME_BYTES) {
      throw new Error(`declared frame length ${len} is over the ${MAX_FRAME_BYTES} limit`);
    }
    const start = offset + 4;
    if (bytes.length - start < len) {
      break;
    }
    frames.push(bytes.subarray(start, start + len));
    offset = start + len;
  }
  return { frames, rest: bytes.subarray(offset) };
}

export function pathTemplateVars(path) {
  const vars = [];
  const re = /\{([A-Za-z_][A-Za-z0-9_]*)\}/g;
  let match;
  while ((match = re.exec(path))) {
    vars.push(match[1]);
  }
  return vars;
}

export function expandPath(template, params) {
  const vars = pathTemplateVars(template);
  const keys = Object.keys(params);
  if (vars.length !== keys.length || vars.some((v) => !keys.includes(v))) {
    throw new Error(`path params mismatch for ${template}`);
  }
  return template.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_, name) =>
    encodeURIComponent(String(params[name])),
  );
}

export function envelopeRouteMap(map, updatedAt) {
  const recordId = map.service;
  return {
    id: recordId,
    scope: "ores.api-docs.route-map",
    kind: "ores.api-docs.route-map",
    record_id: recordId,
    updatedAt,
    payload: map,
  };
}

export function lookup(map, key) {
  return map.map[key];
}

// The browser-safe strict codec has a dedicated `./rpc` package export. These
// prefixed aliases make it discoverable from the existing Node route-map
// entrypoint without replacing or silently changing any legacy v1 function.
export {
  Correlator as RpcV1Correlator,
  LENGTH_PREFIX_BYTES as RPC_V1_LENGTH_PREFIX_BYTES,
  MAX_FRAME_BYTES as RPC_V1_MAX_FRAME_BYTES,
  RPC_VERSION as RPC_V1_VERSION,
  RpcV1Error,
  assertReceiptForCall as assertRpcV1ReceiptForCall,
  callFromNdjson as rpcV1CallFromNdjson,
  callToNdjson as rpcV1CallToNdjson,
  decodeCall as decodeRpcV1Call,
  decodeReceipt as decodeRpcV1Receipt,
  encodeCall as encodeRpcV1Call,
  encodeLengthPrefixed as encodeRpcV1LengthPrefixed,
  encodeReceipt as encodeRpcV1Receipt,
  receiptFromNdjson as rpcV1ReceiptFromNdjson,
  receiptToNdjson as rpcV1ReceiptToNdjson,
  splitLengthPrefixed as splitRpcV1LengthPrefixed,
  toNdjson as rpcV1ToNdjson,
  validateCall as validateRpcV1Call,
  validateReceipt as validateRpcV1Receipt,
} from "./rpc.js";
