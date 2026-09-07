# RPC v1 generated projections

Do not edit files in this directory.

They are generated from the parity-checked RPC v1 TypeSpec and JSON Schema
authorities, `idl/rpc-v1.projection.json`, and `idl/protobuf.lock.json`:

```sh
python3 scripts/generate-rpc-v1-projections.py
python3 scripts/generate-rpc-v1-projections.py --check
```

- `rpc-storage.sql` is disposable/build-time DDL evidence and a downstream
  migration input. Application servers must not apply it at startup.
- `grpc.json` is the digest-bound service/method manifest. It records the
  stable `RpcCall`/`RpcReceipt` payload messages separately from the
  Buf-compliant `CallRequest`/`CallResponse` gRPC wrappers.
- The corresponding generated Proto file is
  `idl/protobuf/ores/rpc/v1/rpc.proto`.

The Proto package intentionally remains `ores.rpc.v1`. The same Buf module also
contains the established `ores.rpc.v2` package, so `idl/protobuf/buf.yaml`
keeps one bounded `PACKAGE_VERSION_SUFFIX` lint exception. All other STANDARD
Buf lint rules remain enabled, including service and RPC request/response
naming. The exception must not be widened or used to bypass compatibility
checks.

After generation the files are made read-only. Git does not persist that mode,
so repository CI verifies bytes and separately freezes the generated tree.
