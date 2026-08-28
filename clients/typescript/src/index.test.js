import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import { inferMethods, inferTransports, parseRouteMap, route, expandPath, envelopeRouteMap, encodeCall, callToNdjson } from "./index.js";

const here = dirname(fileURLToPath(import.meta.url));
const example = JSON.parse(
  readFileSync(join(here, "../../../examples/pmap-api.route-map.json"), "utf8"),
);

test("example map validates and PascalCase is POST", () => {
  const map = parseRouteMap(example);
  assert.equal(map.map.CheckFieldSanity.path, "/pmap.v1.Interview/CheckFieldSanity");
  assert.deepEqual(inferMethods("CheckFieldSanity"), ["POST"]);
  assert.deepEqual(inferMethods("healthz"), ["GET"]);
  const b = map.map.CheckFieldSanity.binding;
  assert.ok(b.param_types.includes("CheckFieldSanityRequest"));
  assert.equal(b.return_type, "CheckFieldSanityResponse");
  assert.match(b.function_type, /UnaryFn/);
  assert.equal(b.annotation, "rpc_unary");
  assert.deepEqual(inferMethods("delete_matter"), ["DELETE"]);
  assert.equal(
    expandPath("/v1/matters/{id}", { id: "a/b" }),
    "/v1/matters/a%2Fb",
  );
  const env = envelopeRouteMap(map, "1689940800123456789");
  assert.equal(env.scope, "ores.api-docs.route-map");
  assert.equal(env.record_id, "pmap-api-server");
  assert.deepEqual(inferTransports("healthz", "/healthz"), ["http"]);
  assert.deepEqual(inferTransports("websocket", "/ws"), ["websocket"]);
  const frame = encodeCall({ id: "c1", key: "healthz", transport: "tcp" });
  assert.equal(frame.op, "call");
  assert.equal(callToNdjson(frame).endsWith("\n"), true);
});

test("typed route helper is a combination of key + path + methods", () => {
  const CheckFieldSanity = route("CheckFieldSanity", "/pmap.v1.Interview/CheckFieldSanity", ["POST"]);
  assert.equal(CheckFieldSanity.key, "CheckFieldSanity");
});

test("language-surface schema accepts annotation, types, or both", () => {
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  const schema = JSON.parse(
    readFileSync(join(here, "../../../json-schema/language-surface.schema.json"), "utf8"),
  );
  const validate = ajv.compile(schema);
  assert.equal(
    validate({
      language: "typescript",
      key: "CheckFieldSanity",
      annotation: "route(...)",
      param_types: ["CheckFieldSanityRequest"],
      return_type: "CheckFieldSanityResponse",
      function_type: "UnaryFn<CheckFieldSanityRequest, CheckFieldSanityResponse>",
    }),
    true,
    JSON.stringify(validate.errors),
  );
  assert.equal(validate({ language: "dart", key: "healthz" }), false);
});
