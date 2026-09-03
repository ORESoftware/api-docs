import {
  encodeCall,
  encodeReceipt,
  type RpcV1Call,
  type RpcV1Receipt,
} from "./rpc.js";

const call: RpcV1Call = encodeCall({ id: "c1", key: "healthz" });
const success: RpcV1Receipt = encodeReceipt({
  id: call.id,
  key: call.key,
  ok: true,
  status: 200,
  body: null,
});
const failure: RpcV1Receipt = encodeReceipt({
  id: call.id,
  key: call.key,
  ok: false,
  status: 503,
  error: { code: "unavailable" },
});

void success;
void failure;

// @ts-expect-error successful receipts cannot carry an error object
encodeReceipt({ id: "c1", key: "healthz", ok: true, error: { code: "bad" } });
// @ts-expect-error failed receipts require an error object
encodeReceipt({ id: "c1", key: "healthz", ok: false });
// @ts-expect-error calls do not accept receipt-only state
encodeCall({ id: "c1", key: "healthz", ok: true });
