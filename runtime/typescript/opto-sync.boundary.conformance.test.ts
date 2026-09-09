/**
 * Shared record-id and queued-body boundary vectors for the TypeScript port.
 * Rust consumes the same fixture in `runtime/rust/tests`.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  OptoSyncTransport,
  OptoSyncTransportError,
  segmentForParam,
  type RidlRequest,
} from "./opto-sync.ts";

const here = dirname(fileURLToPath(import.meta.url));
const fixture = JSON.parse(
  readFileSync(
    join(here, "../../examples/opto-sync/record-id-boundary.conformance.json"),
    "utf8",
  ),
) as {
  schemaVersion: number;
  profile: string;
  pathCases: Array<{
    name: string;
    encoded: string;
    valid: boolean;
    decoded?: string;
  }>;
  requestFieldCases: Array<{
    name: string;
    body: string;
    valid: boolean;
    decoded?: string;
  }>;
  upsertBodyCases: Array<{
    name: string;
    body: string;
    valid: boolean;
  }>;
};

function request(overrides: Partial<RidlRequest> = {}): RidlRequest {
  return {
    key: "widget_operation",
    method: "POST",
    path: "/v1/widgets/widget-42",
    pathTemplate: "/v1/widgets/{id}",
    query: [],
    body: '{"name":"ok"}',
    delivery: "opto_sync_queued",
    optoSync: {
      table: "widgets",
      operation: "upsert",
      recordId: { from: "path", name: "id" },
    },
    ...overrides,
  };
}

function spies() {
  let direct = 0;
  let queue = 0;
  let readback = 0;
  const transport = new OptoSyncTransport(
    {
      async call() {
        direct += 1;
        return "unexpected-direct";
      },
    },
    {
      async queueMutation(_table, _recordId, _payload) {
        queue += 1;
        return 1;
      },
      async queueDelete() {
        queue += 1;
        return 2;
      },
    },
    {
      async localJson(_table, recordId) {
        readback += 1;
        return recordId;
      },
    },
  );
  return {
    transport,
    counts: () => ({ direct, queue, readback }),
  };
}

assert.equal(fixture.schemaVersion, 1);
assert.equal(fixture.profile, "ridl-opto-sync-record-id-boundary-v1");

for (const vector of fixture.pathCases) {
  test(`path record id ${vector.name} follows the shared URI contract`, async () => {
    assert.equal(
      segmentForParam("/v1/widgets/{id}", `/v1/widgets/${vector.encoded}`, "id"),
      vector.valid ? vector.decoded : undefined,
    );

    const { transport, counts } = spies();
    const call = transport.call(
      request({ path: `/v1/widgets/${vector.encoded}` }),
    );

    if (vector.valid) {
      assert.equal(await call, vector.decoded);
      assert.deepEqual(counts(), { direct: 0, queue: 1, readback: 1 });
    } else {
      await assert.rejects(
        call,
        (error: unknown) =>
          error instanceof OptoSyncTransportError && error.reason === "not-queueable",
      );
      assert.deepEqual(counts(), { direct: 0, queue: 0, readback: 0 });
    }
  });
}

for (const vector of fixture.requestFieldCases) {
  test(`request-field record id ${vector.name} follows the shared JSON contract`, async () => {
    const { transport, counts } = spies();
    const call = transport.call(
      request({
        body: vector.body,
        optoSync: {
          table: "widgets",
          operation: "upsert",
          recordId: { from: "request", name: "id" },
        },
      }),
    );

    if (vector.valid) {
      assert.equal(await call, vector.decoded);
      assert.deepEqual(counts(), { direct: 0, queue: 1, readback: 1 });
    } else {
      await assert.rejects(
        call,
        (error: unknown) =>
          error instanceof OptoSyncTransportError && error.reason === "not-queueable",
      );
      assert.deepEqual(counts(), { direct: 0, queue: 0, readback: 0 });
    }
  });
}

for (const vector of fixture.upsertBodyCases) {
  test(`queued upsert body ${vector.name} is admitted only as a JSON object`, async () => {
    const { transport, counts } = spies();
    const call = transport.call(request({ body: vector.body }));

    if (vector.valid) {
      assert.equal(await call, "widget-42");
      assert.deepEqual(counts(), { direct: 0, queue: 1, readback: 1 });
    } else {
      await assert.rejects(
        call,
        (error: unknown) =>
          error instanceof OptoSyncTransportError && error.reason === "not-queueable",
      );
      assert.deepEqual(counts(), { direct: 0, queue: 0, readback: 0 });
    }
  });
}
