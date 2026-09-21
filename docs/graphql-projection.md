# Authored GraphQL projection contract

GraphQL is an explicit projection of the same semantic operations used by REST, RPC and Lambda. It is **not inferred from URL paths**, and `route.rs` never becomes GraphQL schema authority.

The protocol source trees are peers:

```text
src/routes/**      # REST authority: handlers.rs + optional route.rs; rpc.rs/lambda.rs are generated projections
src/rpc/**         # custom RPC authority: authored funcs.rs; sibling lambda.rs is generated
src/graphql/**     # GraphQL authority: authored funcs.rs; generated aggregate host/projection evidence lives outside authored funcs.rs
```

`src/routes/rpc/**` and `src/routes/graphql/**` are reserved collision sentinels. They may contain documentation placeholders, but never Rust route authority.

The canonical GraphQL HTTP endpoint is `POST /v1/graphql`. When subscriptions are enabled, the WebSocket upgrade uses the same `/v1/graphql` path.

GraphQL semantic authority is never duplicated. A resolver in `src/graphql/**/funcs.rs` binds to an existing operation owned either by REST `src/routes/**/handlers.rs` or custom RPC `src/rpc/**/funcs.rs`, using its stable operation key and the exact generated `__ores_invoke_*` policy boundary.

```rust
use ores_api_docs_graphql_macros::ores_graphql;

#[ores_graphql(
    operation_key = "users.get_user",
    invoke = crate::routes::users::get_user::handlers::__ores_invoke_get_user,
    kind = "query",
    field = "get_user",
    stream = "unary"
)]
pub async fn get_user_graphql(/* async-graphql context/input */) -> /* typed output */ {
    crate::routes::users::get_user::handlers::__ores_invoke_get_user(/* typed invocation */).await
}
```

The macro validates resolver shape and projection metadata. `ores-stack` owns the cross-file checks the proc macro cannot prove: the stable operation key must exist exactly once, the declared `invoke` path must be the generated invoker for that semantic operation, the resolver must call that exact invoker rather than the semantic function directly, stream modes must agree, GraphQL field names/kinds must be valid, and each operation may be projected to GraphQL only once in v1.

Query and mutation projections are unary. Subscription projections require `server_stream`. GraphQL relationships, pagination, nullability, DataLoaders, federation, input/output adaptation and resolver composition remain authored GraphQL concerns; they are not synthesized from REST paths or JSON Schema.

`ores-stack sync` writes canonical projection evidence to `generated/graphql/server-graphql-index.json`. The manifest authority is `funcs.rs`, endpoint is fixed at `/v1/graphql`, and entries are sorted deterministically for drift checking and API documentation.
