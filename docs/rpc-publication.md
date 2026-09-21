# RPC publication authority

ORE API servers separate HTTP ingress routes from RPC semantic leaves.

```text
src/
├── routes/
│   ├── rest/**                 # authored REST route leaves
│   ├── rpc/v1/route.rs         # HTTP ingress/mount for POST /v1/rpc
│   └── graphql/v1/route.rs     # HTTP ingress/mount for POST /v1/graphql
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

## HTTP ingress

RPC HTTP ingress is explicitly mounted at:

```text
src/routes/rpc/v1/route.rs
```

That route owns the HTTP adapter for `POST /v1/rpc`; it is **not** RPC semantic authority. The semantic inventory remains the union of:

- REST-associated operations from `src/routes/rest/**/handlers.rs` that publish to RPC; and
- RPC-native operations from `src/rpc/**/funcs.rs`.

Likewise, `src/routes/graphql/v1/route.rs` is GraphQL HTTP ingress and is distinct from GraphQL authored resolver leaves under `src/graphql/**/resolver.rs`.

## Source-tree boundaries

The route tree is now explicitly namespaced by transport:

```text
src/routes/rest/**
src/routes/rpc/v1/route.rs
src/routes/graphql/v1/route.rs
```

Do not place REST leaves directly under `src/routes/**` outside `rest/`. Do not place RPC semantic `funcs.rs` under `src/routes/rpc/**`, and do not place GraphQL semantic `resolver.rs` under `src/routes/graphql/**`; those route namespaces are HTTP ingress only.

## Aggregate publication

REST-derived RPC and custom RPC operations are merged into the same `/v1/rpc` publication inventory. Operation keys must remain globally unique across both semantic authorities. Deployment may host the same admitted inventory in the standalone RPC binary or generated Lambda build units without changing the wire operation key or RPC envelope.

REST, RPC, and GraphQL remain separate production binaries and remain separate binaries in development. Shared semantic operations, stable operation keys, and generated Lambda adapters do not justify a generic runtime transport switch.
