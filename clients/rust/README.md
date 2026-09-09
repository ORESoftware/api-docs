# Rust v1 client

`ores-api-docs-client` is the Rust package alongside the Dart, Gleam, Go and
TypeScript clients. Rust support already existed in `../../rust`; this package
makes that client surface discoverable without moving or copying the existing
implementation.

## Install

For a checkout or a whole-repository Zed installation:

```toml
[dependencies]
ores-api-docs-client = { path = "path/to/api-docs/clients/rust" }
```

Cargo Git dependencies can select the package by name. Pin the dependency to a
reviewed immutable commit containing this package rather than a moving branch.
This package is not published to crates.io.

Use the existing `.zpkg.toml` **repository** target and then the path above.
Do not install only `clients/rust`: the facade needs `rust/`, which embeds the
repository's authored schemas. The existing Zed `rust` target and the
`ores-api-docs` server package remain unchanged for compatibility.

## What is included

The public API re-exports the exact shared Rust types for route maps, discovery,
path expansion, binding metadata, strict v1 call/receipt envelopes, correlation,
NDJSON and bounded length-prefixed framing. No schema, validation rule or
generator is forked. The dependency uses `default-features = false`, so a
standalone client does not enable Axum. A host application that separately
activates the core's Axum feature still gets ordinary Cargo feature unification.

```rust
use ores_api_docs_client::{
    assert_rpc_v1_receipt_for_call, decode_rpc_v1_receipt,
    RpcV1Call, SchemaError, Transport,
};

fn prepare_call() -> Result<Vec<u8>, SchemaError> {
    let mut call = RpcV1Call::new("request-1", "get_item");
    call.transport = Some(Transport::Websocket);
    call.encode()
}

fn admit_reply(call: &RpcV1Call, bytes: &[u8]) -> Result<(), SchemaError> {
    let receipt = decode_rpc_v1_receipt(bytes)?;
    assert_rpc_v1_receipt_for_call(call, &receipt)
}
```

The client encodes and validates; it does not open sockets. Applications own
network I/O, TLS, authentication, timeouts, cancellation and retry policy. Do
not automatically retry mutations. Correlation validation does not replace
server-side authorization or route-specific request/response validation.

`OptionalJson::absent()` and an explicit JSON null remain distinct. Receipts
must match the request ID and operation key; explicitly supplied transports
must agree. TCP adapters retain incomplete tails and reject oversized prefixes.

RIDL v2 streaming frames remain in `runtime/rust` and are not interchangeable
with v1 envelopes. Generated route-specific surfaces still come from the
existing digest-bound bundle; this package introduces no new generator.

## Verify

```sh
cargo test --manifest-path clients/rust/Cargo.toml --locked
cargo clippy --manifest-path clients/rust/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path clients/rust/Cargo.toml -- --check
```

This client uses an independent workspace, like `runtime/rust`, so standalone
consumer checks cannot accidentally pass through a server-enabled dependency
graph. CI verifies the dependency graph, runs regression and documentation
tests, and refuses promotion until its generated lockfile and formatting are
reviewed and committed. TypeSpec and independently authored JSON Schema remain
peer authorities in their existing locations.
