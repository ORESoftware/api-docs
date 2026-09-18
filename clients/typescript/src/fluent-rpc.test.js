import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  OresRpcClient,
  RpcRemoteError,
} from "./fluent-rpc.js";

function receiptFor(envelope, overrides = {}) {
  return {
    v: 1,
    op: "receipt",
    id: envelope.id,
    key: envelope.key,
    transport: "http",
    ok: true,
    status: 200,
    body: { user_id: "u-1" },
    ...overrides,
  };
}

test("makeCall is the sole fetch boundary", async () => {
  let fetches = 0;
  const fetchImpl = async (_url, init) => {
    fetches += 1;
    const envelope = JSON.parse(init.body);
    return new Response(JSON.stringify(receiptFor(envelope)), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  };

  const client = new OresRpcClient({
    baseUrl: "https://example.invalid",
    operations: ["demo.users.find_user_by_id"],
    fetchImpl,
  });

  const call = client
    .prepare("demo.users.find_user_by_id", { body: { id: "u-1" } })
    .addHeader("x-foo", "bar")
    .addBodyField("include_profile", true);

  assert.equal(fetches, 0, "constructing/preparing/chaining must not perform I/O");

  const [value, ctx] = await call.makeCall();
  assert.equal(fetches, 1);
  assert.deepEqual(value, { user_id: "u-1" });
  assert.equal(ctx.ok, true);

  const source = await readFile(new URL("./fluent-rpc.js", import.meta.url), "utf8");
  const fetchCalls = source.match(/this\.fetchImpl\(/g) ?? [];
  assert.equal(fetchCalls.length, 1, "core runtime must contain exactly one fetch invocation");
  const start = source.indexOf("async makeCall()");
  const fetch = source.indexOf("this.fetchImpl(");
  const end = source.indexOf("async makeCallOrThrow()");
  assert.ok(start >= 0 && start < fetch && fetch < end, "fetch must be physically inside makeCall");
});

test("application failure returns context; makeCallOrThrow is opt-in", async () => {
  const fetchImpl = async (_url, init) => {
    const envelope = JSON.parse(init.body);
    return new Response(
      JSON.stringify(
        receiptFor(envelope, {
          ok: false,
          status: 409,
          body: undefined,
          error: { code: "conflict", message: "already exists" },
          traceId: "trace-1",
        }),
      ),
      { status: 409, headers: { "content-type": "application/json" } },
    );
  };
  const config = {
    baseUrl: "https://example.invalid",
    operations: ["demo.users.create_user"],
    fetchImpl,
  };

  const [value, ctx] = await new OresRpcClient(config)
    .prepare("demo.users.create_user")
    .makeCall();
  assert.equal(value, undefined);
  assert.equal(ctx.ok, false);
  assert.equal(ctx.status, 409);
  assert.deepEqual(ctx.errors, [{ code: "conflict", message: "already exists" }]);
  assert.deepEqual(ctx.traceIds, ["trace-1"]);

  await assert.rejects(
    new OresRpcClient(config)
      .prepare("demo.users.create_user")
      .makeCallOrThrow(),
    RpcRemoteError,
  );
});

test("protocol/correlation failure throws", async () => {
  const client = new OresRpcClient({
    baseUrl: "https://example.invalid",
    operations: ["demo.version.get_version"],
    fetchImpl: async () =>
      new Response(
        JSON.stringify({
          v: 1,
          op: "receipt",
          id: "wrong",
          key: "demo.version.get_version",
          ok: true,
          status: 200,
        }),
        { status: 200 },
      ),
  });
  await assert.rejects(
    client.prepare("demo.version.get_version").makeCall(),
    /correlation mismatch/,
  );
});

test("unknown operation is rejected before any I/O", () => {
  let fetches = 0;
  const client = new OresRpcClient({
    baseUrl: "https://example.invalid",
    operations: ["known"],
    fetchImpl: async () => {
      fetches += 1;
      throw new Error("must not execute");
    },
  });
  assert.throws(() => client.prepare("unknown"), /not generated for this audience/);
  assert.equal(fetches, 0);
});
