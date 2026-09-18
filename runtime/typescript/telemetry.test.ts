/** node --experimental-strip-types --test runtime/typescript/telemetry.test.ts */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  emit,
  emitError,
  type RpcErrorEvent,
  type RpcEvent,
  type RpcTelemetrySink,
} from "./telemetry.ts";

const ORES_TRACE_SEAM_TEST = "ores-trace-NSHCBePozab-SgMsHKztH";

function errorEvent(key: string): RpcErrorEvent {
  return {
    key,
    carrier: "http",
    outcome: "failed",
    kind: "decode",
    code: "body_decode_failed",
    oresTraceId: ORES_TRACE_SEAM_TEST,
  };
}

test("no sink swallows an error event", () => {
  emitError(undefined, errorEvent("walk_matter"));
});

test("an adapter written before error events is still a sink", () => {
  const seen: RpcEvent[] = [];
  const sink: RpcTelemetrySink = {
    emit(event) {
      seen.push(event);
    },
  };
  // No emitError on this adapter: the error is dropped, nothing throws, and
  // completed calls keep arriving.
  emitError(sink, errorEvent("walk_matter"));
  emit(sink, {
    key: "walk_matter",
    service: "demo",
    method: "POST",
    pathTemplate: "/v1/matters/{id}/walk",
    carrier: "http",
    outcome: "ok",
    durationMicros: 12,
  });
  assert.equal(seen.length, 1);
});

test("an error event reaches the sink with its static id", () => {
  const seen: RpcErrorEvent[] = [];
  const sink: RpcTelemetrySink = {
    emit() {},
    emitError(event) {
      seen.push(event);
    },
  };
  emitError(sink, errorEvent("walk_matter"));
  assert.equal(seen.length, 1);
  assert.equal(seen[0].oresTraceId, ORES_TRACE_SEAM_TEST);
  assert.match(seen[0].oresTraceId, /^ores-(trace|routine)-[A-Za-z0-9_-]{21}$/);
  assert.equal(seen[0].code, "body_decode_failed");
});

test("an error event carries no field that could hold a payload", () => {
  const keys = Object.keys(errorEvent("walk_matter")).sort();
  assert.deepEqual(keys, [
    "carrier",
    "code",
    "key",
    "kind",
    "oresTraceId",
    "outcome",
  ]);
});

test("a throwing error sink cannot replace the failure it describes", () => {
  const sink: RpcTelemetrySink = {
    emit() {},
    emitError() {
      throw new Error("exporter is down");
    },
  };
  emitError(sink, errorEvent("walk_matter"));
});

test("a rejecting error sink does not become an unhandled rejection", () => {
  const sink: RpcTelemetrySink = {
    emit() {
      return Promise.resolve();
    },
    emitError() {
      return Promise.reject(new Error("queue full"));
    },
  };
  emitError(sink, errorEvent("walk_matter"));
});

test("log-then-rethrow keeps the original error and its stack", () => {
  const seen: RpcErrorEvent[] = [];
  const sink: RpcTelemetrySink = {
    emit() {},
    emitError(event) {
      seen.push(event);
    },
  };
  const original = new Error("handler exploded");

  // The shape every generated dispatcher uses: log the failure through the
  // seam with the call site's static id, then rethrow the original.
  let thrown: unknown;
  try {
    try {
      throw original;
    } catch (error) {
      emitError(sink, {
        key: "walk_matter",
        carrier: "http",
        outcome: "failed",
        kind: "thrown",
        code: "handler_threw",
        oresTraceId: "ores-trace-bAeb2vDauN2gptd5DeVrL",
      });
      throw error;
    }
  } catch (error) {
    thrown = error;
  }

  assert.equal(thrown, original);
  assert.equal(seen.length, 1);
  assert.equal(seen[0].kind, "thrown");
  assert.equal(seen[0].oresTraceId, "ores-trace-bAeb2vDauN2gptd5DeVrL");
  // The message never crossed the seam.
  assert.equal(
    JSON.stringify(seen[0]).includes("handler exploded"),
    false,
  );
});
