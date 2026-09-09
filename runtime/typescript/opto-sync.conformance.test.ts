/**
 * Cross-runtime Opto-Sync transport conformance.
 *
 * The minted-id vectors are shared with the Rust integration suite so a
 * language port cannot silently choose a different hash width or string
 * encoding. Failure-path tests also keep direct, queue, and readback semantics
 * aligned with `runtime/rust/opto_sync.rs`.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  OptoSyncTransport,
  OptoSyncTransportError,
  mintRecordId,
  type DirectTransport,
  type LocalReadback,
  type MutationQueue,
  type RidlRequest,
} from "./opto-sync.ts";

const here = dirname(fileURLToPath(import.meta.url));
const vectors = JSON.parse(
  readFileSync(
    join(here, "../../examples/opto-sync/minted-id.conformance.json"),
    "utf8",
  ),
) as {
  schemaVersion: number;
  profile: string;
  algorithm: string;
  inputEncoding: string;
  cases: Array<{
    name: string;
    key: string;
    path: string;
    body: string;
    expected: string;
  }>;
};

function request(
  overrides: Partial<RidlRequest> = {},
): RidlRequest {
  return {
    key: "widget_operation",
    method: "POST",
    path: "/v1/widgets/widget-42",
    pathTemplate: "/v1/widgets/{id}",
    query: [],
    body: '{"name":"offline edit"}',
    delivery: "opto_sync_queued",
    optoSync: {
      table: "widgets",
      operation: "upsert",
      recordId: { from: "path", name: "id" },
    },
    ...overrides,
  };
}

assert.equal(vectors.schemaVersion, 1);
assert.equal(vectors.profile, "ridl-opto-sync-minted-record-id-v1");
assert.equal(vectors.algorithm, "fnv1a-64");
assert.equal(vectors.inputEncoding, "utf-8");

for (const vector of vectors.cases) {
  test(`mints ${vector.name} identically to the canonical cross-runtime vector`, () => {
    const actual = mintRecordId(
      request({
        key: vector.key,
        path: vector.path,
        pathTemplate: vector.path,
        body: vector.body,
        optoSync: {
          table: "widgets",
          operation: "upsert",
          recordId: { from: "minted" },
        },
      }),
    );
    assert.equal(actual, vector.expected);
  });
}

test("queued delivery without Opto-Sync metadata stays direct", async () => {
  let directCalls = 0;
  let queueCalls = 0;
  let readbackCalls = 0;
  const direct: DirectTransport = {
    async call() {
      directCalls += 1;
      return "authoritative";
    },
  };
  const queue: MutationQueue = {
    async queueMutation() {
      queueCalls += 1;
      return 1;
    },
    async queueDelete() {
      queueCalls += 1;
      return 2;
    },
  };
  const readback: LocalReadback = {
    async localJson() {
      readbackCalls += 1;
      return "local";
    },
  };

  const transport = new OptoSyncTransport(direct, queue, readback);
  const result = await transport.call(
    request({ delivery: "opto_sync_queued", optoSync: undefined }),
  );

  assert.equal(result, "authoritative");
  assert.equal(directCalls, 1);
  assert.equal(queueCalls, 0);
  assert.equal(readbackCalls, 0);
});

test("queue failure is classified and never falls back to direct or readback", async () => {
  let directCalls = 0;
  let readbackCalls = 0;
  const transport = new OptoSyncTransport(
    {
      async call() {
        directCalls += 1;
        return "unexpected-direct";
      },
    },
    {
      async queueMutation() {
        throw new Error("disk full");
      },
      async queueDelete() {
        throw new Error("disk full");
      },
    },
    {
      async localJson() {
        readbackCalls += 1;
        return "unexpected-local";
      },
    },
  );

  await assert.rejects(
    transport.call(request()),
    (error: unknown) =>
      error instanceof OptoSyncTransportError && error.reason === "queue-failed",
  );
  assert.equal(directCalls, 0);
  assert.equal(readbackCalls, 0);
});

test("readback failure is classified after durable queueing and never falls back", async () => {
  let directCalls = 0;
  let queued = 0;
  const transport = new OptoSyncTransport(
    {
      async call() {
        directCalls += 1;
        return "unexpected-direct";
      },
    },
    {
      async queueMutation() {
        queued += 1;
        return 1;
      },
      async queueDelete() {
        queued += 1;
        return 2;
      },
    },
    {
      async localJson() {
        throw new Error("sqlite read failed");
      },
    },
  );

  await assert.rejects(
    transport.call(request()),
    (error: unknown) =>
      error instanceof OptoSyncTransportError && error.reason === "readback-failed",
  );
  assert.equal(queued, 1);
  assert.equal(directCalls, 0);
});

test("queued delete uses only the tombstone path before local readback", async () => {
  let mutations = 0;
  let deletes = 0;
  let readbacks = 0;
  const transport = new OptoSyncTransport(
    {
      async call() {
        return "unexpected-direct";
      },
    },
    {
      async queueMutation() {
        mutations += 1;
        return 1;
      },
      async queueDelete(table, recordId) {
        deletes += 1;
        assert.equal(table, "widgets");
        assert.equal(recordId, "widget-42");
        return 2;
      },
    },
    {
      async localJson(table, recordId) {
        readbacks += 1;
        assert.equal(table, "widgets");
        assert.equal(recordId, "widget-42");
        return '{"id":"widget-42","deleted":true}';
      },
    },
  );

  const result = await transport.call(
    request({
      body: undefined,
      optoSync: {
        table: "widgets",
        operation: "delete",
        recordId: { from: "path", name: "id" },
      },
    }),
  );

  assert.equal(result, '{"id":"widget-42","deleted":true}');
  assert.equal(mutations, 0);
  assert.equal(deletes, 1);
  assert.equal(readbacks, 1);
});
