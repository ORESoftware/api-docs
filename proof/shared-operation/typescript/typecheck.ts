import { ProofRpcClient } from "./generated.js";

const rpc = new ProofRpcClient("http://127.0.0.1:39091");

// These are compile-only witnesses: generated clients must reject payload and
// header argument types that cannot match the backend OperationSpec.
// @ts-expect-error display_name is generated as string
void rpc.createUser("tenant", { id: "bad", display_name: 42 });

// @ts-expect-error idempotency key is generated as string
void rpc.updateUser("bad", 42, { display_name: "Bad" });

// @ts-expect-error include_disabled is generated as boolean|null
void rpc.findUserById("bad", "false", null);
