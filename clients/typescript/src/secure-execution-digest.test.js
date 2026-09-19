import assert from "node:assert/strict";
import test from "node:test";

import { executionDigest, RpcChainState, RpcOptionError } from "./fluent-core.js";
import { OresRpcUnaryClient } from "./fluent-unary.js";

const KEY = "demo.users.find_user";

async function withoutWebCrypto(body) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "crypto");
  Object.defineProperty(globalThis, "crypto", {
    value: undefined,
    configurable: true,
    writable: true,
  });
  try {
    return await body();
  } finally {
    if (descriptor) Object.defineProperty(globalThis, "crypto", descriptor);
    else delete globalThis.crypto;
  }
}

test("executionDigest fails closed instead of returning a credential-bearing identity", async () => {
  const state = new RpcChainState("unary", KEY, "/v1/rpc", {
    query: { access_token: "VERY-SECRET-VALUE" },
  });
  assert.ok(state.executionIdentity().includes("VERY-SECRET-VALUE"));

  await withoutWebCrypto(async () => {
    await assert.rejects(
      executionDigest(state),
      (error) =>
        error instanceof RpcOptionError &&
        /WebCrypto SHA-256/.test(error.message) &&
        !error.message.includes("VERY-SECRET-VALUE"),
    );
  });
});

test("cache and dedupe do not retain raw identities when WebCrypto is unavailable", async () => {
  let sends = 0;
  const client = new OresRpcUnaryClient({
    baseUrl: "http://127.0.0.1:9/",
    operations: [KEY],
    transport: async () => {
      sends += 1;
      return { v: 1, op: "receipt", id: "x", key: KEY, ok: true, status: 200, body: null };
    },
  });

  await withoutWebCrypto(async () => {
    await assert.rejects(
      client
        .prepare(KEY)
        .addQueryField("access_token", "VERY-SECRET-VALUE")
        .withCacheTtl(60)
        .makeCall(),
      (error) => error instanceof RpcOptionError && /WebCrypto SHA-256/.test(error.message),
    );
  });

  assert.equal(sends, 0, "the call must fail before a cacheable request is sent");
  assert.equal(client.scheduler.cache.size, 0);
  assert.equal(client.scheduler.inflight.size, 0);
  for (const key of [...client.scheduler.cache.keys(), ...client.scheduler.inflight.keys()]) {
    assert.ok(!key.includes("VERY-SECRET-VALUE"), key);
  }
});
