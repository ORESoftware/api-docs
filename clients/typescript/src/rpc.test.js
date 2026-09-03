import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  Correlator,
  RpcV1Error,
  assertReceiptForCall,
  callFromNdjson,
  decodeCall,
  decodeReceipt,
  encodeCall,
  encodeLengthPrefixed,
  encodeReceipt,
  splitLengthPrefixed,
  toNdjson,
} from "./rpc.js";

const here = dirname(fileURLToPath(import.meta.url));
const fixtures = JSON.parse(
  readFileSync(join(here, "../../../examples/rpc-v1/conformance.json"), "utf8"),
);
const hex = (bytes) =>
  [...bytes].map((value) => value.toString(16).padStart(2, "0")).join("");

test("fixture profile is v1 and not RIDL v2", () => {
  assert.equal(fixtures.profile, "ores-rpc-v1-call-receipt");
  assert.equal(fixtures.schemaVersion, 1);
});

for (const fixture of fixtures.valid) {
  test(`${fixture.name} round-trips exact shared bytes`, () => {
    const envelope = fixture.kind === "call"
      ? decodeCall(fixture.encoded)
      : decodeReceipt(fixture.encoded);
    assert.equal(toNdjson(envelope).slice(0, -1), fixture.encoded);
    assert.equal(
      hex(encodeLengthPrefixed(envelope).subarray(0, 4)),
      fixture.tcp_prefix_hex,
    );
  });
}

for (const fixture of fixtures.invalid) {
  test(`${fixture.name} fails closed`, () => {
    assert.throws(
      () => fixture.kind === "call"
        ? decodeCall(fixture.encoded)
        : decodeReceipt(fixture.encoded),
      RpcV1Error,
    );
  });
}

test("constructors enforce the receipt state machine", () => {
  const call = encodeCall({ id: "c1", key: "healthz", transport: "tcp" });
  const success = encodeReceipt({
    id: "c1",
    key: "healthz",
    transport: "tcp",
    ok: true,
    status: 200,
    body: null,
  });
  assert.equal(success.body, null);
  assert.equal(assertReceiptForCall(call, success), success);
  assert.throws(
    () => encodeReceipt({ id: "c1", key: "healthz", ok: false, status: 500 }),
    /needs error/,
  );
  assert.throws(
    () => encodeReceipt({
      id: "c1",
      key: "healthz",
      ok: true,
      status: 200,
      error: { code: "bad" },
    }),
    /must not carry error/,
  );
});

test("correlation mismatches fail closed", () => {
  const call = encodeCall({ id: "c1", key: "healthz", transport: "tcp" });
  const receipt = encodeReceipt({
    id: "c2",
    key: "healthz",
    transport: "tcp",
    ok: true,
    status: 200,
  });
  assert.throws(() => assertReceiptForCall(call, receipt), /id does not match/);
});

test("NDJSON accepts one terminator and rejects multiple objects", () => {
  assert.equal(
    callFromNdjson('{"v":1,"op":"call","id":"c1","key":"healthz"}\r\n').id,
    "c1",
  );
  assert.throws(
    () => callFromNdjson(
      '{"v":1,"op":"call","id":"c1","key":"healthz"}\n{}\n',
    ),
    /exactly one JSON object/,
  );
});

test("length-prefix decoder rejects oversized declarations and keeps a tail", () => {
  const first = encodeLengthPrefixed(encodeCall({ id: "c1", key: "healthz" }));
  const second = encodeLengthPrefixed(encodeReceipt({
    id: "c1",
    key: "healthz",
    ok: true,
    status: 200,
  }));
  const buffer = new Uint8Array(first.length + 3);
  buffer.set(first);
  buffer.set(second.subarray(0, 3), first.length);
  const { frames, rest } = splitLengthPrefixed(buffer);
  assert.equal(frames.length, 1);
  assert.equal(rest.length, 3);
  assert.throws(
    () => splitLengthPrefixed(new Uint8Array([0xff, 0xff, 0xff, 0xff])),
    /over the .* limit/,
  );
});

test("invalid UTF-8, unknown members, symbols, and cycles fail closed", () => {
  assert.throws(() => decodeCall(new Uint8Array([0xff])), /UTF-8/);
  const symbol = Symbol("hidden");
  assert.throws(
    () => encodeCall({ id: "c1", key: "healthz", [symbol]: true }),
    /member names must be strings/,
  );
  const cyclic = {};
  cyclic.self = cyclic;
  assert.throws(
    () => encodeCall({ id: "c1", key: "healthz", body: cyclic }),
    /cyclic/,
  );
});

test("correlation identifiers are monotonic and bounded", () => {
  const correlator = new Correlator("request-");
  assert.deepEqual([correlator.take(), correlator.take()], ["request-1", "request-2"]);
  assert.throws(() => new Correlator("\ud800"), /Unicode scalar/);
});
