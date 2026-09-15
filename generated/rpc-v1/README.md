# RPC v1 generated projections

Do not edit files in this directory.

They are generated from the parity-checked RPC v1 TypeSpec and JSON Schema
authorities, `idl/rpc-v1.projection.json`, and `idl/protobuf.lock.json`:

```sh
python3 scripts/generate-rpc-v1-projections.py
python3 scripts/generate-rpc-v1-projections.py --check
node scripts/generate-rpc-v1-formal-projections.mjs
node scripts/generate-rpc-v1-formal-projections.mjs --check
```

- `rpc-storage.sql` is disposable/build-time DDL evidence and a downstream
  migration input. Application servers must not apply it at startup.
- `grpc.json` is the digest-bound service/method manifest. It records the
  stable `RpcCall`/`RpcReceipt` payload messages separately from the
  Buf-compliant `CallRequest`/`CallResponse` gRPC wrappers.
- `wit/ores-rpc-v1.wit` is the WebAssembly Component Model boundary projection.
  It preserves the unary call/receipt inventory and maps JSON containers to
  canonical UTF-8 JSON bytes; shared runtime admission remains mandatory.
- `dafny/ores_rpc_v1.dfy` is proof evidence for the RPC receipt-state invariant.
  It proves success/error disjointness and status-range implications without
  becoming a runtime validator or contract authority.
- The corresponding generated Proto file is
  `idl/protobuf/ores/rpc/v1/rpc.proto`.

The generated `RpcService.Call(CallRequest) -> CallResponse` surface satisfies
Buf's STANDARD service and RPC request/response naming rules without renaming
the released `RpcCall` and `RpcReceipt` payload messages. No naming-rule
exception or custom service suffix is required.

TypeSpec and authored JSON Schema Draft 2020-12 remain the independent peer
authorities. Protobuf/gRPC, WIT, Dafny, SQL, and language bindings are derived
evidence only and must track the same projection digest.

After generation the files are made read-only. Git does not persist that mode,
so repository CI verifies bytes and separately freezes the generated tree.
