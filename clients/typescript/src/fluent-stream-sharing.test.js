import assert from "node:assert/strict";
import test from "node:test";

import { OresRpcStreamClient } from "./fluent-stream.js";

const KEY = "demo.events.watch_events";
const ID = `ores-${KEY}`;

function nextTurn() {
  return new Promise((resolve) => setImmediate(resolve));
}

test("observable and async-iterator views share one upstream carrier", async () => {
  let opens = 0;
  let release;
  const released = new Promise((resolve) => {
    release = resolve;
  });

  const client = new OresRpcStreamClient({
    operations: [KEY],
    framedStream: {
      carrier: "websocket",
      open: async () => {
        opens += 1;
        return {
          incoming: (async function* () {
            await released;
            yield { id: ID, t: "data", body: { n: 1 } };
            yield { id: ID, t: "end" };
          })(),
          cancel: async () => {},
        };
      },
    },
  });

  const handle = await client.prepare(KEY, { method: "GET", path: "/events" }).stream();

  const observed = [];
  const observableDone = new Promise((resolve, reject) => {
    handle.observable.subscribe({
      next: (item) => observed.push(item),
      error: reject,
      complete: resolve,
    });
  });

  const iterated = [];
  const iteratorDone = (async () => {
    for await (const item of handle) iterated.push(item);
  })();

  await nextTurn();
  assert.equal(
    opens,
    1,
    "one RpcStreamHandle must not open one carrier per Observable/iterator consumer",
  );

  release();
  await Promise.all([observableDone, iteratorDone]);

  assert.deepEqual(observed, [{ n: 1 }]);
  assert.deepEqual(iterated, [{ n: 1 }]);
  assert.equal(opens, 1);
  assert.equal(handle.context.attempts, 1);
});
