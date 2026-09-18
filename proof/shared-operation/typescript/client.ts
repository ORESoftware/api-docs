import { ProofRpcClient } from "./generated.ts";

function assertRpcTraceChain(
  traceIds: string[],
  expectedRpc: string,
  expectedHandler: string,
): void {
  if (traceIds[0] !== expectedRpc) throw new Error(`missing rpc trace ${expectedRpc}`);
  if (!traceIds.includes(expectedHandler)) throw new Error(`missing handler trace ${expectedHandler}`);
  if (traceIds.some((value) => value.includes("proof-http"))) {
    throw new Error("RPC result unexpectedly passed through ordinary HTTP adapter");
  }
}

const rpc = new ProofRpcClient("http://127.0.0.1:39091");

const created = await rpc.createUser("tenant-ts", {
  id: "ts-user",
  display_name: "TypeScript User",
});
if (created.result.id !== "ts-user") throw new Error("create result mismatch");
assertRpcTraceChain(
  created.traceIds,
  "ores-trace-HA55l7mbjBwL3g7kcFatR",
  "ores-trace-kyWJwSSkCRw6JPP1fGBXa",
);

const found = await rpc.findUserById("ts-user", false, null);
if (found.result.display_name !== "TypeScript User") throw new Error("find result mismatch");
assertRpcTraceChain(
  found.traceIds,
  "ores-trace-tuxPrC6DrxG1JraioBRdE",
  "ores-trace-k9e5kg-cYX1JRJzeGhXQJ",
);

const updated = await rpc.updateUser("ts-user", "ts-idempotency-1", {
  display_name: "TypeScript Updated",
});
if (updated.result.display_name !== "TypeScript Updated") throw new Error("update result mismatch");
assertRpcTraceChain(
  updated.traceIds,
  "ores-trace-WXw41GYs3E6rSSfesc9RC",
  "ores-trace-1DJT7X2n4_bzNCwb3dhQy",
);
