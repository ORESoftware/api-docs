# Authored GraphQL projection contract

GraphQL is an explicit authored projection over stable semantic operations used by REST, RPC, and Lambda. It is **not inferred from REST URL paths**.

The standalone API server separates transport ingress routes from RPC/GraphQL semantic leaves:

```text
src/
├── routes/
│   ├── rest/
│   │   └── ...                 # REST Lambda leaves
│   ├── rpc/
│   │   └── v1/
│   │       └── route.rs        # HTTP ingress/mount for POST /v1/rpc
│   ├── graphql/
│   │   └── v1/
│   │       └── route.rs        # HTTP ingress/mount for POST /v1/graphql
│   └── ws/
│       └── v1/
│           └── route.rs        # standalone-server WebSocket upgrade ingress
├── rpc/
│   └── <operation>/
│       ├── funcs.rs            # authored RPC-native semantic authority
│       ├── lambda.rs           # generated execution/build projection
│       └── tmp/                # leaf-local build state
└── graphql/
    └── <operation>/
        ├── resolver.rs         # authored GraphQL resolver/schema projection
        ├── lambda.rs           # generated execution/build projection
        └── tmp/                # leaf-local build state
```

REST-associated semantic authority lives under `src/routes/rest/**/handlers.rs`; its HTTP projection is the sibling `route.rs`. RPC-native semantic authority lives under `src/rpc/**/funcs.rs`.

`src/graphql/**/resolver.rs` is the only v1 authored GraphQL leaf filename. A resolver binds by stable `operation_key` to an existing semantic operation from either `src/routes/rest/**/handlers.rs` or `src/rpc/**/funcs.rs`, and it must call that operation's exact generated `__ores_invoke_*` policy boundary. It may not call the semantic function directly.

The standalone-server ingress routes are distinct from semantic authority:

- `src/routes/rpc/v1/route.rs` mounts RPC at `POST /v1/rpc`.
- `src/routes/graphql/v1/route.rs` mounts GraphQL HTTP at `POST /v1/graphql`; GraphQL subscription upgrade behavior uses the admitted GraphQL transport/runtime contract.
- `src/routes/ws/v1/route.rs` is the general standalone-server WebSocket upgrade ingress. It is governed by `.ores-ws.toml`, is not GraphQL resolver authority, and is not a Lambda leaf root.
- `src/routes/rest/**` contains ordinary REST route leaves.

The WebSocket ingress namespace does **not** change the GraphQL projection manifest. WebSocket runtime configuration and upgrade admission are separate from GraphQL resolver projection evidence. If WebSocket messages invoke admitted API operations, they must cross the same identity/middleware/operation policy boundary rather than bypassing it.

GraphQL relationships, pagination, federation, nullability, DataLoaders, input/output adaptation, and resolver composition remain explicitly authored rather than inferred from REST paths.

```rust
use ores_api_docs_graphql_macros::ores_graphql;

#[ores_graphql(
    operation_key = "users.get_user",
    invoke = crate::routes::rest::users::get_user::handlers::__ores_invoke_get_user,
    kind = "query",
    field = "get_user",
    stream = "unary"
)]
pub async fn resolve(/* authored GraphQL context/input */) -> /* typed output */ {
    crate::routes::rest::users::get_user::handlers::__ores_invoke_get_user(/* typed invocation */).await
}
```

A resolver may instead bind to an RPC-native semantic operation:

```rust
#[ores_graphql(
    operation_key = "search.rebuild_index",
    invoke = crate::rpc::search::rebuild_index::funcs::__ores_invoke_rebuild_index,
    kind = "mutation",
    field = "rebuild_index",
    stream = "unary"
)]
pub async fn resolve(/* authored GraphQL input */) -> /* typed output */ {
    crate::rpc::search::rebuild_index::funcs::__ores_invoke_rebuild_index(/* typed invocation */).await
}
```

The macro validates resolver shape and projection metadata. `ores-stack` owns cross-file admission: stable operation key uniqueness, exact generated invoker identity, source-tree identity, stream compatibility, duplicate GraphQL field detection, and rejection of direct semantic-function bypass.

Query and mutation projections are unary. Subscription projections require `server_stream`; the projection contract does not pretend an unadmitted runtime streaming ABI exists.

`ores-stack sync` writes deterministic projection evidence to `generated/graphql/server-graphql-index.json`. Manifest authority is singular `resolver.rs`.

REST, RPC, and GraphQL remain distinct Lambda/build families and remain separate in `ores-stack dev`. Each exact Lambda leaf keeps its own `tmp/`, build lock, hash receipt, and executable identity. WebSocket is a standalone-server ingress/runtime concern and is deliberately not a fourth Lambda leaf kind.
