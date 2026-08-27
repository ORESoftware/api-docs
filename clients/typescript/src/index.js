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
  if (
    lower.includes("create") ||
    lower.includes("walk") ||
    lower.includes("check") ||
    lower.includes("ask") ||
    lower.startsWith("post")
  ) {
    return ["POST"];
  }
  return ["GET"];
}

export function lookup(map, key) {
  return map.map[key];
}
