# oresoftware/api-docs

Feature work lands on a branch off the latest remote default. Do not rebase,
stash, or reset. Do not commit onto `main` unless a human named `main`.

Shared route-map API docs for every ORESoftware HTTP/JSON unary service.
Canonical GitHub repo: https://github.com/oresoftware/api-docs

JSON Schema in `json-schema/` is the contract. Two stacks share this repo;
do not mix their frames:

- **v1** (`schema_version` `1.0.0`): `route-map.schema.json` plus
  `rpc-call.schema.json` / `rpc-receipt.schema.json`. Codegen:
  `scripts/generate-routes.py`. Unary JSON on HTTP, TCP (NDJSON or
  length-prefixed), WebSocket, and NATS (declared on the map; this crate
  does not open NATS).
- **v2** (`schema_version` `2.x`): `route-map-v2.schema.json` plus
  `rpc-frame.schema.json` (`t`: call / data / end / error / cancel). Codegen:
  `python3 -m ridl` / `python3 -m ridl.cli` (zed bin `scripts/ridl`).
  RIDL uses PEP 604 unions; require Python ≥ 3.10 locally, 3.12 in CI.

Rust crate `ores-api-docs` validates and serves `/docs/api`, `/api/docs`,
`/api/docs.json` (k8s-cluster aliases) plus OpenAPI / OpenRPC / Connect
projections. Clients: TypeScript, Dart, Gleam. Gleam CI is 1.14+. RIDL and
`scripts/ridl` run on Python ≥ 3.10; GitHub Actions pins 3.12. Pin this
repo as zed package `oresoftware/api-docs` `^2.0.0` (`.zpkg.toml`); do not
add an opto-sync or ores-otel zed dependency here.
Route maps travel between devices via opto-sync envelopes (scope
`ores.api-docs.route-map`); opto-sync itself must not depend on this crate.
Telemetry is an attribute bag ores-otel may copy; no crate edge.

Do not put secrets in this repo. Do not load Scalar/unpkg/CDN into docs HTML.
