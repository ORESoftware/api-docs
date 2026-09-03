# RPC IDL peer authorities and checked projections

TypeSpec and the JSON Schema/OpenAPI track are peer top-level authorities. Both
are human-authored, independently reviewed, and release-vetoing. Neither is an
intermediate representation of the other, and neither may automatically
overwrite or overrule the other.

| Authority or projection | Source | Role |
| --- | --- | --- |
| **TypeSpec authority** | `idl/typespec/` | Shared fields, constraints, unions, and enums; projects to SQL, Protobuf/gRPC, and wire clients |
| **JSON Schema/OpenAPI authority** | `json-schema/` plus authored route-map operation inventory | Runtime admission, closed-world and conditional validation, API operations; projects to client interfaces/types, SQL, and write clients |
| **Protobuf projection** | `idl/protobuf/` | TypeSpec-derived binary/streaming representation plus stable field-number compatibility ledger |

Both authorities can veto a release when they conflict, omit a required
semantic, or produce different normalized SQL or client types. Protobuf can veto
reuse or projection drift. Intentional representation loss is an exact
allow-list in `idl/expected-deltas.json`; it is never a blanket exemption or a
way to choose a winning authority.

Per-service route-map JSON instances remain the canonical operation inventory
for the digest-bound API-document and language bundle. `declarative-migrations`
(`dpm`) is the SQL apply engine, not an RPC author. This crate does not open
NATS, opto-sync, or ores-otel.

The machine-readable policy is `idl/authority-contract.json`. Current RPC model
cross-checking and digest-bound API-document/client generation are implemented.
The TypeSpec SQL emitter, JSON Schema/OpenAPI SQL emitter, and Diesel/SeaORM
artifact producer are marked `not_yet_materialized` until exact manifests exist.
`scripts/compare-authority-artifacts.py` compares those manifests. A mismatch in
SQL, client types, schema, migrations, constraints, or relations means **halt
and evaluate**; CI must not pick a source automatically.

## Do not mix stacks

- v1 unary: `rpc-call` / `rpc-receipt` (`op`)
- v2 RIDL: `rpc-frame` (`t`)
- HTTP never uses the v2 envelope; HTTP already supplies method, path, headers,
  status, and body framing

## Change workflow

1. Change the semantic fact in both peer authority tracks in the same PR.
2. Generate TypeSpec projections (SQL, Protobuf/gRPC, wire clients) and JSON
   Schema/OpenAPI projections (interfaces/types, SQL, write clients).
3. Compare normalized models, generated SQL, and generated client types. Compare
   Diesel and SeaORM schema, migrations, constraints, and relations.
4. On any unexpected discrepancy, halt and evaluate. Do not overwrite either
   authority or ORM output to force a green build.
5. Reconcile the Protobuf projection and append new field numbers in
   `idl/protobuf.lock.json`; never reuse or renumber a released field.
6. Add an expected delta only for a reviewed, representation-specific loss that
   cannot be encoded faithfully.

```sh
npm --prefix idl/typespec ci
npm --prefix idl/typespec run compile
buf format --diff --exit-code idl/protobuf
buf lint idl/protobuf
python3 scripts/test_validate_authority_contract.py -v
python3 scripts/validate-authority-contract.py
python3 scripts/test_compare_authority_artifacts.py -v
python3 scripts/test_cross_check_rpc_idl.py -v
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_audit_rpc_idl.py -v
python3 scripts/audit-rpc-idl.py
```

Candidate artifacts may be written only below `generated/idl/projections/` or a
runner temporary directory. Promotion into a reviewed authority or projection
requires semantic equivalence under every applicable gate, stable output, and
human review. A generator must never overwrite reviewed source merely to make
CI green.
