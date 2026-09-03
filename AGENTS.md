# oresoftware/api-docs

Feature work lands on a branch off the latest remote default. Do not rebase,
stash, or reset. Do not commit onto `main` unless a human named `main`.

Shared route-map API docs for every ORESoftware HTTP/JSON unary service.
Canonical GitHub repo: https://github.com/oresoftware/api-docs

## Peer schema authorities

TypeSpec in `idl/typespec/` and JSON Schema/OpenAPI in `json-schema/` are
**co-equal, human-authored, top-level contract authorities**. Neither is
generated from, subordinate to, a fallback for, or allowed to overwrite the
other.

The required paired lanes are:

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

A TypeSpec-emitted JSON Schema or a JSON-Schema-derived TypeSpec file is a
generated comparison witness only. It must live below a generated path and must
never replace either authored authority. The committed Protobuf definitions and
field-number ledger are the reviewed binary/streaming artifacts of the TypeSpec
lane; they may veto drift or field-number reuse, but they are not a third
top-level schema authority.

Per-service route-map JSON instances remain the canonical **operation
inventory** consumed by the digest-bound bundle. That inventory role does not
make a generated schema or transport projection authoritative over the two
peer schema lanes.

A shared-envelope change may originate in either peer lane. Reconcile every
affected authored source and reviewed transport artifact in the same PR. Run
`scripts/cross-check-rpc-idl.py` and `scripts/audit-rpc-idl.py`. Any unexplained
difference in fields, required/optional/null semantics, constraints, encoded
names, SQL/catalog output, Protobuf/gRPC behavior, generated types/clients, or
compatibility state enters `STOPPED_FOR_EVALUATION` and blocks generation,
publication, migration, merge/promotion, release, and deployment. No lane wins
by fallback or precedence. Record an explicit discrepancy decision before
editing either authority to resolve it.

Only reviewed representation-specific losses belong in
`idl/expected-deltas.json`. Never edit an authored peer or reviewed transport
artifact merely to silence a gate.

Every scheduled or manual schema sweep must publish an execution receipt with:

- UTC start/end timestamps and actor, workflow, or job identity;
- exact organization/repository/service/file scope, including pagination or
  caps and explicit exclusions;
- source commit SHAs or content digests plus tool versions and options;
- checks executed and links to CI runs, logs, reports, and generated artifacts;
- result, discrepancy fingerprints, owners, and resolution state;
- read-only exceptions or inaccessible surfaces.

A sweep without that receipt is incomplete and may not claim repository or
fleet coverage.

Two stacks share this repo; do not mix their frames:

- **v1** (`schema_version` `1.0.0`): `route-map.schema.json` plus
  `rpc-call.schema.json` / `rpc-receipt.schema.json`. Codegen:
  `scripts/generate-routes.py`. Unary JSON on HTTP, TCP (NDJSON or
  length-prefixed), WebSocket, and NATS (declared on the map; this crate
  does not open NATS).
- **v2** (`schema_version` `2.x`): `route-map-v2.schema.json` plus
  `rpc-frame.schema.json` (`t`: call / data / end / error / cancel). Codegen:
  `python3 -m ridl` / `python3 -m ridl.cli` (zed bin `scripts/ridl`).
  RIDL uses PEP 604 unions; require Python >= 3.10 locally, 3.12 in CI.

`scripts/rpc-contract-bundle.py` is the coupling boundary for v1. It parses one
route map once, validates every embedded Draft 2020-12 schema, and emits
OpenAPI, OpenRPC, Connect, Hyper-Schema, plus Rust, TypeScript, Dart, Gleam, and
Go route surfaces. Every artifact carries the same semantic SHA-256 and the
same transport, framing, delivery, alias, and opto-sync metadata. Never add a
docs-only or language-only generator path that bypasses this digest-bound
bundle. Changes to RPC IDL, JSON Schema/OpenAPI, Protobuf, route maps, or
language emitters must run both strict audit suites and
`rpc-contract-bundle.py --check`.

Rust crate `ores-api-docs` validates and serves `/docs/api`, `/api/docs`,
`/api/docs.json` (k8s-cluster aliases) plus OpenAPI / OpenRPC / Connect
projections. v1 package clients are TypeScript, Dart, and Gleam; the coupled
bundle also emits Rust and Go. RIDL v2 owns its reviewed eight-emitter set:
Dart, Gleam, Go, Kotlin, Python, Rust, Swift, and TypeScript. Gleam CI is 1.14+.
GitHub Actions pins Python 3.12, Rust, Node, Go, Dart, Erlang/Gleam, Buf, and all
third-party actions by immutable versions/commit SHAs.

Pin this repo as zed package `oresoftware/api-docs` `^2.0.0` (`.zpkg.toml`);
do not add an opto-sync or ores-otel zed dependency here. Route maps travel
between devices via opto-sync envelopes (scope `ores.api-docs.route-map`);
opto-sync itself must not depend on this crate. Telemetry is an attribute bag
ores-otel may copy; no crate edge.

Do not put secrets in this repo. Do not load Scalar/unpkg/CDN into docs HTML.
