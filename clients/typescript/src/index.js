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

export function encodeCall({ id, key, transport, path, query, body, traceId, spanId }) {
  const frame = { v: 1, op: "call", id, key };
  if (transport) frame.transport = transport;
  if (path) frame.path = path;
  if (query) frame.query = query;
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
