# RPC IDL authority and checked projections

There is one authority for shared RPC envelope semantics and two committed,
release-vetoing projections. This avoids both silent generator loss and three
competing sources of truth.

| Tier | Source | Role |
| --- | --- | --- |
| **P0** | TypeSpec in `idl/typespec/` | Semantic and wire authority for shared fields, constraints, unions, and enums |
| **P1** | JSON Schema in `json-schema/` | Runtime-admission projection/profile, including closed-world and conditional validation |
| **P2** | Protobuf in `idl/protobuf/` | Binary/streaming projection plus stable field-number compatibility ledger |

P1 and P2 can veto a release when they drift, omit a required semantic, or
reuse compatibility state. They cannot originate a conflicting semantic or
overrule P0. They remain committed and independently reviewed because current
emitters do not preserve every checked JSON Schema and proto3 edge exactly.
Intentional representation loss is an explicit allow-list in
`idl/expected-deltas.json`; it is never a blanket exemption.

Per-service route-map JSON instances remain the canonical operation inventory
for the digest-bound API-document and language bundle. `declarative-migrations`
(`dpm`) is the SQL apply engine, not an RPC author. This crate does not open
NATS, opto-sync, or ores-otel.

## Do not mix stacks

- v1 unary: `rpc-call` / `rpc-receipt` (`op`)
- v2 RIDL: `rpc-frame` (`t`)
- HTTP never uses the v2 envelope; HTTP already supplies method, path, headers,
  status, and body framing

## Change workflow

1. Change the shared semantic fact in TypeSpec first.
2. Reconcile the JSON Schema runtime profile and, when binary identity is
   affected, the Protobuf projection in the same PR.
3. Run the structural and strict semantic gates.
4. Fix unexpected drift. Add an expected delta only for a reviewed,
   representation-specific loss that cannot be encoded faithfully.
5. Append new Protobuf field numbers in `idl/protobuf.lock.json`; never reuse or
   renumber a released field.

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

A future TypeSpec emitter may write candidate artifacts only below
`generated/idl/projections/`. Promotion into `json-schema/` or `idl/protobuf/`
requires semantic equivalence under both gates, stable output, and human review;
an emitter must never overwrite a release projection merely to make CI green.
