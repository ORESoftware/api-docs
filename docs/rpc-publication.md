# RPC publication authority

ORE API servers have two semantic RPC sources that share one public aggregate endpoint, `POST /v1/rpc`.

## REST-associated RPC

REST-associated semantic authority lives in `src/routes/**/handlers.rs`. `route.rs` is the authored Axum/HTTP projection. The generated sibling `rpc.rs` publishes admitted handler operations through `/v1/rpc`, and generated `lambda.rs` hosts the same typed operation boundary for provider-neutral Lambda execution/builds.

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

Operations that are RPC-native rather than REST projections live under `src/rpc/**/funcs.rs`. `funcs.rs` is authored semantic authority. A custom RPC function must be `pub async`, accept one `TypedOperationContext<State, Spec>`, return the admitted typed result shape, and carry both `#[ores_rpc]` and `#[ores_operation]` with a stable operation key.

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

`ores-stack custom rpc sync` discovers `src/rpc/**/funcs.rs`, rejects duplicate keys and malformed authority, writes deterministic `generated/rpc/custom-operation-index.json`, and generates sibling `lambda.rs`. Server-stream publication must use the separately admitted streaming ABI rather than being silently treated as unary.

## Source-tree boundaries

The protocol roots are peers:

```text
src/routes/**      # handlers.rs + route.rs + generated rpc.rs/lambda.rs
src/rpc/**         # authored custom RPC funcs.rs + generated lambda.rs
src/graphql/**     # authored GraphQL resolvers.rs + generated transport/build projection
```

`src/routes/rpc/**` and `src/routes/graphql/**` are reserved sentinels and may not contain route authority. `src/rpc/**` must not contain REST `handlers.rs`/`route.rs` or GraphQL `resolvers.rs`; `src/graphql/**` must not contain REST/RPC authority files. GraphQL may project an existing stable operation from either `handlers.rs` or `funcs.rs`, but `resolvers.rs` does not become a second semantic authority.

## Aggregate publication

REST-derived RPC and custom RPC operations are merged into the same `/v1/rpc` publication inventory. Operation keys must remain globally unique across both semantic authorities. Deployment may host that inventory in the standalone RPC binary, provider-neutral Lambda build units, or a dedicated aggregate RPC Lambda without changing the wire operation key or RPC envelope.

REST, RPC, and GraphQL are separate production binaries and remain separate binaries in development. Shared semantic operations and generated Lambda adapters do not justify a generic runtime transport switch.
