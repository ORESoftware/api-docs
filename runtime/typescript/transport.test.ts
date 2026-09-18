/** node --experimental-strip-types --test runtime/typescript/transport.test.ts */

import assert from "node:assert/strict";
import { test } from "node:test";

import { type Frame, dataFrame, endFrame, errorFrame } from "./frame.ts";
import {
  FramedStreamTransport,
  FramedTransport,
  HttpTransport,
  RpcTransportError,
  type FramedConnection,
  type FramedStream,
  type RidlRequest,
} from "./transport.ts";
import type { RpcEvent, RpcTelemetrySink } from "./telemetry.ts";

const request: RidlRequest = {
  key: "healthz",
  method: "GET",
  path: "/healthz",
  pathTemplate: "/healthz",
  query: [],
  delivery: "direct",
};

function canned(frames: Frame[], carrier: "websocket" | "tcp" = "websocket"): FramedConnection {
  return { carrier, exchange: async () => frames };
}

class Recorder implements RpcTelemetrySink {
  readonly seen: RpcEvent[] = [];
  emit(event: RpcEvent): void {
    this.seen.push(event);
  }
}

test("a unary answer is one data frame then end", async () => {
  const t = new FramedTransport(canned([dataFrame("1", { ok: true }), endFrame("1")]), "demo");
  assert.equal(await t.call(request), '{"ok":true}');
});

test("the same generated request works over websocket and tcp alike", async () => {
  for (const carrier of ["websocket", "tcp"] as const) {
    const t = new FramedTransport(canned([dataFrame("1", { c: carrier }), endFrame("1")], carrier), "demo");
    assert.equal(await t.call(request), `{"c":"${carrier}"}`);
  }
});

test("an error frame becomes a remote error, not a body", async () => {
  const t = new FramedTransport(canned([errorFrame("1", "503", "draining")]), "demo");
  await assert.rejects(t.call(request), (e: RpcTransportError) => {
    assert.equal(e.reason, "remote");
    assert.equal(e.code, "503");
    return true;
  });
});

test("a second data frame on a unary call is refused", async () => {
  const t = new FramedTransport(canned([dataFrame("1", 1), dataFrame("1", 2), endFrame("1")]), "demo");
  await assert.rejects(t.call(request), /more than one data frame/);
});

test("a body without an end frame is refused", async () => {
  const t = new FramedTransport(canned([dataFrame("1", { ok: true })]), "demo");
  await assert.rejects(t.call(request), /never ended/);
});

test("a frame for another exchange is refused", async () => {
  const t = new FramedTransport(canned([dataFrame("99", { ok: true }), endFrame("99")]), "demo");
  await assert.rejects(t.call(request), /arrived on the exchange for/);
});

test("telemetry records the template and never the substituted path", async () => {
  const sink = new Recorder();
  const t = new FramedTransport(canned([dataFrame("1", { ok: true }), endFrame("1")]), "demo", "", sink);
  await t.call({ ...request, path: "/v1/matters/secret-uuid", pathTemplate: "/v1/matters/{id}" });
  assert.equal(sink.seen.length, 1);
  assert.equal(sink.seen[0]!.pathTemplate, "/v1/matters/{id}");
  assert.equal(sink.seen[0]!.outcome, "ok");
  assert.equal(sink.seen[0]!.carrier, "websocket");
  assert.ok(!JSON.stringify(sink.seen[0]).includes("secret-uuid"), "no substituted path in telemetry");
});

test("a failure is still observed, with its code", async () => {
  const sink = new Recorder();
  const t = new FramedTransport(canned([errorFrame("1", "404")]), "demo", "", sink);
  await assert.rejects(t.call(request));
  assert.equal(sink.seen[0]!.outcome, "failed");
  assert.equal(sink.seen[0]!.code, "404");
});

test("a throwing telemetry sink cannot break the call", async () => {
  const boom: RpcTelemetrySink = {
    emit() {
      throw new Error("exporter is down");
    },
  };
  const t = new FramedTransport(canned([dataFrame("1", { ok: true }), endFrame("1")]), "demo", "", boom);
  assert.equal(await t.call(request), '{"ok":true}');
});

test("http goes through untouched and is observed as http", async () => {
  const sink = new Recorder();
  const t = new HttpTransport({ call: async () => '{"ok":true}' }, "demo", sink);
  assert.equal(await t.call(request), '{"ok":true}');
  assert.equal(sink.seen[0]!.carrier, "http");
});

test("an http carrier failure is reported as transport_error", async () => {
  const sink = new Recorder();
  const t = new HttpTransport({ call: async () => { throw new Error("ECONNREFUSED"); } }, "demo", sink);
  await assert.rejects(t.call(request), (e: RpcTransportError) => e.reason === "carrier");
  assert.equal(sink.seen[0]!.outcome, "transport_error");
});

async function* frameStream(frames: readonly Frame[]): AsyncIterable<Frame> {
  for (const frame of frames) yield frame;
}

test("stream builder performs no I/O until stream() and returns an opened stream client", async () => {
  let opens = 0;
  const framed: FramedStream = {
    carrier: "websocket",
    async open() {
      opens += 1;
      return {
        incoming: frameStream([
          dataFrame("s-1", { value: 1 }),
          dataFrame("s-1", { value: 2 }),
          endFrame("s-1"),
        ]),
      };
    },
  };
  const transport = new FramedStreamTransport(framed, "s-");
  const builder = transport.prepare<{ value: number }>(
    { ...request, key: "demo.events.watch_stream" },
    (value) => value as { value: number },
  );

  assert.equal(opens, 0, "constructing/preparing a streaming RPC must not perform I/O");

  const stream = await builder.stream();
  assert.equal(opens, 1, "stream() is the first and only open boundary");

  const values: Array<{ value: number }> = [];
  for await (const value of stream) values.push(value);
  assert.deepEqual(values, [{ value: 1 }, { value: 2 }]);
  assert.equal(stream.context.ended, true);
  assert.equal(stream.context.cancelled, false);
});

test("stream builder cannot open the same call twice", async () => {
  const framed: FramedStream = {
    carrier: "tcp",
    async open() {
      return { incoming: frameStream([endFrame("once-1")]) };
    },
  };
  const builder = new FramedStreamTransport(framed, "once-").prepare(request);
  await builder.stream();
  await assert.rejects(builder.stream(), /can only be opened once/);
});

test("stream client rejects correlation mismatches", async () => {
  const framed: FramedStream = {
    carrier: "websocket",
    async open() {
      return { incoming: frameStream([dataFrame("wrong", 1), endFrame("wrong")]) };
    },
  };
  const stream = await new FramedStreamTransport(framed, "expected-").prepare(request).stream();
  await assert.rejects(
    async () => {
      for await (const _ of stream) {
        // consume
      }
    },
    /arrived on the stream for/,
  );
  assert.equal(stream.context.error?.reason, "protocol");
});

test("stream client surfaces remote error frames and records terminal context", async () => {
  const framed: FramedStream = {
    carrier: "websocket",
    async open() {
      return { incoming: frameStream([errorFrame("err-1", "503", "draining")]) };
    },
  };
  const stream = await new FramedStreamTransport(framed, "err-").prepare(request).stream();
  await assert.rejects(
    async () => {
      for await (const _ of stream) {
        // consume
      }
    },
    (error: RpcTransportError) => error.reason === "remote" && error.code === "503",
  );
  assert.equal(stream.context.error?.code, "503");
});

test("stream cancellation is delegated to the opened session", async () => {
  let cancels = 0;
  const framed: FramedStream = {
    carrier: "tcp",
    async open() {
      return {
        incoming: frameStream([]),
        async cancel() {
          cancels += 1;
        },
      };
    },
  };
  const stream = await new FramedStreamTransport(framed, "cancel-").prepare(request).stream();
  await stream.cancel();
  await stream.cancel();
  assert.equal(cancels, 1);
  assert.equal(stream.context.cancelled, true);
});
