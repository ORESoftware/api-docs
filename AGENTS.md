# oresoftware/api-docs

Work on `main`. Do not rebase, stash, or reset.

Shared route-map API docs for every ORESoftware HTTP/JSON unary service.
Canonical GitHub repo: https://github.com/oresoftware/api-docs

JSON Schema in `json-schema/` is the contract. Rust crate `ores-api-docs`
validates and serves `/docs/api`, `/api/docs`, `/api/docs.json` (k8s-cluster
aliases) plus OpenAPI / OpenRPC / Connect projections. Clients: TypeScript,
Dart, Gleam.

Do not put secrets in this repo. Do not load Scalar/unpkg/CDN into docs HTML.
