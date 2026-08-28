import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  callToNdjson,
  compileValidator,
  encodeCall,
  encodeReceipt,
  envelopeRouteMap,
  expandPath,
  inferTransports,
  parseRouteMap,
} from "./index.js";

const here = dirname(fileURLToPath(import.meta.url));
const example = JSON.parse(
  readFileSync(join(here, "../../../examples/rpc-transports.route-map.json"), "utf8"),
);

const validateCall = compileValidator("rpc-call.schema.json");
const validateReceipt = compileValidator("rpc-receipt.schema.json");
const validateTelemetry = compileValidator("telemetry-attributes.schema.json");
const validateEnvelope = compileValidator("opto-sync-envelope.schema.json");

test("get_item call/receipt/telemetry are valid on http, tcp, and websocket", () => {
  const map = parseRouteMap(example);
  assert.deepEqual(map.map.get_item.transports, ["http", "tcp", "websocket"]);
  assert.equal(expandPath(map.map.get_item.path, { id: "item-42" }), "/v1/items/item-42");
  assert.deepEqual(inferTransports("websocket", "/ws"), ["websocket"]);

  const bodies = [];
  for (const transport of ["http", "tcp", "websocket"]) {
    const call = encodeCall({
      id: `${transport}-get-item`,
      key: "get_item",
      transport,
      path: { id: "item-42" },
      traceId: "4bf92f3577b34da6a3ce929d0e0e4736",
      spanId: "00f067aa0ba902b7",
    });
    assert.equal(validateCall(call), true, JSON.stringify(validateCall.errors));
    const line = callToNdjson(call);
    assert.equal(line.endsWith("\n"), true);
    const back = JSON.parse(line);
    assert.equal(validateCall(back), true);
    const receipt = encodeReceipt({
      id: call.id,
      key: call.key,
      transport,
      ok: true,
      status: 200,
      body: { id: "item-42", name: "item-item-42" },
      traceId: call.traceId,
      spanId: call.spanId,
    });
    assert.equal(validateReceipt(receipt), true, JSON.stringify(validateReceipt.errors));
    bodies.push(receipt.body);
    const fields = {
      "rpc.system": "ores-api-docs",
      "rpc.service": map.service,
      "rpc.method": "get_item",
      "rpc.transport": transport,
      "rpc.ok": true,
      "http.status_code": 200,
    };
    assert.equal(validateTelemetry(fields), true, JSON.stringify(validateTelemetry.errors));
    const log = { fields, traceId: call.traceId, spanId: call.spanId };
    assert.equal(log.fields["rpc.system"], "ores-api-docs");
    assert.match(log.traceId, /^[0-9a-f]{32}$/);
  }
  assert.deepEqual(bodies[0], bodies[1]);
  assert.deepEqual(bodies[1], bodies[2]);
});

test("opto-sync envelope carries the route map, not an rpc-call", () => {
  const map = parseRouteMap(example);
  const env = envelopeRouteMap(map, "1689940800123456789");
  assert.equal(validateEnvelope(env), true, JSON.stringify(validateEnvelope.errors));
  assert.equal(env.payload.map.get_item.path, "/v1/items/{id}");
  const stuffed = { ...env, payload: encodeCall({ id: "nope", key: "get_item" }) };
  assert.equal(validateEnvelope(stuffed), false);
});
