import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

import { OresRpcUnaryClient, RpcDroppedError } from "./fluent-unary.js";
import { OresRpcStreamClient } from "./fluent-stream.js";
import { Rpc, OPTIONS } from "./options.generated.js";

const OPERATIONS = ["demo.users.find_user", "demo.events.watch_events"];
const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));

function planValidator() {
  const schema = JSON.parse(
    readFileSync(`${repoRoot}json-schema/rpc-request-plan.schema.json`, "utf8"),
  );
  const ajv = new Ajv2020({ strict: false, allErrors: true });
  return ajv.compile(schema);
}

function receipt(overrides = {}) {
  return {
    v: 1,
    op: "receipt",
    id: "fixed",
    key: "demo.users.find_user",
    ok: true,
    status: 200,
    body: { id: "user-42" },
    ...overrides,
  };
}

function unaryClient(transport, options = {}) {
  return new OresRpcUnaryClient({
    baseUrl: "http://127.0.0.1:9/",
    operations: OPERATIONS,
    transport,
    ...options,
  });
}

test("a unary chain terminates in makeCall and never exposes stream", () => {
  const builder = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  assert.equal(typeof builder.makeCall, "function");
  assert.equal(typeof builder.makeCallOrThrow, "function");
  assert.equal(builder.stream, undefined, "the unary surface must not carry stream()");
});

test("a streaming chain terminates in stream and never exposes makeCall", () => {
  const client = new OresRpcStreamClient({
    framedStream: { carrier: "websocket", open: async () => ({ incoming: [] }) },
    operations: OPERATIONS,
  });
  const builder = client.prepare("demo.events.watch_events", { method: "GET", path: "/events" });
  assert.equal(typeof builder.stream, "function");
  assert.equal(builder.makeCall, undefined, "the streaming surface must not carry makeCall()");
});

test("spending an exclusive group removes its methods from the builder", () => {
  const builder = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  assert.equal(typeof builder.useJson, "function");
  assert.equal(typeof builder.useProtobuf, "function");
  assert.equal(typeof builder.useSerialStrategy, "function");

  const narrowed = builder.useProtobuf();
  assert.equal(narrowed.useJson, undefined, "a second strategy must be absent, not merely guarded");
  assert.equal(narrowed.useProtobuf, undefined);
  assert.equal(narrowed.useSerialStrategy, undefined);
  assert.ok(!("useJson" in narrowed));
  // Unrelated groups survive the narrowing.
  assert.equal(typeof narrowed.omitAuth, "function");
  assert.equal(typeof narrowed.withTimeout, "function");
});

test("every exclusive group is a one-way door on both surfaces", () => {
  const unary = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  const groups = new Map();
  for (const option of OPTIONS) {
    if (!option.exclusiveGroup || option.appliesTo === "stream") continue;
    const members = groups.get(option.exclusiveGroup) ?? [];
    members.push(option);
    groups.set(option.exclusiveGroup, members);
  }
  assert.ok(groups.size >= 4, "the catalog should declare several exclusive groups");

  for (const [group, members] of groups) {
    const [first, ...rest] = members;
    const narrowed = unary[first.method](...sampleArgs(first));
    for (const other of [first, ...rest]) {
      assert.equal(
        narrowed[other.method],
        undefined,
        `${other.method} must be absent after spending ${group}`,
      );
    }
  }
});

function sampleArgs(option) {
  return option.params.map((param) => {
    switch (param.type) {
      case "enum":
        return param.enumId === "serial_strategy" ? Rpc.Strat.json : 0;
      case "bool":
        return true;
      case "u8":
      case "u32":
      case "i64":
        return param.minimum ?? 1;
      case "f64":
        return param.minimum ?? 1;
      case "url":
        return "http://127.0.0.1:8080";
      case "callback":
        return () => {};
      case "json_object":
        return {};
      default:
        return "x";
    }
  });
}

test("a built chain serializes to a plan the authored schema accepts", () => {
  const validate = planValidator();
  const plan = unaryClient(async () => receipt())
    .prepare("demo.users.find_user")
    .addPathField("user_id", "user-42")
    .useMessagePack()
    .withTimeout(2500)
    .withRetries(3)
    .withRetryBackoff(100, 2)
    .omitAuth()
    .queuePriority(Rpc.QPriority.three)
    .concurrencyKey("user-reads")
    .withTraceId("4bf92f3577b34da6a3ce929d0e0e4736")
    .toPlan();

  assert.equal(validate(plan), true, JSON.stringify(validate.errors));
  assert.equal(plan.kind, "unary");
  assert.equal(plan.serial_strategy, "message_pack");
  assert.equal(plan.auth_mode, "omitted");
  assert.deepEqual(plan.retry_backoff, { base_millis: 100, factor: 2 });
});

test("a streaming chain serializes to a valid streaming plan", () => {
  const validate = planValidator();
  const client = new OresRpcStreamClient({
    framedStream: { carrier: "websocket", open: async () => ({ incoming: [] }) },
    operations: OPERATIONS,
  });
  const plan = client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .useJson()
    .withBackpressure("drop_oldest")
    .withStreamBuffer(256)
    .sampleEach(100)
    .toPlan();

  assert.equal(validate(plan), true, JSON.stringify(validate.errors));
  assert.equal(plan.kind, "stream");
  assert.equal(plan.backpressure, "drop_oldest");
});

test("a credential never reaches the plan but does reach the wire", async () => {
  let seen;
  const client = unaryClient(async (request) => {
    seen = request.headers;
    return receipt();
  });
  const builder = client
    .prepare("demo.users.find_user")
    .withBearerToken("super-secret-value");

  const plan = builder.toPlan();
  assert.equal(plan.auth_mode, "bearer_override");
  assert.ok(
    !JSON.stringify(plan).includes("super-secret-value"),
    "the plan must not carry the credential",
  );

  await builder.makeCall();
  assert.equal(seen.authorization, "Bearer super-secret-value");
});

test("omitAuth drops a default Authorization header", async () => {
  let seen;
  const client = unaryClient(async (request) => {
    seen = request.headers;
    return receipt();
  });
  await client
    .prepare("demo.users.find_user", { headers: { authorization: "Bearer default" } })
    .omitAuth()
    .makeCall();
  assert.equal(seen.authorization, undefined);
});

test("serialization strategy sets the content negotiation headers", async () => {
  let seen;
  const client = unaryClient(async (request) => {
    seen = request.headers;
    return receipt();
  });
  await client.prepare("demo.users.find_user").useProtobuf().makeCall();
  assert.equal(seen["content-type"], "application/x-protobuf");
  assert.equal(seen.accept, "application/x-protobuf");
});

test("withRetries retries through rxjs and reports each attempt", async () => {
  let attempts = 0;
  const observed = [];
  const client = unaryClient(async () => {
    attempts += 1;
    if (attempts < 3) throw new Error("transient");
    return receipt();
  });
  const [value] = await client
    .prepare("demo.users.find_user")
    .withRetries(3)
    .onRetry((attempt) => observed.push(attempt))
    .makeCall();

  assert.equal(attempts, 3);
  assert.deepEqual(observed, [1, 2]);
  assert.deepEqual(value, { id: "user-42" });
});

test("withFallback resolves instead of failing once retries are exhausted", async () => {
  const client = unaryClient(async () => {
    throw new Error("always down");
  });
  const [value, ctx] = await client
    .prepare("demo.users.find_user")
    .withFallback({ id: "cached" })
    .makeCall();
  assert.deepEqual(value, { id: "cached" });
  assert.equal(ctx.fallback, true);
});

test("withTimeout aborts a slow call locally", async () => {
  const client = unaryClient(
    () => new Promise((resolve) => setTimeout(() => resolve(receipt()), 200)),
  );
  await assert.rejects(
    client.prepare("demo.users.find_user").withTimeout(20).makeCall(),
    /[Tt]imeout/,
  );
});

test("dedupe joins an in-flight identical call", async () => {
  let opened = 0;
  const client = unaryClient(async () => {
    opened += 1;
    await new Promise((resolve) => setTimeout(resolve, 20));
    return receipt();
  });
  const chain = () => client.prepare("demo.users.find_user").dedupe().makeCall();
  const [a, b] = await Promise.all([chain(), chain()]);
  assert.equal(opened, 1, "an identical in-flight call must not open a second socket");
  assert.deepEqual(a, b);
});

test("withCacheTtl collapses a repeated call", async () => {
  let opened = 0;
  const client = unaryClient(async () => {
    opened += 1;
    return receipt();
  });
  const chain = () => client.prepare("demo.users.find_user").withCacheTtl(60).makeCall();
  await chain();
  await chain();
  assert.equal(opened, 1);
});

test("throttle drops a call inside the window", async () => {
  const client = unaryClient(async () => receipt());
  const chain = () => client.prepare("demo.users.find_user").throttle(1000).makeCall();
  await chain();
  await assert.rejects(chain(), RpcDroppedError);
});

test("concurrencyKey serializes calls sharing the key", async () => {
  const order = [];
  let active = 0;
  const client = unaryClient(async () => {
    active += 1;
    order.push(active);
    await new Promise((resolve) => setTimeout(resolve, 10));
    active -= 1;
    return receipt();
  });
  const chain = () =>
    client.prepare("demo.users.find_user").concurrencyKey("sync_draft").makeCall();
  await Promise.all([chain(), chain(), chain()]);
  assert.deepEqual(order, [1, 1, 1], "calls on one concurrency key never overlap");
});

test("skipTlsVerify is refused without the capability and admitted with it", () => {
  const guarded = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  assert.throws(() => guarded.skipTlsVerify(), /insecure_local_dev/);

  const granted = unaryClient(async () => receipt(), {
    capabilities: ["insecure_local_dev"],
  }).prepare("demo.users.find_user");
  assert.equal(granted.skipTlsVerify().toPlan().skip_tls_verify, true);
});

test("declared bounds are enforced at the call site", () => {
  const builder = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  assert.throws(() => builder.withTimeout(0), /at least 1/);
  assert.throws(() => builder.withTimeout(600001), /at most 600000/);
  assert.throws(() => builder.withRetries(17), /at most 16/);
  assert.throws(() => builder.withTimeout(1.5), /must be an integer/);
});

test("a stream yields decoded items through the async iterator", async () => {
  const frames = [
    { id: "ores-demo.events.watch_events", t: "data", body: { n: 1 } },
    { id: "ores-demo.events.watch_events", t: "data", body: { n: 2 } },
    { id: "ores-demo.events.watch_events", t: "end" },
  ];
  const client = new OresRpcStreamClient({
    framedStream: {
      carrier: "websocket",
      open: async () => ({ incoming: frames, cancel: async () => {} }),
    },
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withStreamBuffer(8)
    .stream();

  const received = [];
  for await (const item of handle) received.push(item);
  assert.deepEqual(received, [{ n: 1 }, { n: 2 }]);
  assert.equal(handle.context.ended, true);
});

test("a remote error frame surfaces as a transport error", async () => {
  const frames = [
    { id: "ores-demo.events.watch_events", t: "error", code: "unavailable", message: "gone" },
  ];
  const client = new OresRpcStreamClient({
    framedStream: {
      carrier: "websocket",
      open: async () => ({ incoming: frames }),
    },
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .stream();

  await assert.rejects(async () => {
    for await (const _ of handle) void _;
  }, /gone/);
});

test("the generated catalog and the installed surface agree", () => {
  const builder = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  const expected = OPTIONS.filter((o) => o.appliesTo !== "stream").map((o) => o.method);
  for (const method of expected) {
    assert.equal(typeof builder[method], "function", `missing unary method ${method}`);
  }
  const streamOnly = OPTIONS.filter((o) => o.appliesTo === "stream").map((o) => o.method);
  for (const method of streamOnly) {
    assert.equal(builder[method], undefined, `unary builder must not carry ${method}`);
  }
});

// --- adversarial redaction -------------------------------------------------
// An option-level `secret` flag only covers withBearerToken. These assert the
// final-boundary rule: whatever wrote a header, a credential-shaped name is
// redacted out of the plan.

test("caller-supplied credential headers never reach the plan", () => {
  const builder = unaryClient(async () => receipt()).prepare("demo.users.find_user");
  const plan = builder
    .addHeader("authorization", "Bearer CALLER-SECRET")
    .addHeader("Cookie", "session=COOKIE-SECRET")
    .addHeader("x-api-key", "APIKEY-SECRET")
    .addHeader("X-Tenant-Api-Key", "VENDOR-SECRET")
    .addHeader("proxy-authorization", "Basic PROXY-SECRET")
    .addHeader("x-refresh-token", "REFRESH-SECRET")
    .addHeaders({ "x-session-id": "SESSION-SECRET", "x-signature": "SIG-SECRET" })
    .toPlan();

  const text = JSON.stringify(plan);
  for (const needle of [
    "CALLER-SECRET",
    "COOKIE-SECRET",
    "APIKEY-SECRET",
    "VENDOR-SECRET",
    "PROXY-SECRET",
    "REFRESH-SECRET",
    "SESSION-SECRET",
    "SIG-SECRET",
  ]) {
    assert.ok(!text.includes(needle), `${needle} leaked into the plan: ${text}`);
  }
});

test("ordinary headers are not redacted", () => {
  const plan = unaryClient(async () => receipt())
    .prepare("demo.users.find_user")
    .addHeader("accept", "application/json")
    .addHeader("x-request-id", "req-42")
    .addHeader("x-api-version", "2026-09-18")
    .toPlan();
  assert.equal(plan.headers.accept, "application/json");
  assert.equal(plan.headers["x-request-id"], "req-42");
  assert.equal(plan.headers["x-api-version"], "2026-09-18");
});

test("a credential in a proxy URL is stripped, the rest of the URL is kept", () => {
  const plan = unaryClient(async () => receipt())
    .prepare("demo.users.find_user")
    .viaProxy("http://user:PROXY-PASSWORD@proxy.internal:8080/path?q=1")
    .toPlan();
  assert.ok(!plan.proxy_url.includes("PROXY-PASSWORD"), plan.proxy_url);
  assert.ok(plan.proxy_url.includes("proxy.internal:8080"), plan.proxy_url);
  assert.ok(plan.proxy_url.includes("/path?q=1"), plan.proxy_url);
});

test("an at-sign in a proxy path is not mistaken for userinfo", () => {
  const plan = unaryClient(async () => receipt())
    .prepare("demo.users.find_user")
    .viaProxy("http://proxy.internal/a@b")
    .toPlan();
  assert.equal(plan.proxy_url, "http://proxy.internal/a@b");
});

test("redacted headers still reach the wire", async () => {
  let seen;
  const client = unaryClient(async (request) => {
    seen = request.headers;
    return receipt();
  });
  const builder = client
    .prepare("demo.users.find_user")
    .addHeader("authorization", "Bearer CALLER-SECRET");
  assert.equal(builder.toPlan().headers.authorization, "[redacted]");
  await builder.makeCall();
  assert.equal(seen.authorization, "Bearer CALLER-SECRET");
});

// --- stream timeout --------------------------------------------------------

function hangingStream(onCancel) {
  const never = {
    async *[Symbol.asyncIterator]() {
      await new Promise(() => {});
    },
  };
  return {
    carrier: "websocket",
    open: async () => ({ incoming: never, cancel: async () => onCancel() }),
  };
}

test("a total stream timeout fails the stream and closes the carrier", async () => {
  let cancelled = false;
  const client = new OresRpcStreamClient({
    framedStream: hangingStream(() => {
      cancelled = true;
    }),
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withTimeout(60)
    .stream();

  await assert.rejects(async () => {
    for await (const _ of handle) void _;
  }, /exceeded its total timeout/);

  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(handle.context.ended, false, "a timed-out stream is not a clean end");
  assert.equal(cancelled, true, "the carrier must be closed on timeout");
});

test("an idle stream timeout fails the stream and closes the carrier", async () => {
  let cancelled = false;
  const client = new OresRpcStreamClient({
    framedStream: hangingStream(() => {
      cancelled = true;
    }),
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withStreamIdleTimeout(60)
    .stream();

  await assert.rejects(async () => {
    for await (const _ of handle) void _;
  }, /exceeded its idle timeout/);

  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(handle.context.ended, false);
  assert.equal(cancelled, true);
});

// --- audit regressions -----------------------------------------------------
// Each of these reproduced against the shipped client before it was fixed.

test("a cached response is never served across credentials", async () => {
  const client = unaryClient(async (request) =>
    receipt({ body: { whoami: request.headers.authorization } }),
  );
  const call = (token) =>
    client.prepare("demo.users.find_user").withBearerToken(token).withCacheTtl(60).makeCall();
  const [alice] = await call("ALICE-TOKEN");
  const [bob] = await call("BOB-TOKEN");
  // Plans are redacted, so both calls have the SAME plan. Keying the cache on
  // the plan handed Bob the response cached for Alice.
  assert.equal(alice.whoami, "Bearer ALICE-TOKEN");
  assert.equal(bob.whoami, "Bearer BOB-TOKEN");
});

test("an in-flight call is never joined across credentials", async () => {
  const client = unaryClient(async (request) => {
    await new Promise((resolve) => setTimeout(resolve, 20));
    return receipt({ body: { whoami: request.headers.authorization } });
  });
  const call = (token) =>
    client.prepare("demo.users.find_user").withBearerToken(token).dedupe().makeCall();
  const [[alice], [bob]] = await Promise.all([call("ALICE-TOKEN"), call("BOB-TOKEN")]);
  assert.equal(alice.whoami, "Bearer ALICE-TOKEN");
  assert.equal(bob.whoami, "Bearer BOB-TOKEN");
});

test("the same credentials still share a cache entry", async () => {
  let opened = 0;
  const client = unaryClient(async () => {
    opened += 1;
    return receipt();
  });
  const call = () =>
    client.prepare("demo.users.find_user").withBearerToken("SAME").withCacheTtl(60).makeCall();
  await call();
  await call();
  assert.equal(opened, 1, "the fix must not disable caching for authenticated calls");
});

test("debounce releases only the last call and rejects the superseded ones", async () => {
  let opened = 0;
  const client = unaryClient(async () => {
    opened += 1;
    return receipt();
  });
  const call = () => client.prepare("demo.users.find_user").debounce(30).makeCall();
  const results = await Promise.allSettled([call(), call(), call()]);
  assert.deepEqual(
    results.map((result) => result.status),
    ["rejected", "rejected", "fulfilled"],
  );
  assert.ok(results[0].reason instanceof RpcDroppedError);
  assert.equal(opened, 1);
});

test("throttle admits a new call once the window has passed", async () => {
  const client = unaryClient(async () => receipt());
  const call = () => client.prepare("demo.users.find_user").throttle(40).makeCall();
  await call();
  await assert.rejects(call(), RpcDroppedError);
  await new Promise((resolve) => setTimeout(resolve, 60));
  await call();
});

test("a retried stream reopens its carrier and surfaces the real outcome", async () => {
  const id = "ores-demo.events.watch_events";
  let opens = 0;
  const client = new OresRpcStreamClient({
    framedStream: {
      carrier: "websocket",
      open: async () => {
        opens += 1;
        const attempt = opens;
        return {
          incoming: (async function* () {
            if (attempt < 2) throw new Error("carrier dropped");
            yield { id, t: "data", body: { n: 1 } };
            yield { id, t: "end" };
          })(),
          cancel: async () => {},
        };
      },
    },
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withRetries(2)
    .stream();
  const received = [];
  for await (const item of handle) received.push(item);
  assert.equal(opens, 2, "retry must reopen the carrier, not re-read a dead session");
  assert.deepEqual(received, [{ n: 1 }]);
  assert.equal(handle.context.attempts, 2);
});

test("an exhausted stream retry reports the carrier's error, not a protocol error", async () => {
  const client = new OresRpcStreamClient({
    framedStream: {
      carrier: "websocket",
      open: async () => ({
        incoming: (async function* () {
          throw new Error("carrier dropped");
        })(),
        cancel: async () => {},
      }),
    },
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withRetries(1)
    .stream();
  await assert.rejects(async () => {
    for await (const _ of handle) void _;
  }, /carrier dropped/);
});

test("delay on a stream holds the open and drops nothing", async () => {
  const id = "ores-demo.events.watch_events";
  let openedAt;
  const startedAt = Date.now();
  const client = new OresRpcStreamClient({
    framedStream: {
      carrier: "websocket",
      open: async () => {
        openedAt = Date.now();
        return {
          incoming: (async function* () {
            for (let n = 1; n <= 5; n += 1) yield { id, t: "data", body: { n } };
            yield { id, t: "end" };
          })(),
          cancel: async () => {},
        };
      },
    },
    operations: OPERATIONS,
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .delay(40)
    .stream();
  const received = [];
  for await (const item of handle) received.push(item.n);
  assert.deepEqual(received, [1, 2, 3, 4, 5], "delay must not drop items");
  assert.ok(openedAt - startedAt >= 35, "the open itself must be held");
});

test("credential-bearing query fields are redacted by name", () => {
  const plan = unaryClient(async () => receipt())
    .prepare("demo.users.find_user")
    .addQueryField("access_token", "QUERY-SECRET")
    .addQueryField("Api-Key", "KEY-SECRET")
    .addQueryField("sig", "SIG-SECRET")
    .addQueryField("page", 2)
    .toPlan();
  const text = JSON.stringify(plan);
  for (const needle of ["QUERY-SECRET", "KEY-SECRET", "SIG-SECRET"]) {
    assert.ok(!text.includes(needle), `${needle} leaked: ${text}`);
  }
  assert.equal(plan.query.page, 2, "ordinary query fields survive");
});

test("redacted query fields still reach the wire", async () => {
  let seen;
  const client = unaryClient(async (request) => {
    seen = request.query;
    return receipt();
  });
  await client
    .prepare("demo.users.find_user")
    .addQueryField("access_token", "QUERY-SECRET")
    .makeCall();
  assert.equal(seen.access_token, "QUERY-SECRET");
});
