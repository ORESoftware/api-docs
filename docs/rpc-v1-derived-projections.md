# RPC v1 derived SQL, Protobuf, and gRPC projections

TypeSpec (`idl/typespec/v1.tsp`) and the authored JSON Schemas
(`json-schema/rpc-call.schema.json` and `rpc-receipt.schema.json`) remain
independent top-level authorities. Neither is generated from or overwritten by
the other.

Run:

```sh
python3 scripts/generate-rpc-v1-projections.py
python3 scripts/generate-rpc-v1-projections.py --check
python3 -m unittest scripts/test_generate_rpc_v1_projections.py -v
```

The generator stops before writing if the authorities disagree on a model's
field inventory, requiredness, scalar kind, literals, enum values, length
bounds, numeric bounds, or reviewed regular expressions.

## Committed outputs

- `idl/protobuf/ores/rpc/v1/rpc.proto` — the field-number-locked Protobuf
  messages and the unary `ores.rpc.v1.RpcGateway/Call` gRPC service.
- `generated/rpc-v1/rpc-storage.sql` — PostgreSQL storage projection for call
  and receipt frames, including the reviewed success/error receipt state rule.
- `generated/rpc-v1/grpc.json` — machine-readable service and method identities
  for client generators, documentation, and conformance tests.

Every output carries the same SHA-256 of the exact TypeSpec source, both JSON
Schema documents, the projection configuration, and the append-only Protobuf
field ledger.

## Compatibility boundary

`idl/protobuf.lock.json` assigns every released message field and enum value.
The generator never derives field numbers from declaration order. Existing
numbers cannot be reused; removed names or numbers remain reserved.

The v1 identities `RpcCall`, `RpcReceipt`, and `RpcGateway/Call` are retained
intentionally. Buf's `STANDARD` request/response naming rules normally prefer
new `CallRequest` and `CallResponse` wrappers, and its default service rule
prefers a `Service` suffix. For this already reviewed projection,
`idl/protobuf/buf.yaml` keeps every other `STANDARD` rule, scopes only the two
request/response-name exceptions to `ores/rpc/v1/rpc.proto`, and configures the
recorded `Gateway` service suffix. This avoids cosmetic message duplication or
a service rename while preserving field-number and fully-qualified service
identity. The exceptions do not waive descriptor compilation, unique request
and response use, field naming, package versioning, enum rules, or breaking
checks.

The projection configuration in `idl/rpc-v1.projection.json` records the three
reviewed representation deltas:

1. JSON objects and unknown bodies are canonical UTF-8 JSON bytes in Protobuf.
2. normalized header-name restrictions need the shared semantic validator;
3. the receipt success/error state is represented by JSON Schema
   `if`/`then`/`else`, TypeSpec aliases, a SQL check constraint, and runtime
   validation because flattened Protobuf cannot express it completely.

Protobuf decoding and a successful SQL insert are not authorization. Route
ownership, authentication, tenancy, idempotency, transaction boundaries,
payload limits, and business behavior remain explicit server policy.

## CI

The focused workflow:

1. compiles the authored TypeSpec project;
2. regenerates and byte-compares every output;
3. runs intentional authority- and field-number-drift tests;
4. formats, lints, and builds the Protobuf descriptor with Buf;
5. applies the generated DDL to PostgreSQL 15;
6. inserts valid call/success/error rows and proves invalid receipt states fail.
