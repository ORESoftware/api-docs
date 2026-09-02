# Digest-bound RPC and API-document generation

This repository has two intentionally separate RPC generations:

- v1 route-map RPC uses `rpc-call` / `rpc-receipt` JSON envelopes.
- v2 RIDL uses discriminated `rpc-frame` call/data/end/error/cancel frames.

They share schema vocabulary and review discipline, but they do not share a
wire frame. A consumer must never decode a v2 frame as v1 or vice versa.

## Authority and coupling

TypeSpec is the semantic and wire authority for shared RPC envelopes. JSON
Schema Draft 2020-12 is the committed runtime-admission projection/profile, and
Protobuf is the committed binary/streaming projection plus field-number ledger.
The projections are release vetoes when they drift, but they cannot redefine the
TypeSpec authority. They remain reviewed in source control while current
emitters cannot preserve every closed-world, conditional, and proto3
compatibility edge. Generation must never be used merely to make a drift check
green.

Reserved TypeSpec property identifiers use the language's backtick escaping,
for example `` `op` ``. This keeps the compiler input and the deliberately
small audited cross-check grammar identical; CI compiles the actual TypeSpec
source before any generated artifact is trusted.

For v1, `scripts/rpc-contract-bundle.py` turns one route map into one normalized
semantic contract. A SHA-256 over that canonical object binds all of these
outputs:

- OpenAPI 3.1;
- OpenRPC 1.3;
- Connect JSON unary discovery;
- JSON Hyper-Schema links;
- Rust route keys;
- TypeScript route keys and types;
- Dart route metadata;
- Gleam route keys;
- Go route metadata.

Each docs operation has an `x-ores-rpc` extension containing the map key,
contract digest, transports, TCP framing, delivery mode, alias, and opto-sync
queue metadata. Each generated language surface includes the same digest and a
complete mechanism manifest. A deployment may compare its compiled digest to
the served docs digest and fail closed before accepting traffic when they
mismatch.

The digest intentionally ignores source filename, JSON formatting, and object
key order. It changes when an operation key, path, method, schema, transport,
framing, delivery, alias, binding, or queue contract changes.

The standalone v2 Rust reference runtime is a nested crate with its own
committed `runtime/rust/Cargo.lock`. CI always tests it with `--locked`; the
reference implementation therefore cannot silently resolve a different serde,
JSON, UUID, or transitive dependency graph from the one reviewed here.

## Admission gates

The repository accepts an RPC change only when all of these hold:

1. The TypeSpec files compile with the pinned compiler.
2. Buf formats, lints, and compiles every Protobuf source.
3. The legacy structural cross-check passes.
4. The strict audit proves missing constraints are mismatches, expected deltas
   are an exact allow-list, every Proto assignment was parsed, field numbers are
   unique and locked, enum values match the lock, and the declaration set is
   reviewed.
5. Every embedded request, response, path, query, and error schema is a valid
   Draft 2020-12 schema.
6. Every path template variable is declared and required.
7. Alias chains are acyclic.
8. Connect keys bind exactly to `/package.Service/Key` and remain POST-only.
9. OpenAPI, OpenRPC, Connect, Hyper-Schema, and all five v1 language surfaces
   contain one matching contract digest and full mechanism metadata.
10. The v2 RIDL emitter set remains exactly Dart, Gleam, Go, Kotlin, Python,
    Rust, Swift, and TypeScript, with its existing golden and malformed corpus.
11. The Rust v1 crate and nested v2 reference runtime both pass against their
    committed lockfiles.

## Commands

```sh
python3 scripts/test_cross_check_rpc_idl.py -v
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_audit_rpc_idl.py -v
python3 scripts/audit-rpc-idl.py
python3 scripts/test_rpc_contract_bundle.py -v
python3 scripts/rpc-contract-bundle.py --check
cargo test --manifest-path rust/Cargo.toml --all-features --locked
cargo test --manifest-path runtime/rust/Cargo.toml --locked
```

To inspect generated artifacts without committing them:

```sh
python3 scripts/rpc-contract-bundle.py \
  --map examples/rpc-transports.route-map.json \
  --out /tmp/ores-rpc-contracts \
  --check
```

The bundle generator is write-free unless `--out` is supplied, and CI writes
only beneath the runner temporary directory. Generated artifacts are not proof
of deployment, transport availability, authentication, authorization, or a
healthy origin.
