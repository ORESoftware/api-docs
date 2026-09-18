import assert from "node:assert/strict";
import test from "node:test";

import {
  OresRpcStreamClient,
  RpcStreamTransportError,
} from "./stream-rpc.js";

function asyncFrames(frames) {
  return {
    async *[Symbol.asyncIterator]() {
      for (const frame of frames) yield frame;
    },
  };
}

test("prepare performs zero I/O; stream is the sole open boundary", async () => {
  let opens = 0;
  const framedStream = {
    carrier: "websocket",
    async open(call) {
      opens += 1;
      assert.equal(call.key, "demo.events.watch_stream");
      assert.equal(call.id, "s-1");
      return {
        incoming: asyncFrames([
          { id: "s-1", t: "data", body: { value: 1 } },
          { id: "s-1", t: "data", body: { value: 2 } },
          { id: "s-1", t: "end" },
        ]),
      };
    },
  };
  const client = new OresRpcStreamClient({
    framedStream,
    operations: ["demo.events.watch_stream"],
    idPrefix: "s-",
  });

  const call = client
    .prepare(
      "demo.events.watch_stream",
      { method: "GET", path: "/v1/events" },
      (value) => value.value,
    )
    .addQueryField("room_id", 7)
    .withBody({ include_deleted: false });

  assert.equal(opens, 0);
  const stream = await call.stream();
  assert.equal(opens, 1);

  const values = [];
  for await (const value of stream) values.push(value);
  assert.deepEqual(values, [1, 2]);
  assert.equal(stream.context.ended, true);
  assert.equal(stream.context.cancelled, false);
});

test("a stream builder can open only once", async () => {
  const framedStream = {
    carrier: "tcp",
    async open(call) {
      return { incoming: asyncFrames([{ id: call.id, t: "end" }]) };
    },
  };
  const call = new OresRpcStreamClient({
    framedStream,
    operations: ["demo.watch_stream"],
  }).prepare("demo.watch_stream", { method: "GET", path: "/v1/watch" });

  await call.stream();
  await assert.rejects(
    call.stream(),
    (error) =>
      error instanceof RpcStreamTransportError &&
      error.reason === "protocol",
  );
});

test("correlation mismatches fail closed", async () => {
  const framedStream = {
    carrier: "websocket",
    async open() {
      return {
        incoming: asyncFrames([{ id: "wrong", t: "data", body: 1 }]),
      };
    },
  };
  const stream = await new OresRpcStreamClient({
    framedStream,
    operations: ["demo.watch_stream"],
    idPrefix: "expected-",
  })
    .prepare("demo.watch_stream", { method: "GET", path: "/v1/watch" })
    .stream();

  await assert.rejects(
    async () => {
      for await (const _ of stream) {
        void _;
      }
    },
    (error) =>
      error instanceof RpcStreamTransportError &&
      error.reason === "protocol",
  );
});

test("remote errors retain code and reason", async () => {
  const framedStream = {
    carrier: "tcp",
    async open(call) {
      return {
        incoming: asyncFrames([
          { id: call.id, t: "error", code: "denied", message: "nope" },
        ]),
      };
    },
  };
  const stream = await new OresRpcStreamClient({
    framedStream,
    operations: ["demo.watch_stream"],
  })
    .prepare("demo.watch_stream", { method: "GET", path: "/v1/watch" })
    .stream();

  await assert.rejects(
    async () => {
      for await (const _ of stream) {
        void _;
      }
    },
    (error) =>
      error instanceof RpcStreamTransportError &&
      error.reason === "remote" &&
      error.code === "denied",
  );
});

test("cancel delegates once", async () => {
  let cancels = 0;
  const framedStream = {
    carrier: "websocket",
    async open() {
      return {
        incoming: asyncFrames([]),
        async cancel() {
          cancels += 1;
        },
      };
    },
  };
  const stream = await new OresRpcStreamClient({
    framedStream,
    operations: ["demo.watch_stream"],
  })
    .prepare("demo.watch_stream", { method: "GET", path: "/v1/watch" })
    .stream();

  await stream.cancel();
  await stream.cancel();
  assert.equal(cancels, 1);
  assert.equal(stream.context.cancelled, true);
});
