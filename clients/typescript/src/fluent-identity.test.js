// Identity isolation, total-call timeouts, and scheduler hygiene.
//
// Every test here reproduced a defect in the shipped client before it was
// fixed. They are kept together because they share one theme: state that
// outlives a single call must never be keyed, bounded or timed by something a
// caller — or redaction — can collapse.

import assert from "node:assert/strict";
import test from "node:test";
import { inspect } from "node:util";

import { executionDigest } from "./fluent-core.js";
import { OresRpcStreamClient, RpcOptionError, RpcStreamTimeoutError } from "./fluent-stream.js";
import { OresRpcUnaryClient, RpcDroppedError, RpcTimeoutError } from "./fluent-unary.js";

const OPERATIONS = ["demo.users.find_user", "demo.events.watch_events"];
const STREAM_ID = "ores-demo.events.watch_events";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function receipt(body = { id: "user-42" }) {
  return { v: 1, op: "receipt", id: "x", key: "demo.users.find_user", ok: true, status: 200, body };
}

function unaryClient(transport, options = {}) {
  return new OresRpcUnaryClient({
    baseUrl: "http://127.0.0.1:9/",
    operations: OPERATIONS,
    transport,
    ...options,
  });
}

/**
 * Echoes back exactly what the transport was given to execute. It reads the
 * WIRE view, never the plan: the plan is redacted, so an echo built from it is
 * identical for every principal and the assertions below would compare two
 * equal strings and prove nothing.
 */
function whoamiTransport(delayMillis = 0) {
  return async (request) => {
    if (delayMillis > 0) await sleep(delayMillis);
    return receipt({
      headers: request.headers,
      query: request.query ?? null,
      proxy: request.wire.proxy_url ?? null,
    });
  };
}

/**
 * One entry per class of value that redaction collapses. Adding a redaction
 * class without adding it here is fine — the identity is the pre-redaction
 * document, so it is covered structurally — but every class listed is proved.
 */
const COLLAPSIBLE = [
  ["bearer option", (b, who) => b.withBearerToken(who)],
  ["authorization header", (b, who) => b.addHeader("authorization", `Bearer ${who}`)],
  ["cookie header", (b, who) => b.addHeader("cookie", `session=${who}`)],
  ["vendor api-key header", (b, who) => b.addHeader("x-tenant-api-key", who)],
  ["access_token query field", (b, who) => b.addQueryField("access_token", who)],
  ["signature query field", (b, who) => b.addQueryField("sig", who)],
  ["proxy URL userinfo", (b, who) => b.viaProxy(`http://${who}:pw@proxy.internal:8080`)],
];

for (const [label, apply] of COLLAPSIBLE) {
  test(`${label}: plans collapse, identities and digests do not`, async () => {
    const client = unaryClient(whoamiTransport());
    const alice = apply(client.prepare("demo.users.find_user"), "ALICE");
    const bob = apply(client.prepare("demo.users.find_user"), "BOB");

    // The premise. If this stops holding, the class is no longer redacted and
    // the assertions below prove nothing.
    assert.equal(
      JSON.stringify(alice.toPlan()),
      JSON.stringify(bob.toPlan()),
      "plans must be identical once redacted",
    );
    assert.notEqual(alice.state.executionIdentity(), bob.state.executionIdentity());
    assert.notEqual(await executionDigest(alice.state), await executionDigest(bob.state));
  });

  test(`${label}: a cached response is never served across principals`, async () => {
    let sent = 0;
    const transport = whoamiTransport();
    const client = unaryClient(async (request) => {
      sent += 1;
      return transport(request);
    });
    const call = (who) =>
      apply(client.prepare("demo.users.find_user"), who).withCacheTtl(60).makeCall();
    const [alice] = await call("ALICE");
    const [bob] = await call("BOB");
    // The oracle must be able to tell the two apart, or the rest is vacuous.
    assert.ok(JSON.stringify(alice).includes("ALICE"), "the echo does not reflect this class");
    assert.ok(JSON.stringify(bob).includes("BOB"), "Bob was served a response that is not his");
    assert.ok(!JSON.stringify(bob).includes("ALICE"), "Bob was served the response cached for Alice");
    assert.equal(sent, 2);
    const [again] = await call("BOB");
    assert.deepEqual(again, bob);
    assert.equal(sent, 2, "the same principal must still hit its own cache entry");
  });

  test(`${label}: an in-flight call is never joined across principals`, async () => {
    let sent = 0;
    const transport = whoamiTransport(15);
    const client = unaryClient(async (request) => {
      sent += 1;
      return transport(request);
    });
    const call = (who) =>
      apply(client.prepare("demo.users.find_user"), who).dedupe().makeCall();
    const [[alice], [bob]] = await Promise.all([call("ALICE"), call("BOB")]);
    assert.equal(sent, 2, "two principals must open two calls");
    assert.ok(JSON.stringify(alice).includes("ALICE") && !JSON.stringify(alice).includes("BOB"));
    assert.ok(JSON.stringify(bob).includes("BOB") && !JSON.stringify(bob).includes("ALICE"));
  });
}

test("the same principal still shares one in-flight call", async () => {
  let sent = 0;
  const client = unaryClient(async () => {
    sent += 1;
    await sleep(15);
    return receipt();
  });
  const call = () =>
    client.prepare("demo.users.find_user").addQueryField("access_token", "SAME").dedupe().makeCall();
  await Promise.all([call(), call(), call()]);
  assert.equal(sent, 1, "isolation must not disable deduplication");
});

test("the digest is a fixed-size hash, not the credential-bearing identity", async () => {
  const builder = unaryClient(whoamiTransport())
    .prepare("demo.users.find_user")
    .addQueryField("access_token", "VERY-SECRET-VALUE");
  const digest = await executionDigest(builder.state);
  assert.match(digest, /^[0-9a-f]{64}$/);
  assert.ok(!digest.includes("VERY-SECRET-VALUE"));
  assert.ok(builder.state.executionIdentity().includes("VERY-SECRET-VALUE"));
});

test("scheduler maps hold digests, never raw credentials", async () => {
  const client = unaryClient(whoamiTransport());
  await client
    .prepare("demo.users.find_user")
    .addQueryField("access_token", "VERY-SECRET-VALUE")
    .withBearerToken("ANOTHER-SECRET")
    .withCacheTtl(60)
    .makeCall();
  for (const key of client.scheduler.cache.keys()) {
    assert.ok(!key.includes("VERY-SECRET-VALUE") && !key.includes("ANOTHER-SECRET"), key);
  }
});

// --- total-call timeout ------------------------------------------------------

test("withTimeout bounds the whole call, not each attempt", async () => {
  let attempts = 0;
  const client = unaryClient(() => {
    attempts += 1;
    return new Promise(() => {}); // a carrier that never answers
  });
  const started = Date.now();
  await assert.rejects(
    client.prepare("demo.users.find_user").withTimeout(100).withRetries(3).makeCall(),
    (error) => error instanceof RpcTimeoutError && error.kind === "total",
  );
  const elapsed = Date.now() - started;
  assert.ok(elapsed < 250, `a 100ms budget ran for ${elapsed}ms`);
  assert.equal(attempts, 1, "expiring the budget is terminal; the timeout itself is not retried");
});

test("retries and backoff consume one shared budget", async () => {
  let attempts = 0;
  const client = unaryClient(async () => {
    attempts += 1;
    throw new Error("connection refused");
  });
  const started = Date.now();
  await assert.rejects(
    client
      .prepare("demo.users.find_user")
      .withTimeout(120)
      .withRetries(16)
      .withRetryBackoff(40, 1)
      .makeCall(),
    (error) => error instanceof RpcTimeoutError && error.kind === "total",
  );
  const elapsed = Date.now() - started;
  assert.ok(elapsed < 300, `a 120ms budget ran for ${elapsed}ms`);
  assert.ok(attempts >= 2 && attempts <= 5, `expected the budget to cut retries short, saw ${attempts}`);
});

test("a deliberate delay is not charged to the timeout", async () => {
  const client = unaryClient(async () => receipt());
  const [value] = await client
    .prepare("demo.users.find_user")
    .delay(80)
    .withTimeout(40)
    .makeCall();
  assert.deepEqual(value, { id: "user-42" });
});

test("an absolute deadline does include the delay", async () => {
  const client = unaryClient(async () => receipt());
  await assert.rejects(
    client
      .prepare("demo.users.find_user")
      .delay(120)
      .withDeadline(Date.now() + 40)
      .makeCall(),
    (error) => error instanceof RpcTimeoutError && error.kind === "deadline",
  );
});

function hangingCarrier(events) {
  let opens = 0;
  return {
    carrier: "websocket",
    open: async () => {
      opens += 1;
      const n = opens;
      events.push(`open${n}`);
      return {
        incoming: { async *[Symbol.asyncIterator]() { await new Promise(() => {}); } },
        cancel: async () => { events.push(`cancel${n}`); },
      };
    },
  };
}

test("a stream's total timeout is not reset by retries", async () => {
  const events = [];
  const client = new OresRpcStreamClient({ framedStream: hangingCarrier(events), operations: OPERATIONS });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withTimeout(100)
    .withRetries(3)
    .stream();
  const started = Date.now();
  await assert.rejects(async () => {
    for await (const _ of handle) void _;
  }, (error) => error instanceof RpcStreamTimeoutError && error.kind === "total");
  const elapsed = Date.now() - started;
  assert.ok(elapsed < 250, `a 100ms budget ran for ${elapsed}ms`);
  await sleep(10);
  assert.deepEqual(events, ["open1", "cancel1"], "one open, and the carrier is closed on expiry");
});

test("an idle timeout belongs to the attempt, so retry can reopen a stalled carrier", async () => {
  const events = [];
  const client = new OresRpcStreamClient({ framedStream: hangingCarrier(events), operations: OPERATIONS });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withStreamIdleTimeout(30)
    .withRetries(2)
    .stream();
  await assert.rejects(async () => {
    for await (const _ of handle) void _;
  }, (error) => error instanceof RpcStreamTimeoutError && error.kind === "idle");
  await sleep(10);
  assert.deepEqual(
    events,
    ["open1", "cancel1", "open2", "cancel2", "open3", "cancel3"],
    "each stalled session must be closed BEFORE the next one is opened",
  );
});

test("a failed session is closed before the retry opens the next one", async () => {
  const events = [];
  let opens = 0;
  const client = new OresRpcStreamClient({
    operations: OPERATIONS,
    framedStream: {
      carrier: "websocket",
      open: async () => {
        opens += 1;
        const n = opens;
        events.push(`open${n}`);
        return {
          incoming: (async function* () {
            if (n === 1) throw new Error("carrier dropped");
            yield { id: STREAM_ID, t: "data", body: { n } };
            yield { id: STREAM_ID, t: "end" };
          })(),
          // A slow close: if cleanup were fire-and-forget, open2 would land first.
          cancel: async () => { await sleep(20); events.push(`cancel${n}`); },
        };
      },
    },
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .withRetries(1)
    .stream();
  const received = [];
  for await (const item of handle) received.push(item);
  assert.deepEqual(received, [{ n: 2 }]);
  assert.deepEqual(events, ["open1", "cancel1", "open2"], "a cleanly ended session needs no cancel");
});

// --- debounce + scheduler hygiene --------------------------------------------

test("changing the debounce interval still supersedes the pending call", async () => {
  let sent = 0;
  const client = unaryClient(async () => {
    sent += 1;
    return receipt();
  });
  const first = client.prepare("demo.users.find_user").debounce(40).makeCall();
  const second = client.prepare("demo.users.find_user").debounce(80).makeCall();
  const settled = await Promise.race([
    Promise.allSettled([first, second]),
    sleep(600).then(() => "a promise was left pending forever"),
  ]);
  assert.notEqual(settled, "a promise was left pending forever");
  assert.deepEqual(settled.map((result) => result.status), ["rejected", "fulfilled"]);
  assert.ok(settled[0].reason instanceof RpcDroppedError);
  assert.equal(sent, 1, "both calls went out: the old gate fired on its own timer");
});

test("scheduler maps do not grow with caller-controlled keys", async () => {
  const client = unaryClient(async () => receipt());
  const { scheduler } = client;
  await Promise.all(
    Array.from({ length: 50 }, (_, n) =>
      client.prepare("demo.users.find_user").concurrencyKey(`tenant-${n}`).makeCall(),
    ),
  );
  await sleep(5);
  assert.equal(scheduler.queues.size, 0, "a drained concurrency queue must forget its key");

  await Promise.all(
    Array.from({ length: 20 }, (_, n) =>
      client.prepare("demo.users.find_user").concurrencyKey(`d-${n}`).debounce(10).makeCall(),
    ),
  );
  await sleep(5);
  assert.equal(scheduler.debounces.size, 0, "a released debounce gate must remove itself");

  await Promise.all(
    Array.from({ length: 20 }, (_, n) =>
      client.prepare("demo.users.find_user").concurrencyKey(`t-${n}`).throttle(15).makeCall(),
    ),
  );
  assert.equal(scheduler.throttles.size, 20);
  await sleep(60);
  assert.equal(scheduler.throttles.size, 0, "an idle throttle gate must remove itself");
});

test("the response cache is bounded", async () => {
  const client = unaryClient(async () => receipt());
  for (let n = 0; n < 1100; n += 1) {
    await client
      .prepare("demo.users.find_user")
      .addQueryField("page", n)
      .withCacheTtl(600)
      .makeCall();
  }
  assert.ok(client.scheduler.cache.size <= 1024, `cache grew to ${client.scheduler.cache.size}`);
});

// --- plan is for logs, wire is for the network ---------------------------------

test("a transport is given the credentials it has to execute with", async () => {
  let seen;
  const client = unaryClient(async (request) => {
    seen = request;
    return receipt();
  });
  await client
    .prepare("demo.users.find_user")
    .viaProxy("http://user:pw@proxy.internal:8080")
    .addQueryField("access_token", "T0KEN")
    .withBearerToken("B3ARER")
    .makeCall();

  // What a transport executes. Handed only the plan, a proxying transport saw
  // http://redacted@proxy.internal and the proxy answered 407.
  assert.equal(seen.wire.proxy_url, "http://user:pw@proxy.internal:8080");
  assert.equal(seen.wire.query.access_token, "T0KEN");
  assert.equal(seen.wire.headers.authorization, "Bearer B3ARER");

  // What anyone logs. Same fields, nothing secret.
  const logged = JSON.stringify(seen.plan);
  for (const secret of ["user:pw", "T0KEN", "B3ARER"]) {
    assert.ok(!logged.includes(secret), `the plan leaks ${secret}`);
  }
  assert.deepEqual(Object.keys(seen.wire), Object.keys(seen.plan));
});

test("a stream carrier is given its options beside the frame, never inside it", async () => {
  let frame;
  let options;
  const client = new OresRpcStreamClient({
    operations: OPERATIONS,
    framedStream: {
      carrier: "websocket",
      open: async (call, carrierOptions) => {
        frame = call;
        options = carrierOptions;
        return {
          incoming: (async function* () {
            yield { id: STREAM_ID, t: "end" };
          })(),
        };
      },
    },
  });
  const handle = await client
    .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
    .viaProxy("http://user:pw@proxy.internal:8080")
    .stream();
  for await (const _ of handle) void _;

  assert.equal(options.wire.proxy_url, "http://user:pw@proxy.internal:8080");
  assert.equal(options.plan.proxy_url, "http://redacted@proxy.internal:8080");
  // The frame is sent to the SERVER. A proxy URL — credentials or not — has no
  // business in it.
  assert.ok(!JSON.stringify(frame).includes("proxy"), "the proxy leaked into the call frame");
});

// --- a stream's plan and its frame are the same call ---------------------------

function captureCarrier() {
  const seen = {};
  return {
    seen,
    framedStream: {
      carrier: "websocket",
      open: async (call) => {
        seen.frame = call;
        return {
          incoming: (async function* () {
            yield { id: STREAM_ID, t: "end" };
          })(),
        };
      },
    },
  };
}

test("what a stream chain plans is what it sends", async () => {
  const { seen, framedStream } = captureCarrier();
  const chain = new OresRpcStreamClient({ operations: OPERATIONS, framedStream })
    .prepare("demo.events.watch_events", {
      method: "GET",
      path: "/rooms/{room}/events",
      query: { since: "10" },
    })
    .addQueryField("page", 2)
    .addPathField("room", "lobby/1");
  const handle = await chain.stream();
  for await (const _ of handle) void _;

  const plan = chain.toPlan();
  // addQueryField() used to reach the plan and never the frame; the
  // prepare()-time query reached the frame and never the plan.
  assert.deepEqual(plan.query, { page: 2, since: "10" });
  assert.deepEqual(seen.frame.query, plan.query);
  assert.deepEqual(plan.path, { room: "lobby/1" });
  assert.equal(seen.frame.path, "/rooms/lobby%2F1/events", "path fields fill the template, encoded");
});

test("a prepare()-time query credential is planned, redacted and still sent", async () => {
  const { seen, framedStream } = captureCarrier();
  const chain = new OresRpcStreamClient({ operations: OPERATIONS, framedStream }).prepare(
    "demo.events.watch_events",
    { method: "GET", path: "/events", query: { access_token: "T0KEN" } },
  );
  assert.equal(chain.toPlan().query.access_token, "[redacted]");
  const handle = await chain.stream();
  for await (const _ of handle) void _;
  assert.equal(seen.frame.query.access_token, "T0KEN");
});

test("a path field that would be silently dropped is an error", async () => {
  const { framedStream } = captureCarrier();
  const client = new OresRpcStreamClient({ operations: OPERATIONS, framedStream });
  await assert.rejects(
    client
      .prepare("demo.events.watch_events", { method: "GET", path: "/rooms/{room}/events" })
      .stream(),
    (error) => error instanceof RpcOptionError && /\{room\} has no value/.test(error.message),
  );
  await assert.rejects(
    client
      .prepare("demo.events.watch_events", { method: "GET", path: "/events" })
      .addPathField("room", "lobby")
      .stream(),
    (error) => error instanceof RpcOptionError && /no \{room\} placeholder/.test(error.message),
  );
});

// --- how a call is shown ---------------------------------------------------------

for (const [label, apply] of COLLAPSIBLE) {
  test(`${label}: logging or serializing a chain never shows the credential`, () => {
    const unary = apply(unaryClient(whoamiTransport()).prepare("demo.users.find_user"), "S3CRET-PRINCIPAL");
    const stream = apply(
      new OresRpcStreamClient({
        operations: OPERATIONS,
        framedStream: { carrier: "websocket", open: async () => ({ incoming: [] }) },
      }).prepare("demo.events.watch_events", {
        method: "GET",
        path: "/events",
        // Held on the builder itself, outside the chain state.
        query: { access_token: "S3CRET-PRINCIPAL" },
      }),
      "S3CRET-PRINCIPAL",
    );
    // The premise: the secret really is in there to be leaked.
    assert.ok(unary.state.executionIdentity().includes("S3CRET-PRINCIPAL"));
    const shown = {
      "JSON.stringify(unary)": JSON.stringify(unary),
      "JSON.stringify(unary.state)": JSON.stringify(unary.state),
      "JSON.stringify({ call })": JSON.stringify({ error: "boom", call: unary }),
      "inspect(unary)": inspect(unary, { depth: 12, showHidden: false }),
      "inspect(unary.state)": inspect(unary.state, { depth: 12 }),
      "inspect({ call })": inspect({ call: unary }, { depth: 12 }),
      "JSON.stringify(stream)": JSON.stringify(stream),
      "inspect(stream)": inspect(stream, { depth: 12 }),
    };
    for (const [how, output] of Object.entries(shown)) {
      assert.ok(!output.includes("S3CRET-PRINCIPAL"), `${how} leaks the credential: ${output}`);
    }
    // Still useful: what is shown is the plan.
    assert.deepEqual(JSON.parse(shown["JSON.stringify(unary)"]), unary.toPlan());
  });
}

// --- a redacted plan is still well-formed -------------------------------------

test("a redacted proxy URL is still a valid RFC 3986 URI", () => {
  const plan = unaryClient(async () => receipt())
    .prepare("demo.users.find_user")
    .viaProxy("http://user:pw@proxy.internal:8080/p?q=1")
    .toPlan();
  assert.equal(plan.proxy_url, "http://redacted@proxy.internal:8080/p?q=1");
  const userinfo = plan.proxy_url.split("://")[1].split("@")[0];
  // userinfo = *( unreserved / pct-encoded / sub-delims / ":" ). "[redacted]"
  // fails this, which made every redacted proxy URL fail `format: uri`.
  assert.match(userinfo, /^(?:[A-Za-z0-9\-._~!$&'()*+,;=:]|%[0-9A-Fa-f]{2})*$/);
  assert.equal(new URL(plan.proxy_url).username, "redacted");
});

// --- what the cache is allowed to remember --------------------------------------

function sequenceTransport(receipts) {
  let sent = 0;
  const transport = async () => {
    const next = receipts[Math.min(sent, receipts.length - 1)];
    sent += 1;
    if (next instanceof Error) throw next;
    return next;
  };
  return { transport, sent: () => sent };
}

test("an error response is never cached", async () => {
  const failed = { ...receipt({ error: "unavailable" }), ok: false, status: 503 };
  const { transport, sent } = sequenceTransport([failed, receipt({ id: "fresh" })]);
  const client = unaryClient(transport);
  const call = () => client.prepare("demo.users.find_user").withCacheTtl(60).makeCall();
  const [, first] = await call();
  assert.equal(first.status, 503);
  const [value, second] = await call();
  assert.equal(sent(), 2, "a transient 503 was pinned for the whole TTL");
  assert.equal(second.ok, true);
  assert.deepEqual(value, { id: "fresh" });
});

test("a fallback is never cached", async () => {
  const { transport, sent } = sequenceTransport([new Error("offline"), receipt({ id: "fresh" })]);
  const client = unaryClient(transport);
  const call = () =>
    client
      .prepare("demo.users.find_user")
      .withFallback({ id: "placeholder" })
      .withCacheTtl(60)
      .makeCall();
  const [placeholder, ctx] = await call();
  assert.deepEqual(placeholder, { id: "placeholder" });
  assert.equal(ctx.fallback, true);
  const [value] = await call();
  assert.equal(sent(), 2, "one outage was remembered as the answer");
  assert.deepEqual(value, { id: "fresh" });
});

test("a caller mutating its result cannot change what the next caller is served", async () => {
  const { transport, sent } = sequenceTransport([receipt({ items: [3, 1, 2] })]);
  const client = unaryClient(transport);
  const call = () => client.prepare("demo.users.find_user").withCacheTtl(60).makeCall();

  const [first] = await call();
  first.items.sort(); // the caller that made the network call
  first.injected = true;

  const [second] = await call();
  assert.deepEqual(second, { items: [3, 1, 2] }, "the cache stored the caller's own object");
  second.items.length = 0; // a caller served from cache

  const [third] = await call();
  assert.deepEqual(third, { items: [3, 1, 2] }, "cache hits share one mutable object");
  assert.equal(sent(), 1);
});

test("an outcome that cannot be copied is not cached", async () => {
  let sent = 0;
  const client = unaryClient(async () => {
    sent += 1;
    return receipt({ notCloneable: () => {} });
  });
  const call = () => client.prepare("demo.users.find_user").withCacheTtl(60).makeCall();
  await call();
  await call();
  assert.equal(sent, 2);
  assert.equal(client.scheduler.cache.size, 0);
});

test("staleWhileRevalidate answers from the stale entry and refreshes once behind it", async () => {
  let sent = 0;
  const client = unaryClient(async () => {
    sent += 1;
    await sleep(10);
    return receipt({ version: sent });
  });
  // A zero TTL makes every entry stale at once, so the window is all that serves it.
  const call = () =>
    client
      .prepare("demo.users.find_user")
      .withCacheTtl(0)
      .staleWhileRevalidate(60)
      .makeCall();

  const [first] = await call();
  assert.deepEqual(first, { version: 1 });

  const started = Date.now();
  const stale = await Promise.all([call(), call(), call()]);
  assert.ok(Date.now() - started < 8, "a stale hit must not wait for the network");
  assert.deepEqual(stale.map(([value]) => value), [{ version: 1 }, { version: 1 }, { version: 1 }]);

  await sleep(30);
  assert.equal(sent, 2, "three stale hits must share one background refresh");
  const [refreshed] = await call();
  assert.deepEqual(refreshed, { version: 2 });
});

test("without staleWhileRevalidate an expired entry is never served", async () => {
  let sent = 0;
  const client = unaryClient(async () => {
    sent += 1;
    return receipt({ version: sent });
  });
  const call = () => client.prepare("demo.users.find_user").withCacheTtl(0).makeCall();
  await call();
  const [value] = await call();
  assert.deepEqual(value, { version: 2 });
});

test("a failed background refresh keeps the stale answer and is not an unhandled rejection", async () => {
  const { transport } = sequenceTransport([receipt({ version: 1 }), new Error("offline")]);
  const client = unaryClient(transport);
  const call = () =>
    client
      .prepare("demo.users.find_user")
      .withCacheTtl(0)
      .staleWhileRevalidate(60)
      .makeCall();
  await call();
  const [stale] = await call();
  await sleep(10);
  const [still] = await call();
  assert.deepEqual([stale, still], [{ version: 1 }, { version: 1 }]);
  assert.equal(client.scheduler.refreshes.size <= 1, true);
});

test("changing the throttle window does not reopen a closed gate", async () => {
  let sent = 0;
  const client = unaryClient(async () => {
    sent += 1;
    return receipt();
  });
  await client.prepare("demo.users.find_user").throttle(200).makeCall();
  // Inside both the old window and its own: dropped, whichever number is used.
  await assert.rejects(
    client.prepare("demo.users.find_user").throttle(100).makeCall(),
    (error) => error instanceof RpcDroppedError,
  );
  assert.equal(sent, 1, "a new window let the call straight through");
  await sleep(120);
  // Judged by its own 100ms window, which has now passed.
  await client.prepare("demo.users.find_user").throttle(100).makeCall();
  assert.equal(sent, 2);
});
