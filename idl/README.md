# RPC IDL peer authorities and discrepancy gates

TypeSpec and JSON Schema/OpenAPI are co-equal, human-authored, top-level
contract authorities. The rejected topology is:

```text
TypeSpec -> JSON Schema/OpenAPI/Protobuf -> clients
```

The required paired topology is:

```text
TypeSpec
  -> normalized contract/persistence IR_T
  -> SQL_T where persistence mapping applies
  -> Protobuf/proto3
  -> gRPC
  -> wire clients

JSON Schema/OpenAPI
  -> normalized contract/persistence IR_J
  -> interfaces, language types, and runtime validators
  -> SQL_J where persistence mapping applies
  -> HTTP/write clients
```

Neither authored authority may be generated from, demoted beneath, silently
replaced by, or used as a fallback for the other. Cross-translations are
comparison evidence only:

```text
T -> J(T) -> T(J(T))
J -> T(J) -> J(T(J))
```

Generated witnesses must remain below generated paths and may never overwrite
`idl/typespec/` or the authored JSON Schema/OpenAPI sources in `json-schema/`.

The committed Protobuf definitions are the reviewed binary/streaming artifacts
of the TypeSpec lane. Their stable field-number ledger remains release-critical:
field reuse, renumbering, or unexplained semantic drift is a veto. Protobuf is
not a third top-level schema authority.

Per-service route-map JSON instances remain the canonical operation inventory
for the digest-bound API-document and language bundle. `declarative-migrations`
(`dpm`) is the SQL apply engine, not an RPC author. This crate does not open
NATS, opto-sync, or ores-otel.

See [`../docs/adr/0001-peer-schema-authority.md`](../docs/adr/0001-peer-schema-authority.md)
for the binding decision, discrepancy protocol, and execution-receipt contract.

## Do not mix stacks

- v1 unary: `rpc-call` / `rpc-receipt` (`op`)
- v2 RIDL: `rpc-frame` (`t`)
- HTTP never uses the v2 envelope; HTTP already supplies method, path, headers,
  status, and body framing

## Change workflow

1. A change may originate in either TypeSpec or JSON Schema/OpenAPI.
2. Identify every shared semantic and generated artifact affected by the change.
3. Reconcile both authored peer authorities in the same PR; update the reviewed
   Protobuf/gRPC artifacts when the TypeSpec transport lane is affected.
4. Generate independent candidate outputs and run the structural and strict
   semantic gates.
5. Compare normalized TypeSpec and JSON Schema/OpenAPI semantics, SQL/catalog
   candidates where applicable, Protobuf/gRPC compatibility, and generated
   language surfaces.
6. On any unexplained mismatch, emit a stable discrepancy fingerprint, enter
   `STOPPED_FOR_EVALUATION`, and block publication, migration, merge/promotion,
   release, and deployment. Never auto-pick a winner.
7. Add an expected delta only for a reviewed, representation-specific loss that
   cannot be encoded faithfully. The delta must be scoped, owned, tested, and
   reviewable.
8. Append new Protobuf field numbers in `idl/protobuf.lock.json`; never reuse or
   renumber a released field.
9. Publish the execution receipt required by the ADR. A run without a receipt
   cannot claim this repository was fully audited.

```sh
npm --prefix idl/typespec ci
npm --prefix idl/typespec run compile
buf format --diff --exit-code idl/protobuf
buf lint idl/protobuf
python3 scripts/test_cross_check_rpc_idl.py -v
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_audit_rpc_idl.py -v
python3 scripts/audit-rpc-idl.py
```

A future TypeSpec emitter may write derived candidates only below
`generated/idl/`. A JSON-Schema-to-TypeSpec converter follows the same rule.
Neither converter may promote output over an authored peer. Promotion of any
generated client, SQL candidate, or transport artifact requires semantic
convergence, stable output, human review, and a complete execution receipt.
