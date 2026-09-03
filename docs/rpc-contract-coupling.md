# Digest-bound RPC and API-document generation

This repository has two intentionally separate RPC generations:

- v1 route-map RPC uses `rpc-call` / `rpc-receipt` JSON envelopes.
- v2 RIDL uses discriminated `rpc-frame` call/data/end/error/cancel frames.

They share schema vocabulary and review discipline, but they do not share a
wire frame. A consumer must never decode a v2 frame as v1 or vice versa.

## Peer authority and coupling

TypeSpec and JSON Schema/OpenAPI are co-equal, human-authored, top-level
contract authorities. Neither is a projection of, subordinate to, or a fallback
for the other.

The TypeSpec lane independently produces normalized contract/persistence IR,
SQL where persistence mapping applies, Protobuf/proto3, gRPC, and wire clients.
The JSON Schema/OpenAPI lane independently produces normalized
contract/persistence IR, interfaces, language types, runtime validators, SQL
where persistence mapping applies, and HTTP/write clients.

The committed Protobuf definitions and field-number ledger are reviewed
binary/streaming artifacts of the TypeSpec lane. They remain release vetoes for
wire drift, field-number reuse, or incompatible representation, but they are not
a third top-level schema authority. TypeSpec-to-JSON-Schema and
JSON-Schema-to-TypeSpec conversions are generated comparison witnesses only and
must never overwrite either authored authority.

Reserved TypeSpec property identifiers use the language's backtick escaping,
for example `` `op` ``. This keeps the compiler input and the deliberately
small audited cross-check grammar identical; CI compiles the actual TypeSpec
source before any generated artifact is trusted.

A shared semantic change may originate in either peer lane. The same PR must
reconcile both authored sources and every affected reviewed transport or client
artifact. Any unexplained mismatch enters `STOPPED_FOR_EVALUATION`; no source,
generator, transport, or ORM wins by precedence. Generation must never be used
merely to make a drift check green.

The binding decision and receipt contract are in
[`adr/0001-peer-schema-authority.md`](adr/0001-peer-schema-authority.md).

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

## Admission and discrepancy gates

The repository accepts an RPC change only when all of these hold:

1. The independently authored TypeSpec files compile with the pinned compiler.
2. Every independently authored JSON Schema/OpenAPI source validates under its
   pinned dialect, vocabulary, validator, and options.
3. Buf formats, lints, and compiles every Protobuf source, and the TypeSpec-lane
   transport comparison preserves field numbers and wire behavior.
4. The structural cross-check and strict audit compare the peer semantic views;
   missing constraints are mismatches, not neutral omissions.
5. Expected representation deltas form an exact allow-list with stable scope,
   ownership, tests, and review status.
6. Every Proto assignment is parsed, field numbers are unique and locked, enum
   values match the lock, and the declaration set is reviewed.
7. Independently generated SQL candidates are compared by normalized catalog
   read-back wherever this contract carries persistence mapping.
8. Generated client/type surfaces from both lanes pass the same positive,
   negative, boundary, and compatibility fixtures.
9. Every embedded request, response, path, query, and error schema is a valid
   Draft 2020-12 schema.
10. Every path template variable is declared and required.
11. Alias chains are acyclic.
12. Connect keys bind exactly to `/package.Service/Key` and remain POST-only.
13. OpenAPI, OpenRPC, Connect, and Hyper-Schema round-trip to the exact normalized
    RPC operation bindings, including path, method, transport, framing, delivery,
    alias, and queue semantics.
14. Rust, TypeScript, Dart, Gleam, and Go expose one matching contract digest and
    a machine-readable mechanism object that parses back to the exact route-map
    contract.
15. The v2 RIDL emitter set remains exactly Dart, Gleam, Go, Kotlin, Python,
    Rust, Swift, and TypeScript, with its existing golden and malformed corpus.
16. The Rust v1 crate and nested v2 reference runtime both pass against their
    committed lockfiles.
17. No unexplained mismatch remains. A discrepancy blocks generation,
    publication, migration, automatic merge/promotion, package release,
    consumer rollout, and deployment until human evaluation reconciles both
    peer authorities.
18. The run publishes a complete execution receipt. A green command without a
    receipt is not evidence of repository or fleet coverage.

## Execution receipt

Every manual or scheduled run records, at minimum:

- UTC start/end timestamps and actor, workflow, or job identity;
- organizations, repositories, branches, commits, services, files, and exported
  Linear/GitHub records searched;
- pagination, caps, exclusions, inaccessible surfaces, and read-only exceptions;
- source digests, compiler/emitter/validator/comparator versions, and options;
- exact checks executed plus CI, log, report, and artifact links;
- final status and every discrepancy fingerprint, owner, and resolution state.

Receipts must be machine-readable, immutable or content-addressed, and linked
from the relevant GitHub and Linear records. Scheduled sweeps that do not emit
receipts are incomplete and may not claim zero findings.

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
of deployment, transport availability, authentication, authorization, a
healthy origin, or fleet coverage. The execution receipt must distinguish
checks actually run from planned or inferred checks.
