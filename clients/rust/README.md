# Rust client

`ores-api-docs-client` is the client-facing Rust package for route maps,
documentation discovery, path/query encoding, and validated RPC v1 envelopes.
It re-exports the existing `ores-api-docs` implementation from `../../rust`
with `default-features = false`; it does not fork models or validators and does
not enable the core crate's optional Axum server adapter.

## Why this package exists

Rust was implemented in the repository's top-level `rust/` crate, while the
other packaged languages were visible under `clients/`. The missing Rust
entry was a packaging/discoverability gap, not a lack of Rust RPC support.
This additive facade makes the client entry point explicit without moving or
breaking the existing server/library crate.

## Consume it

For a repository installed with zed-pkg's **whole-repository** target, preserve
its directory layout and point Cargo at the client package:

```toml
[dependencies]
ores-api-docs-client = { path = "path/to/api-docs/clients/rust" }
```

For a Git dependency, select package `ores-api-docs-client` from
`https://github.com/ORESoftware/api-docs` and pin the reviewed commit containing
this package with Cargo's `rev` field. Neither Rust package is currently
published to crates.io (`publish = false`).

Do not install only the `clients/rust` subtree: its path dependency needs
`rust/`, and the core embeds schemas and projection assets from the repository.
The existing zed `rust` target remains unchanged for compatibility; use the
`repository` target for the new facade. Cargo features are additive: another
dependency can still enable the core's Axum feature in the same build. The
client's isolated CI checks that this package alone does not enable it.

```rust
use ores_api_docs_client::{
    assert_rpc_v1_receipt_for_call, decode_rpc_v1_call, decode_rpc_v1_receipt,
};

fn validate_reply() -> Result<(), Box<dyn std::error::Error>> {
    let call = decode_rpc_v1_call(
        br#"{"v":1,"op":"call","id":"request-1","key":"get_item"}"#,
    )?;
    let receipt = decode_rpc_v1_receipt(
        br#"{"v":1,"op":"receipt","id":"request-1","key":"get_item","ok":true}"#,
    )?;
    assert_rpc_v1_receipt_for_call(&call, &receipt)?;
    Ok(())
}
```

Applications own network I/O, credentials, timeouts, retry/idempotency policy,
and service-specific request/response validation. This package validates the
RPC envelope; it does not imply that arbitrary operation bodies satisfy the
service contract. Use the existing digest-bound generated route surfaces for
operation keys and metadata. A decoded envelope is not authorization.

## Contract boundaries

- Re-exported types are the same Rust types as the core crate's types.
- Authored TypeSpec and JSON Schema/OpenAPI remain independent authorities.
- No generated file or emitter is introduced or edited by this facade.
- RPC v1 (`v: 1`, `op: call/receipt`) is separate from RIDL v2 streaming frames.
- HTTP/TCP/WebSocket/NATS declarations and framing helpers do not open sockets.
- Axum routers, HTML serving, and the server catalog are not exported here.
- No opto-sync or ores-otel dependency is added; existing envelopes/attributes
  remain interoperable data contracts.
- Native compilation is tested by CI; WASM, no-std, and a reduced transitive
  dependency footprint are not claimed by this package.

## Validate

From the repository root:

```sh
cargo test --manifest-path clients/rust/Cargo.toml --locked
cargo check --manifest-path clients/rust/Cargo.toml --all-targets --locked
cargo tree --manifest-path clients/rust/Cargo.toml --edges normal --locked
cargo test --workspace --all-features --locked
```

The dedicated Rust-client workflow runs the public API regression tests and
doctest independently from the server build and rejects an Axum dependency in
the normal client dependency graph. Existing contract-authority and bundle
checks remain in the main CI workflow.
