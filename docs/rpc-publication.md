# RPC publication authority

ORE API servers have two semantic RPC sources that share one public aggregate endpoint, `POST /v1/rpc`.

## REST-associated RPC

REST semantic authority lives in `src/routes/**/handlers.rs`. `route.rs` is only the optional Axum/HTTP projection. The generated sibling `rpc.rs` publishes admitted handler operations through `/v1/rpc`, and generated `lambda.rs` hosts the same typed operation boundary for provider-neutral Lambda execution.

A REST-owned operation may suppress public RPC publication with `#[ores_no_rpc]` without changing its semantic identity:

```rust
#[ores_no_rpc]
#[ores_operation(
    spec = InternalPreviewOperation,
    key = "chapter.preview.internal",
)]
pub async fn internal_preview(...) -> ... { ... }
```

## Authored custom RPC

Operations that are RPC-native rather than REST projections live under `src/rpc/**/funcs.rs`. That file is authored authority. A custom RPC function must be `pub async`, accept one `TypedOperationContext<State, Spec>`, return `Result<Success, Error>`, and carry both `#[ores_rpc]` and `#[ores_operation]` with a stable operation key.

```rust
#[ores_rpc]
#[ores_operation(
    spec = RebuildIndexOperation,
    key = "search.rebuild_index",
    codecs("json"),
)]
pub async fn rebuild_index(
    ctx: TypedOperationContext<AppState, RebuildIndexOperation>,
) -> Result<RebuildIndexResponse, RebuildIndexError> {
    // semantic implementation
}
```

`ores-stack custom rpc sync` discovers `src/rpc/**/funcs.rs`, rejects duplicate keys and malformed authority, writes deterministic `generated/rpc/custom-operation-index.json`, and generates sibling `lambda.rs`. Generated custom-RPC Lambda support is currently unary-only; a custom RPC `server_stream` declaration is rejected until the server-stream Lambda ABI is implemented.

## Source-tree boundaries

The protocol roots are peers:

```text
src/routes/**      # REST + generated REST-RPC
src/rpc/**         # authored custom RPC funcs.rs + generated lambda.rs
src/graphql/**     # authored GraphQL funcs.rs
```

`src/routes/rpc/**` and `src/routes/graphql/**` are reserved sentinels and may not contain Rust route authority. `src/rpc/**` must not contain REST `handlers.rs`/`route.rs` or GraphQL authority; `src/graphql/**` must not contain REST/RPC authority files.

## Aggregate publication

REST-derived RPC and custom RPC operations are merged into the same `/v1/rpc` publication inventory. Operation keys must remain globally unique across both semantic authorities. Deployment may host that inventory in the standalone API server, provider-neutral Lambda build units, or a dedicated aggregate RPC Lambda without changing the wire operation key or RPC envelope.
