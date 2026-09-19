// Cross-language conformance: replay each chain the Rust builder recorded and
// require a byte-identical plan. This is the check that makes "the clients
// agree" falsifiable rather than a claim in a README.

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { OresRpcUnaryClient } from "./fluent-unary.js";
import { OresRpcStreamClient } from "./fluent-stream.js";
import { OPTIONS } from "./options.generated.js";
import { canonicalPlanString } from "./fluent-core.js";

const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
const conformance = JSON.parse(
  readFileSync(`${repoRoot}generated/rpc-client-options/chain-conformance.json`, "utf8"),
);

const METHOD_BY_ID = new Map(OPTIONS.map((option) => [option.id, option.method]));

function builderFor(surface, key) {
  if (surface === "stream") {
    const client = new OresRpcStreamClient({
      framedStream: { carrier: "websocket", open: async () => ({ incoming: [] }) },
      operations: [key],
    });
    return client.prepare(key, { method: "GET", path: "/events" });
  }
  const client = new OresRpcUnaryClient({
    baseUrl: "http://127.0.0.1:9/",
    operations: [key],
    transport: async () => {
      throw new Error("conformance replays never open the network");
    },
  });
  return client.prepare(key);
}

/** Replay `[option_id, ...args]` steps against a fresh builder. */
function replay(chain) {
  let builder = builderFor(chain.surface, chain.key);
  for (const [optionId, ...args] of chain.steps) {
    const method = METHOD_BY_ID.get(optionId);
    assert.ok(method, `chain ${chain.chain_id} names unknown option ${optionId}`);
    assert.equal(
      typeof builder[method],
      "function",
      `${method} is not reachable at this point in ${chain.chain_id}`,
    );
    // A hook step carries no serializable argument; supply a real callback.
    const applied = optionId.startsWith("on_") && args.length === 0 ? [() => {}] : args;
    builder = builder[method](...applied);
  }
  return builder.toPlan();
}

test("the conformance corpus is non-trivial", () => {
  assert.ok(conformance.chains.length >= 8, "expected a meaningful number of chains");
  assert.ok(
    conformance.chains.some((chain) => chain.surface === "stream"),
    "both surfaces must be represented",
  );
});

for (const chain of conformance.chains) {
  test(`chain ${chain.chain_id} matches the Rust plan byte for byte`, () => {
    const actual = canonicalPlanString(replay(chain));
    // plan_canonical is the string Rust serialized. It is compared as-is: an
    // earlier version re-serialized Rust's plan through JSON.stringify first,
    // which quietly normalized `2.0` to `2` and hid a real byte difference.
    assert.equal(
      actual,
      chain.plan_canonical,
      `${chain.rationale}\n  rust: ${chain.plan_canonical}\n  ts:   ${actual}`,
    );
  });
}

test("the canonical string and the structured plan describe the same document", () => {
  for (const chain of conformance.chains) {
    assert.deepEqual(JSON.parse(chain.plan_canonical), chain.plan, chain.chain_id);
  }
});

test("a credential never appears in any recorded plan", () => {
  for (const chain of conformance.chains) {
    assert.ok(
      !JSON.stringify(chain.plan).includes("super-secret-value"),
      `${chain.chain_id} leaked a credential`,
    );
  }
});
