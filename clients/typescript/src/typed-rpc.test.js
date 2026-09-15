import assert from "node:assert/strict";
import test from "node:test";

import { createTypedRpcClient } from "./typed-rpc.js";

const routes = Object.freeze({
  get_item: Object.freeze({
    key: "get_item",
    path: "/v1/items/{id}",
    methods: Object.freeze(["GET"]),
    transports: Object.freeze(["http", "tcp", "websocket"]),
  }),
  replace_item: Object.freeze({
    key: "replace_item",
    path: "/v1/items/{id}",
    methods: Object.freeze(["PUT", "PATCH"]),
    transports: Object.freeze(["http"]),
  }),
  nats_ping: Object.freeze({
    key: "nats_ping",
    path: "/rpc/nats-ping",
    methods: Object.freeze(["POST"]),
    transports: Object.freeze(["nats"]),
  }),
});

test("binds the operation key to declared transport and route metadata", async () => {
  let observed;
  const client = createTypedRpcClient({
    routes,
    transport: "websocket",
    correlationPrefix: "ws-",
    invoke: async (call, binding) => {
      observed = { call, binding };
      return {
        v: 1,
        op: "receipt",
        id: call.id,
        key: call.key,
        transport: call.transport,
        ok: true,
        status: 200,
        body: { id: "42" },
      };
    },
  });

  const body = await client.call("get_item", {
    path: { id: "42" },
    headers: { "x-request-id": "req-1" },
  });

  assert.deepEqual(body, { id: "42" });
  assert.equal(observed.call.id, "ws-1");
  assert.equal(observed.call.key, "get_item");
  assert.equal(observed.binding.path, "/v1/items/{id}");
  assert.equal(observed.binding.selectedMethod, undefined);
});

test("requires explicit HTTP method only for multi-method bindings", async () => {
  const methods = [];
  const client = createTypedRpcClient({
    routes,
    transport: "http",
    invoke: async (call, binding) => {
      methods.push(binding.selectedMethod);
      return {
        v: 1,
        op: "receipt",
        id: call.id,
        key: call.key,
        transport: "http",
        ok: true,
        status: 204,
      };
    },
  });

  await client.call("get_item", {
    path: { id: "42" },
    headers: { "x-request-id": "req-1" },
  });
  assert.equal(methods[0], "GET");

  await assert.rejects(
    () => client.call("replace_item", { path: { id: "42" } }),
    /choose method explicitly/,
  );

  await client.call("replace_item", {
    path: { id: "42" },
    method: "PATCH",
  });
  assert.equal(methods[1], "PATCH");
});

test("fails closed when the operation is not declared on the selected transport", async () => {
  const client = createTypedRpcClient({
    routes,
    transport: "tcp",
    invoke: async () => {
      throw new Error("invoke must not run");
    },
  });

  await assert.rejects(
    () => client.call("nats_ping", {}),
    /does not declare transport tcp/,
  );
});

test("keeps runtime response validation as an explicit boundary", async () => {
  const client = createTypedRpcClient({
    routes,
    transport: "nats",
    invoke: async (call) => ({
      v: 1,
      op: "receipt",
      id: call.id,
      key: call.key,
      transport: "nats",
      ok: true,
      body: { pong: true },
    }),
    validateResponse: (key, value) => {
      assert.equal(key, "nats_ping");
      assert.deepEqual(value, { pong: true });
      return value;
    },
  });

  assert.deepEqual(await client.call("nats_ping", {}), { pong: true });
});
