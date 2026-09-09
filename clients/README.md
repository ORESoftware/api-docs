# Client packages

Client-facing packages live here so each supported language has a discoverable
entry point. Service-specific generated route surfaces remain in `../generated/`
and are produced by the existing digest-bound contract bundle.

| Language | Package directory |
| --- | --- |
| Rust | [`rust/`](rust/README.md) — `ores-api-docs-client` |
| TypeScript | [`typescript/`](typescript/) |
| Dart / Flutter | [`dart/`](dart/) |
| Gleam | [`gleam/`](gleam/) |
| Go | [`go/`](go/) |

The Rust client is a facade over [`../rust/`](../rust/), the existing
`ores-api-docs` core/server crate, with the core's default Axum feature disabled.
It shares the same types and validators rather than copying them. Existing
imports of `ores-api-docs` continue to work. Use the whole-repository zed target
or a commit-pinned Git dependency; copying only `clients/rust` breaks its
relative dependency and embedded-asset layout.

Do not confuse these v1 call/receipt APIs with the separate RIDL v2 streaming
runtime under `../runtime/`. Authored TypeSpec and JSON Schema/OpenAPI remain
independent contract authorities; generated clients and documentation must
continue to agree on the contract bundle digest.
