# v1 clients

| Language | Package location |
| --- | --- |
| Rust | [`rust/`](rust/README.md) — `ores-api-docs-client`, a transport-neutral facade over the existing `../rust` core |
| TypeScript | [`typescript/`](typescript/) |
| Dart / Flutter | [`dart/`](dart/) |
| Go | [`go/`](go/) |
| Gleam | [`gleam/`](gleam/) |

Rust's existing `../rust` server/library crate remains available. The new client
facade disables that dependency's default Axum feature and reuses its validators
and types instead of defining a second contract implementation.

These packages concern the v1 route-map/RPC stack. RIDL v2 streaming runtimes
live separately under `../runtime`. Generated route-specific artifacts remain
owned by the digest-bound contract bundle, not by a client-local generator.
