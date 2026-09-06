/**
 * Byte-for-byte conformance and fail-closed tests for the RIDL frame port.
 *
 *   node --experimental-strip-types --test runtime/typescript/frame.conformance.test.ts
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  Correlator,
  MAX_FRAME_BYTES,
  type Frame,
  callFrame,
  cancelFrame,
  dataFrame,
  decode,
  decodeStream,
  encode,
  encodeTcp,
  endFrame,
  errorFrame,
  validate,
  withMeta,
} from "./frame.ts";

const here = dirname(fileURLToPath(import.meta.url));
const fixtures = JSON.parse(
  readFileSync(join(here, "../../examples/frames/conformance.json"), "utf8"),
) as {
  frame_version: number;
  cases: Array<{ name: string; encoded: string; tcp_prefix_hex: string }>;
};

const build: Record<string, () => Frame> = {
  "call-minimal": () => callFrame("1", "healthz", "GET", "/healthz"),
  "call-full": () =>
    callFrame(
      "c7-2",
      "walk_matter",
      "POST",
      "/v1/matters/abc/walk",
      [
        ["include_facts", "true"],
        ["kinds", "branch"],
      ],
      { value: { choice_id: "c", answers: {}, confirm_override: false } },
    ),
  "call-unicode-path": () =>
    callFrame("3", "get_matter", "GET", "/v1/matters/caf%C3%A9"),
  "call-unicode-body": () =>
    callFrame("4", "create_note", "POST", "/v1/notes", [], {
      value: { text: "café — ok" },
    }),
  "data-object": () => dataFrame("1", { ok: true, n: 3 }),
  "data-null": () => dataFrame("1", null),
  "data-scalar": () => dataFrame("1", "plain string"),
  end: () => endFrame("1"),
  cancel: () => cancelFrame("c7-2"),
  "error-minimal": () => errorFrame("1", "503"),
  "error-message": () => errorFrame("1", "404", "no such matter"),
  "call-with-meta": () =>
    withMeta(
      withMeta(
        callFrame("5", "healthz", "GET", "/healthz"),
        "authorization",
        "Bearer x",
      ),
      "traceparent",
      "00-a-b-01",
    ),
};

const decoder = new TextDecoder();
const hex = (bytes: Uint8Array): string =>
  [...bytes].map((value) => value.toString(16).padStart(2, "0")).join("");

assert.equal(fixtures.frame_version, 1);

for (const fixture of fixtures.cases) {
  test(`encodes ${fixture.name} to the canonical bytes`, () => {
    const make = build[fixture.name];
    assert.ok(make, `no builder for fixture ${fixture.name}`);
    assert.equal(decoder.decode(encode(make())), fixture.encoded);
  });

  test(`decodes ${fixture.name} back to canonical bytes`, () => {
    const round = decode(fixture.encoded);
    assert.equal(decoder.decode(encode(round)), fixture.encoded);
  });

  test(`length-prefixes ${fixture.name} identically`, () => {
    const tcp = encodeTcp(build[fixture.name]!());
    assert.equal(hex(tcp.subarray(0, 4)), fixture.tcp_prefix_hex);
  });
}

test("an absent body and a null body stay distinguishable", () => {
  assert.equal(decode('{"v":1,"id":"1","t":"end"}').hasBody, false);
  const nullBody = decode('{"v":1,"id":"1","t":"data","body":null}');
  assert.equal(nullBody.hasBody, true);
  assert.equal(nullBody.body, null);
});

test("encode rejects runtime values TypeScript types cannot protect at JS boundaries", () => {
  assert.throws(
    () => validate({ ...endFrame("1"), t: "unknown" as never }),
    /unknown frame type/,
  );
  assert.throws(() => encode(dataFrame("1", undefined)), /undefined/);
  assert.throws(
    () =>
      validate({
        ...callFrame("1", "healthz", "GET", "/healthz"),
        query: [["name", 7] as never],
      }),
    /query entry/,
  );
  assert.throws(
    () =>
      validate({
        ...endFrame("1"),
        meta: [
          ["traceparent", "first"],
          ["traceparent", "second"],
        ],
      }),
    /duplicate meta member/,
  );
});

test("withMeta replaces a name and emits sorted object members", () => {
  const frame = withMeta(withMeta(withMeta(endFrame("1"), "z", "last"), "a", "first"), "z", "new");
  assert.equal(
    decoder.decode(encode(frame)),
    '{"v":1,"id":"1","t":"end","meta":{"a":"first","z":"new"}}',
  );
});

test("decode rejects unknown fields, wrapped versions, and invalid UTF-8", () => {
  assert.throws(
    () => decode('{"v":1,"id":"1","t":"end","deadline":"5s"}'),
    /unknown frame member/,
  );
  assert.throws(() => decode('{"v":257,"id":"1","t":"end"}'), /version/);
  assert.throws(() => decode(new Uint8Array([0xff])), /UTF-8/);
});

test("string input is subject to the same byte limit as bytes", () => {
  assert.throws(() => decode("x".repeat(MAX_FRAME_BYTES + 1)), /over the .* limit/);
});

test("a corrupt length prefix cannot force a huge allocation", () => {
  const buffer = new Uint8Array(4);
  new DataView(buffer.buffer).setUint32(0, 0xffffffff, false);
  assert.throws(() => decodeStream(buffer), /over the .* limit/);
});

test("a partial tail is left for the next read", () => {
  const first = encodeTcp(callFrame("1", "healthz", "GET", "/healthz"));
  const second = encodeTcp(endFrame("1"));
  const buffer = new Uint8Array(first.length + 3);
  buffer.set(first, 0);
  buffer.set(second.subarray(0, 3), first.length);
  const { frames, rest } = decodeStream(buffer);
  assert.equal(frames.length, 1);
  assert.equal(rest.length, 3);
});

test("correlation ids are monotonic and not content-derived", () => {
  const correlator = new Correlator("c7-");
  assert.deepEqual(
    [correlator.take(), correlator.take(), correlator.take()],
    ["c7-1", "c7-2", "c7-3"],
  );
});
