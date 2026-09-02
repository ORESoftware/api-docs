# oresoftware/api-docs

Feature work lands on a branch off the latest remote default. Do not rebase,
stash, or reset. Do not commit onto `main` unless a human named `main`.

Shared route-map API docs for every ORESoftware HTTP/JSON unary service.
Canonical GitHub repo: https://github.com/oresoftware/api-docs

For shared RPC envelope vocabulary, TypeSpec in `idl/typespec/` is the
**P0 semantic and wire authority**. JSON Schema in `json-schema/` is the **P1
runtime-admission projection/profile**; it preserves closed-world and conditional
validation and may veto a release when it drifts, but it must not redefine P0.
Protobuf in `idl/protobuf/` is the **P2 binary/streaming projection and
field-number compatibility ledger**; it may veto reuse or drift, but it also must
not redefine P0. Per-service route-map JSON instances remain the canonical
operation inventory consumed by the digest-bound bundle.

Projections stay committed and independently reviewed while current emitters
cannot reproduce every checked semantic exactly. Begin a shared-envelope change
in TypeSpec, reconcile the JSON Schema and Protobuf projections in the same PR,
and document only intentional lossy edges in `idl/expected-deltas.json`.
`scripts/cross-check-rpc-idl.py` and `scripts/audit-rpc-idl.py` fail closed on
absent constraints, unparsed Proto fields, enum-ledger drift, unknown
declarations, and undeclared deltas. Never edit a projection merely to silence a
gate.

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
bundle. Changes to RPC IDL, projections, route maps, or language emitters must
run both strict audit suites and `rpc-contract-bundle.py --check`.

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
