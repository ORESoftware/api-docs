# RPC publication authority

ORE API servers separate standalone-server ingress routes from RPC semantic leaves.

```text
src/
├── routes/
│   ├── rest/**                 # authored REST route/Lambda leaves
│   ├── rpc/v1/route.rs         # HTTP ingress/mount for POST /v1/rpc
│   ├── graphql/v1/route.rs     # HTTP ingress/mount for POST /v1/graphql
│   └── ws/v1/route.rs          # standalone-server WebSocket upgrade ingress
├── rpc/**/funcs.rs             # authored RPC-native semantic authority
└── graphql/**/resolver.rs      # authored GraphQL projection leaves
```

## REST-associated RPC

REST-associated semantic authority lives in `src/routes/rest/**/handlers.rs`. The sibling `route.rs` is the authored REST/HTTP projection. Generated REST-associated `rpc.rs` publication exposes admitted operations through the aggregate RPC transport, and generated `lambda.rs` provides the execution/build projection where required.

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

RPC-native operations live under `src/rpc/**/funcs.rs`. `funcs.rs` is authored semantic authority. A custom RPC function must be `pub async`, accept the admitted typed operation context, return the admitted typed result shape, and carry the RPC/operation metadata with a stable operation key.

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

`ores-stack custom rpc sync` discovers `src/rpc/**/funcs.rs`, rejects duplicate keys and malformed authority, writes deterministic `generated/rpc/custom-operation-index.json`, and generates sibling `lambda.rs` execution/build projections.

## Standalone-server ingress

RPC HTTP ingress is explicitly mounted at:

```text
src/routes/rpc/v1/route.rs
```

That route owns the HTTP adapter for `POST /v1/rpc`; it is **not** RPC semantic authority. The semantic inventory remains the union of:

- REST-associated operations from `src/routes/rest/**/handlers.rs` that publish to RPC; and
- RPC-native operations from `src/rpc/**/funcs.rs`.

The sibling route namespaces are likewise ingress-only:

```text
src/routes/graphql/v1/route.rs  # GraphQL HTTP ingress
src/routes/ws/v1/route.rs       # standalone-server WebSocket upgrade ingress
```

GraphQL authored resolver leaves remain under singular `src/graphql/**/resolver.rs`.

WebSocket ingress is governed by `.ores-ws.toml` and is not a REST/RPC/GraphQL semantic hierarchy or a Lambda leaf root. If WebSocket messages invoke admitted API operations, dispatch must cross the same trusted identity/middleware/operation policy boundary rather than inventing a parallel authorization or operation registry.

## Source-tree boundaries

The standalone-server route tree is explicitly namespaced:

```text
src/routes/rest/**
src/routes/rpc/v1/route.rs
src/routes/graphql/v1/route.rs
src/routes/ws/v1/route.rs
```

Do not place REST leaves directly under `src/routes/**` outside `rest/`. Do not place RPC semantic `funcs.rs` under `src/routes/rpc/**`, GraphQL semantic `resolver.rs` under `src/routes/graphql/**`, or Lambda/semantic leaves under `src/routes/ws/**`; those namespaces are ingress only.

The Lambda/build leaf roots remain exactly:

```text
src/routes/rest
src/rpc
src/graphql
```

## Aggregate publication

REST-derived RPC and custom RPC operations are merged into the same `/v1/rpc` publication inventory. Operation keys must remain globally unique across both semantic authorities. Deployment may host the same admitted inventory in the standalone RPC binary or generated Lambda build units without changing the wire operation key or RPC envelope.

REST, RPC, and GraphQL remain separate Lambda/build identities. WebSocket remains a standalone-server ingress/runtime identity. Shared semantic operations, stable operation keys, and generated adapters do not justify a generic runtime transport switch.
