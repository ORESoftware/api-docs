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
  assert.equal(ctx.endpointTarget, "standalone");

  const source = await readFile(new URL("./fluent-rpc.js", import.meta.url), "utf8");
  const fetchCalls = source.match(/this\.fetchImpl\(/g) ?? [];
  assert.equal(fetchCalls.length, 1, "core runtime must contain exactly one fetch invocation");
  const start = source.indexOf("async makeCall()");
  const fetch = source.indexOf("this.fetchImpl(");
  const end = source.indexOf("async makeCallOrThrow()");
  assert.ok(start >= 0 && start < fetch && fetch < end, "fetch must be physically inside makeCall");
});

test("standaloneBaseUrl is a first-class constructor spelling", () => {
  const client = new OresRpcClient({
    standaloneBaseUrl: "https://standalone.example.test",
    operations: ["known"],
    fetchImpl: async () => {
      throw new Error("must not execute");
    },
  });
  assert.equal(client.baseUrl, "https://standalone.example.test");
  assert.equal(client.standaloneBaseUrl, "https://standalone.example.test");
  assert.equal(client.call("known").endpointTarget, "standalone");
});

test("per-call endpoint selection changes only origin, never RPC envelope semantics", async () => {
  const seen = [];
  const fetchImpl = async (url, init) => {
    const envelope = JSON.parse(init.body);
    seen.push({ url: String(url), envelope });
    return new Response(JSON.stringify(receiptFor(envelope)), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  };
  const client = new OresRpcClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    operations: ["demo.users.find_user_by_id"],
    fetchImpl,
  });

  const [standaloneBody, standaloneCtx] = await client
    .call("demo.users.find_user_by_id", { body: { id: "u-1" } }, { target: "standalone" })
    .makeCall();
  const [lambdaBody, lambdaCtx] = await client
    .call("demo.users.find_user_by_id", { body: { id: "u-1" } }, { lambda: true })
    .makeCall();

  assert.deepEqual(standaloneBody, lambdaBody);
  assert.equal(standaloneCtx.endpointTarget, "standalone");
  assert.equal(lambdaCtx.endpointTarget, "lambda");
  assert.equal(new URL(seen[0].url).origin, "https://api.example.test");
  assert.equal(new URL(seen[1].url).origin, "https://lambda.example.test");
  assert.equal(new URL(seen[0].url).pathname, "/v1/rpc");
  assert.equal(new URL(seen[1].url).pathname, "/v1/rpc");

  const first = { ...seen[0].envelope, id: "normalized" };
  const second = { ...seen[1].envelope, id: "normalized" };
  assert.deepEqual(first, second, "endpoint placement must not enter the RPC wire envelope");
  assert.equal(first.transport, "http");
});

test("endpoint selection fails closed on ambiguity or missing Lambda origin", () => {
  const client = new OresRpcClient({
    baseUrl: "https://api.example.test",
    operations: ["known"],
    fetchImpl: async () => {
      throw new Error("must not execute");
    },
  });

  assert.throws(
    () => client.call("known", {}, { lambda: true, standalone: true }),
    /cannot enable lambda and standalone together/,
  );
  assert.throws(
    () => client.call("known", {}, { target: "standalone", lambda: true }),
    /conflicts with lambda=true/,
  );
  assert.throws(
    () => client.call("known", {}, { target: "lambda" }),
    /not configured/,
  );
});

test("basic client rejects malformed endpoint selectors before network I/O", () => {
  let fetches = 0;
  const client = new OresRpcClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    operations: ["known"],
    fetchImpl: async () => {
      fetches += 1;
      throw new Error("must not execute");
    },
  });

  for (const endpoint of [
    { lamba: true },
    { lambda: "true" },
    { standalone: 1 },
    { target: "lambda", extra: true },
    [],
    null,
  ]) {
    assert.throws(
      () => client.call("known", {}, endpoint),
      /endpoint selection|selector|unknown RPC endpoint/,
    );
  }
  assert.equal(fetches, 0);
});

test("endpoint reconfiguration is closed and transactional", () => {
  const client = new OresRpcClient({
    standaloneBaseUrl: "https://api.example.test",
    operations: ["known"],
    fetchImpl: async () => {
      throw new Error("must not execute");
    },
  });
  const before = {
    baseUrl: client.baseUrl,
    standaloneBaseUrl: client.standaloneBaseUrl,
    lambdaBaseUrl: client.lambdaBaseUrl,
    defaultTarget: client.defaultTarget,
  };

  assert.throws(
    () => client.configureEndpoints({
      standaloneBaseUrl: "https://changed.example.test",
      defaultTarget: "lambda",
    }),
    /requires lambdaBaseUrl/,
  );
  assert.deepEqual(
    {
      baseUrl: client.baseUrl,
      standaloneBaseUrl: client.standaloneBaseUrl,
      lambdaBaseUrl: client.lambdaBaseUrl,
      defaultTarget: client.defaultTarget,
    },
    before,
    "a rejected endpoint update must not partially mutate client state",
  );

  assert.throws(
    () => client.configureEndpoints({ lambaBaseUrl: "https://typo.example.test" }),
    /unknown RPC endpoint configuration field/,
  );
  assert.deepEqual(
    {
      baseUrl: client.baseUrl,
      standaloneBaseUrl: client.standaloneBaseUrl,
      lambdaBaseUrl: client.lambdaBaseUrl,
      defaultTarget: client.defaultTarget,
    },
    before,
  );

  client.configureEndpoints({
    lambdaBaseUrl: "https://lambda.example.test",
    defaultTarget: "lambda",
  });
  assert.equal(client.lambdaBaseUrl, "https://lambda.example.test");
  assert.equal(client.defaultTarget, "lambda");
});

test("default target can be Lambda only when Lambda endpoint is configured", async () => {
  assert.throws(
    () =>
      new OresRpcClient({
        baseUrl: "https://api.example.test",
        defaultTarget: "lambda",
        operations: ["known"],
        fetchImpl: async () => {
          throw new Error("must not execute");
        },
      }),
    /requires lambdaBaseUrl/,
  );

  let seen;
  const client = new OresRpcClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    defaultTarget: "lambda",
    operations: ["known"],
    fetchImpl: async (url, init) => {
      seen = String(url);
      const envelope = JSON.parse(init.body);
      return new Response(JSON.stringify(receiptFor(envelope)), { status: 200 });
    },
  });
  const [, ctx] = await client.call("known").makeCall();
  assert.equal(new URL(seen).origin, "https://lambda.example.test");
  assert.equal(ctx.endpointTarget, "lambda");
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
