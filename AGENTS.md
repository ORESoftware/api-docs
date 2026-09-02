# oresoftware/api-docs

Feature work lands on a branch off the latest remote default. Do not rebase,
stash, or reset. Do not commit onto `main` unless a human named `main`.

Shared route-map API docs for every ORESoftware HTTP/JSON unary service.
Canonical GitHub repo: https://github.com/oresoftware/api-docs

For shared RPC envelope vocabulary, TypeSpec in `idl/typespec/` and JSON
Schema/OpenAPI in `json-schema/` and `openapi/` are **independent, top-level,
human-authored contract authorities**. Neither is an intermediate
representation of, subordinate to, or an automatic winner over the other.

The TypeSpec lane projects PostgreSQL SQL, Protobuf, gRPC, wire types, and wire
clients. The JSON Schema/OpenAPI lane projects runtime interfaces, client types,
PostgreSQL SQL, and write clients. Protobuf in `idl/protobuf/` is the TypeSpec
lane's binary/streaming projection and field-number compatibility ledger, not a
third semantic authority. Per-service route-map JSON instances remain the
canonical operation inventory consumed by the digest-bound bundle.

Projections stay committed and independently reviewed while current emitters
cannot reproduce every checked semantic exactly. Begin a change in the relevant
human-authored authority; when the semantic belongs to both lanes, change both
sources in the same PR. Normalize and compare the independently emitted SQL and
type surfaces before admitting either. Generate Diesel and SeaORM independently
from admitted SQL, compare them with each other, and compare both back to the
normalized PostgreSQL catalog contract. No source-order, timing, historical
P0/P1/P2 rank, or generator preference may select a winner.

`idl/authority-graph.json` and `scripts/check-authority-graph.py` enforce this
topology. Any undeclared SQL, type, Protobuf, Diesel, or SeaORM discrepancy must
pause generation, commit, merge, release, and deployment until a reviewer has a
normalized semantic diff, records the decision, adds an intentional lossy edge
to `idl/expected-deltas.json` when appropriate, and reruns every parity gate.
`scripts/cross-check-rpc-idl.py` and `scripts/audit-rpc-idl.py` also fail closed
on absent constraints, unparsed Proto fields, enum-ledger drift, unknown
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
run the authority-graph gate, both strict audit suites, and
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
