# RPC IDL authorities and checked projections

TypeSpec and JSON Schema/OpenAPI are independent, peer top-level source
authorities. Neither is an intermediate representation of the other.

| Lane | Source | Direct outputs |
| --- | --- | --- |
| **A** | TypeSpec in `idl/typespec/` | SQL, Protobuf, gRPC, and wire clients |
| **B** | JSON Schema in `json-schema/` plus OpenAPI operation contracts | language interfaces/types, SQL, and write clients |

Protobuf is a committed downstream projection of TypeSpec and the stable
binary/streaming field-number compatibility ledger. OpenAPI is a committed
downstream HTTP/document projection of the JSON Schema/OpenAPI lane. Both can
veto a release when they omit facts, drift, or reuse compatibility state; they
cannot silently override their upstream authority.

The two source lanes intentionally overlap. Their generated SQL and language
model facts are compared instead of selecting a preferred source. Diesel and
SeaORM are also peer projections and cross-check each other. Differences in
names, scalar types, requiredness/nullability, defaults, enum values, bounds,
patterns, keys, foreign keys, uniqueness, checks, indexes, relations, cascade
actions, RPC operations, requests, responses, or errors trigger a hard
**pause-and-evaluate** gate.

There is no automatic winner. An intentional representation loss must be an
exact JSON Pointer exception with a reason, owner, and expiry. Wildcards,
expired exceptions, and exceptions that no longer match a real difference are
errors.

The required topology is machine-readable in `idl/source-authorities.json` and
validated by `scripts/audit-schema-convergence.py`. Generator repositories emit
producer-neutral `ores.schema-convergence.v1` manifests containing canonical
SQL, type/interface, ORM, and RPC facts. The gate compares those manifests and
can write a structured discrepancy report for GitHub and Linear automation.

Per-service route-map JSON instances remain the canonical operation inventory
for the digest-bound API-document and language bundle. `declarative-migrations`
(`dpm`) is the SQL apply engine, not a source authority. This crate does not
open NATS, opto-sync, or ores-otel.

## Do not mix stacks

- v1 unary: `rpc-call` / `rpc-receipt` (`op`)
- v2 RIDL: `rpc-frame` (`t`: call / data / end / error / cancel)
- HTTP never uses the v2 envelope; HTTP already supplies method, path, headers,
  status, and body framing

## Change workflow

1. Identify which top-level lane originates the changed fact. A shared fact may
   require coordinated edits in both lanes.
2. Generate candidate SQL, models/types/interfaces, RPC projections, and ORM
   artifacts into isolated candidate directories; never overwrite reviewed
   outputs before comparison.
3. Reconcile TypeSpec-derived Protobuf/gRPC and JSON-Schema-derived OpenAPI in
   the same PR.
4. Normalize and compare the TypeSpec and JSON Schema/OpenAPI convergence
   manifests.
5. Cross-check Diesel and SeaORM manifests independently.
6. On any unexplained difference, stop generation, publish the discrepancy
   report, and evaluate. Do not continue to clients or database application.
7. Add an expected delta only for reviewed, representation-specific loss that
   cannot be encoded faithfully; give it an owner and near-term expiry.
8. Append new Protobuf field numbers in `idl/protobuf.lock.json`; never reuse or
   renumber a released field.
9. Only after every gate passes, promote generated artifacts and run the
   digest-bound documentation/language bundle.

```sh
python3 scripts/test_audit_schema_convergence.py -v
python3 scripts/audit-schema-convergence.py
npm --prefix idl/typespec ci
npm --prefix idl/typespec run compile
buf format --diff --exit-code idl/protobuf
buf lint idl/protobuf
python3 scripts/test_cross_check_rpc_idl.py -v
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_audit_rpc_idl.py -v
python3 scripts/audit-rpc-idl.py
python3 scripts/test_rpc_contract_bundle.py -v
python3 scripts/rpc-contract-bundle.py --check
```

Candidate emitters may write only below generated candidate directories. A
promotion requires deterministic output, convergence under all gates, and human
review. No emitter may rewrite a reviewed projection merely to make CI green.
