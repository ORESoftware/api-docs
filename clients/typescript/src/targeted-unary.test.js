import assert from "node:assert/strict";
import test from "node:test";

import { OresTargetedRpcUnaryClient } from "./targeted-unary.js";

function receipt(envelope, body) {
  return {
    v: 1,
    op: "receipt",
    id: envelope.id,
    key: envelope.key,
    transport: "http",
    ok: true,
    status: 200,
    body,
  };
}

function clientWithRecorder(records) {
  return new OresTargetedRpcUnaryClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    operations: ["demo.users.find"],
    fetchImpl: async (url, init) => {
      const envelope = JSON.parse(init.body);
      records.push({ url: String(url), envelope });
      return new Response(
        JSON.stringify(receipt(envelope, { origin: new URL(url).origin })),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    },
  });
}

test("standalone and Lambda fluent calls use identical RPC wire shape on different origins", async () => {
  const records = [];
  const client = clientWithRecorder(records);

  const [standalone, standaloneCtx] = await client
    .call("demo.users.find", { body: { id: "u-1" } }, { standalone: true })
    .makeCall();
  const [lambda, lambdaCtx] = await client
    .call("demo.users.find", { body: { id: "u-1" } }, { lambda: true })
    .makeCall();

  assert.equal(standalone.origin, "https://api.example.test");
  assert.equal(lambda.origin, "https://lambda.example.test");
  assert.equal(standaloneCtx.endpointTarget, "standalone");
  assert.equal(lambdaCtx.endpointTarget, "lambda");
  assert.equal(new URL(records[0].url).pathname, "/v1/rpc");
  assert.equal(new URL(records[1].url).pathname, "/v1/rpc");

  const left = { ...records[0].envelope, id: "normalized" };
  const right = { ...records[1].envelope, id: "normalized" };
  assert.deepEqual(left, right);
  assert.equal(left.transport, "http");
});

test("cache entries never cross standalone and Lambda endpoint targets", async () => {
  const records = [];
  const client = clientWithRecorder(records);

  const standalone = () =>
    client
      .call("demo.users.find", { body: { id: "u-1" } }, { target: "standalone" })
      .withCacheTtl(30)
      .makeCall();
  const lambda = () =>
    client
      .call("demo.users.find", { body: { id: "u-1" } }, { target: "lambda" })
      .withCacheTtl(30)
      .makeCall();

  await standalone();
  await standalone();
  await lambda();
  await lambda();

  assert.equal(records.length, 2, "each endpoint target owns an independent cache");
  assert.equal(new URL(records[0].url).origin, "https://api.example.test");
  assert.equal(new URL(records[1].url).origin, "https://lambda.example.test");
});

test("dedupe joins within a target but never across targets", async () => {
  const records = [];
  let releases = [];
  let resolveTwoEntries;
  const twoEntries = new Promise((resolve) => {
    resolveTwoEntries = resolve;
  });
  const client = new OresTargetedRpcUnaryClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    operations: ["demo.users.find"],
    fetchImpl: async (url, init) => {
      const envelope = JSON.parse(init.body);
      records.push(String(url));
      if (records.length === 2) resolveTwoEntries();
      await new Promise((resolve) => releases.push(resolve));
      return new Response(JSON.stringify(receipt(envelope, { ok: true })), { status: 200 });
    },
  });

  const standaloneA = client.call("demo.users.find", {}, { standalone: true }).dedupe().makeCall();
  const standaloneB = client.call("demo.users.find", {}, { standalone: true }).dedupe().makeCall();
  const lambda = client.call("demo.users.find", {}, { lambda: true }).dedupe().makeCall();

  await Promise.race([
    twoEntries,
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error("standalone and Lambda transports did not both start")), 1000),
    ),
  ]);
  assert.equal(records.length, 2, "standalone duplicates collapse, Lambda stays separate");
  for (const release of releases) release();
  releases = [];
  await Promise.all([standaloneA, standaloneB, lambda]);
});

test("selection ambiguity and absent Lambda endpoint fail before network I/O", () => {
  let fetches = 0;
  const client = new OresTargetedRpcUnaryClient({
    baseUrl: "https://api.example.test",
    operations: ["demo.users.find"],
    fetchImpl: async () => {
      fetches += 1;
      throw new Error("must not execute");
    },
  });

  assert.throws(
    () => client.call("demo.users.find", {}, { lambda: true, standalone: true }),
    /cannot enable lambda and standalone together/,
  );
  assert.throws(
    () => client.call("demo.users.find", {}, { target: "standalone", lambda: true }),
    /conflicts with lambda=true/,
  );
  assert.throws(
    () => client.call("demo.users.find", {}, { target: "lambda" }),
    /not configured/,
  );
  assert.equal(fetches, 0);
});

test("malformed endpoint selectors fail closed before network I/O", () => {
  let fetches = 0;
  const client = new OresTargetedRpcUnaryClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    operations: ["demo.users.find"],
    fetchImpl: async () => {
      fetches += 1;
      throw new Error("must not execute");
    },
  });

  for (const endpoint of [
    { lamba: true },
    { lambda: "true" },
    { standalone: 1 },
    { target: "edge" },
    { target: "lambda", extra: true },
    null,
    [],
  ]) {
    assert.throws(() => client.call("demo.users.find", {}, endpoint), /endpoint|target|plain object/i);
  }
  assert.equal(fetches, 0);
});

test("one-shot operation and capability iterables are snapshotted before endpoint fan-out", async () => {
  function* once(value) {
    yield value;
  }

  const records = [];
  const client = new OresTargetedRpcUnaryClient({
    baseUrl: "https://api.example.test",
    lambdaBaseUrl: "https://lambda.example.test",
    operations: once("demo.users.find"),
    capabilities: once("insecure_local_dev"),
    fetchImpl: async (url, init) => {
      const envelope = JSON.parse(init.body);
      records.push(String(url));
      return new Response(JSON.stringify(receipt(envelope, { ok: true })), { status: 200 });
    },
  });

  await client.call("demo.users.find", {}, { standalone: true }).skipTlsVerify().makeCall();
  await client.call("demo.users.find", {}, { lambda: true }).skipTlsVerify().makeCall();
  assert.equal(records.length, 2);
});
