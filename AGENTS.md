# oresoftware/api-docs

Work on `main`. Do not rebase, stash, or reset.

Shared route-map API docs for every ORESoftware HTTP/JSON unary service.
Canonical GitHub repo: https://github.com/oresoftware/api-docs

JSON Schema in `json-schema/` is the contract. Rust crate `ores-api-docs`
validates and serves `/docs/api`, `/api/docs`, `/api/docs.json` (k8s-cluster
aliases) plus OpenAPI / OpenRPC / Connect projections. Clients: TypeScript,
Dart, Gleam. `scripts/generate-routes.py` emits compile-time key objects in
Rust, TypeScript, Dart, and Gleam. Pin this repo as zed package
`oresoftware/api-docs` (`.zpkg.toml`); do not add an opto-sync or ores-otel zed
dependency here.
Route maps travel between devices via opto-sync envelopes (scope
`ores.api-docs.route-map`); opto-sync itself must not depend on this crate.
RPC call/receipt frames are transport-neutral JSON (HTTP, TCP NDJSON,
WebSocket). Telemetry is an attribute bag ores-otel may copy; no crate edge.

Do not put secrets in this repo. Do not load Scalar/unpkg/CDN into docs HTML.
