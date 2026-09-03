# Digest-bound RPC and API-document generation

This repository has two intentionally separate RPC generations:

- v1 route-map RPC uses `rpc-call` / `rpc-receipt` JSON envelopes.
- v2 RIDL uses discriminated `rpc-frame` call/data/end/error/cancel frames.

They share schema vocabulary and review discipline, but they do not share a
wire frame. A consumer must never decode a v2 frame as v1 or vice versa.

## Authority and coupling

TypeSpec and the JSON Schema/OpenAPI track are **peer top-level contract
authorities**. Both are human-authored and independently reviewed. Neither is an
intermediate representation of, subordinate to, or allowed to overwrite the
other. TypeSpec projects toward SQL, Protobuf/gRPC, and wire clients. JSON
Schema/OpenAPI projects toward client interfaces/types, SQL, and write clients.
Protobuf remains a TypeSpec-derived binary/streaming projection plus the stable
field-number ledger.

Both authority tracks are release vetoes when they drift. Current emitters do
not preserve every closed-world, conditional, and proto3 compatibility edge, so
reviewed representation loss remains an exact allow-list in
`idl/expected-deltas.json`. Generation must never be used merely to make a drift
check green, and no gate may silently select one authority as the winner.

The machine policy in `idl/authority-contract.json` also requires comparison of
generated SQL and client-type manifests from both authority tracks and
schema/migration/constraint/relation manifests from Diesel and SeaORM. The
comparison utility is `scripts/compare-authority-artifacts.py`. Any unexpected
difference means **halt and evaluate**. The actual SQL and ORM emitters remain
explicitly `not_yet_materialized`; this repository must not claim their parity
until exact artifacts exist.

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
complete machine-readable mechanism manifest. A deployment may compare its
compiled digest to the served docs digest and fail closed before accepting
traffic when they mismatch.

The verifier does not accept string presence as evidence of coupling. It parses
every OpenAPI, OpenRPC, Connect, and Hyper-Schema operation back into normalized
`key + path + methods + x-ores-rpc` bindings, and compares that full multiset to
the route-map contract. It also extracts the embedded mechanism object from the
Rust, TypeScript, Dart, Gleam, and Go surfaces and compares the parsed object for
exact equality. A stale transport hidden in a comment, unused constant, copied
digest, or partially updated route table is therefore a release veto.

The digest intentionally ignores source filename, JSON formatting, and object
key order. It changes when an operation key, path, method, schema, transport,
framing, delivery, alias, binding, or queue contract changes.

The standalone v2 Rust reference runtime is a nested crate with its own
committed `runtime/rust/Cargo.lock`. CI always tests it with `--locked`; the
reference implementation therefore cannot silently resolve a different serde,
JSON, UUID, or transitive dependency graph from the one reviewed here.

## Admission gates

The repository accepts an RPC change only when all of these hold:

1. The peer-authority policy validates and no governing document reinstates a
   TypeSpec-over-JSON-Schema/OpenAPI hierarchy.
2. The TypeSpec files compile with the pinned compiler.
3. Buf formats, lints, and compiles every Protobuf source.
4. The structural and strict semantic cross-checks pass between the two peer
   authority tracks and their reviewed projections.
5. Missing constraints are mismatches, expected deltas are an exact allow-list,
   every Proto assignment is parsed, field numbers are unique and locked, enum
   values match the lock, and the declaration set is reviewed.
6. When SQL/type and ORM generators are materialized, their exact manifests pass
   `compare-authority-artifacts.py`; otherwise production promotion remains
   blocked for those paths.
7. Every embedded request, response, path, query, and error schema is a valid
   Draft 2020-12 schema.
8. Every path template variable is declared and required.
9. Alias chains are acyclic.
10. Connect keys bind exactly to `/package.Service/Key` and remain POST-only.
11. OpenAPI, OpenRPC, Connect, and Hyper-Schema round-trip to the exact normalized
    RPC operation bindings, including path, method, transport, framing, delivery,
    alias, and queue semantics.
12. Rust, TypeScript, Dart, Gleam, and Go expose one matching contract digest and
    a machine-readable mechanism object that parses back to the exact route-map
    contract.
13. The v2 RIDL emitter set remains exactly Dart, Gleam, Go, Kotlin, Python,
    Rust, Swift, and TypeScript, with its existing golden and malformed corpus.
14. The Rust v1 crate and nested v2 reference runtime both pass against their
    committed lockfiles.

## Commands

```sh
python3 scripts/test_validate_authority_contract.py -v
python3 scripts/validate-authority-contract.py
python3 scripts/test_compare_authority_artifacts.py -v
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
