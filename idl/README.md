# RPC IDL peer authorities and checked projections

TypeSpec and JSON Schema/OpenAPI are independent, top-level, human-authored
contract authorities. Neither lane is generated from, subordinate to, or
permitted to silently overwrite the other.

| Authority lane | Sources | Required projections |
| --- | --- | --- |
| **TypeSpec** | `idl/typespec/` | PostgreSQL SQL, Protobuf, gRPC, wire types, and wire clients |
| **JSON Schema/OpenAPI** | `json-schema/` and `openapi/` | Runtime interfaces, client types, PostgreSQL SQL, and write clients |

Protobuf in `idl/protobuf/` is the TypeSpec lane's binary/streaming projection
and stable field-number compatibility ledger. It is committed and independently
reviewed because current emitters do not preserve every checked proto3 edge
exactly, but it is not a third semantic authority.

The SQL emitted by both authority lanes must normalize to the same PostgreSQL
semantic IR. Their generated type surfaces must normalize to the same
language-neutral type IR. Only an artifact that passes those gates is admitted.
Diesel and SeaORM are then generated independently from admitted SQL, compared
with each other, and compared back to the normalized PostgreSQL catalog
contract.

Any undeclared discrepancy pauses generation, commit, merge, release, and
deployment. Source order, elapsed time, generator preference, and historical
P0/P1/P2 labels never choose a winner. Intentional representation loss is an
explicit reviewed allow-list in `idl/expected-deltas.json`; it is never a
blanket exemption.

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

1. Change the semantic fact in the authority lane that owns it. When the fact is
   shared by both lanes, edit both human-authored sources in the same PR.
2. Regenerate each lane's SQL and type projections plus TypeSpec's
   Protobuf/gRPC projections.
3. Run the authority-graph, structural, and strict semantic gates.
4. Stop on unexpected SQL, type, Protobuf, Diesel, or SeaORM drift. Add an
   expected delta only for reviewed representation-specific loss that cannot be
   encoded faithfully.
5. Append new Protobuf field numbers in `idl/protobuf.lock.json`; never reuse or
   renumber a released field.

```sh
python3 scripts/check-authority-graph.py
python3 -m unittest scripts/test_check_authority_graph.py
npm --prefix idl/typespec ci
npm --prefix idl/typespec run compile
buf format --diff --exit-code idl/protobuf
buf lint idl/protobuf
python3 scripts/test_cross_check_rpc_idl.py -v
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_audit_rpc_idl.py -v
python3 scripts/audit-rpc-idl.py
```

A future emitter may write candidate artifacts only below
`generated/idl/projections/`. Promotion into committed projections requires
semantic equivalence under every gate, stable output, and human review; an
emitter must never overwrite a release artifact merely to make CI green.
