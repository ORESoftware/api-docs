# RPC publication authority

`handlers.rs` remains the semantic authority for API operations. RPC publication is a separate transport decision.

## REST-associated operations

When a `handlers.rs` file has a sibling authored `route.rs`, every `#[ores_operation]` is published through the generated `/v1/rpc` dispatcher by default. Use `#[ores_no_rpc]` to suppress only public RPC publication; the semantic operation remains available to REST, GraphQL, and admitted Lambda/direct hosts.

```rust
#[ores_no_rpc]
#[ores_operation(
    spec = InternalPreviewOperation,
    key = "chapter.preview.internal",
)]
pub async fn internal_preview(...) -> ... { ... }
```

## Independent RPC operations

A route-less semantic operation is not inferred to be public RPC. It must explicitly opt in with `#[ores_rpc]`.

```rust
#[ores_rpc]
#[ores_operation(
    spec = RebuildIndexOperation,
    key = "search.rebuild_index",
)]
pub async fn rebuild_index(...) -> ... { ... }
```

`ores-stack` owns the cross-file rule because it can see whether sibling `route.rs` exists. The proc-macro crate only makes `#[ores_rpc]` and `#[ores_no_rpc]` compile-time Rust attributes and rejects malformed/conflicting marker usage.

## Deployment

The public RPC protocol remains a single aggregate endpoint, canonically `POST /v1/rpc`. The same generated RPC dispatch inventory may be hosted by the standalone API server, colocated in a REST Lambda build unit, or exposed by a dedicated aggregate RPC Lambda build unit. Deployment does not change the operation key or RPC envelope.
