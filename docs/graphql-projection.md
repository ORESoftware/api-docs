# Authored GraphQL projection contract

GraphQL is an explicit authored projection of the same semantic operations used by REST, RPC and Lambda. It is **not inferred from URL paths**, and `route.rs` never becomes GraphQL schema authority.

For a REST-associated API operation, the canonical folder is:

```text
src/routes/**/
  handlers.rs   # authored semantic authority
  route.rs      # authored REST/HTTP projection
  graphql.rs    # authored GraphQL projection
  rpc.rs        # generated guarded RPC projection
  lambda.rs     # generated provider-neutral Lambda projection
```

`handlers.rs` is the only semantic authority in this v1 route-local model. `graphql.rs` may design GraphQL field names, input/output adaptation, relationships, pagination, nullability, DataLoaders, federation and resolver composition, but it must reach application behavior through the exact generated sibling `handlers::__ores_invoke_*` boundary. It may not call the semantic handler directly.

Custom RPC-only operations under `src/rpc/**/funcs.rs` remain a separate RPC authority and are not GraphQL semantic authority in v1. If a capability needs REST/GraphQL/RPC/Lambda parity, place its semantic operation in `src/routes/**/handlers.rs` and project it through the siblings above.

The canonical GraphQL HTTP endpoint is `POST /v1/graphql`. When subscriptions are enabled, the WebSocket upgrade uses the same `/v1/graphql` path.

```rust
use ores_api_docs_graphql_macros::ores_graphql;

#[ores_graphql(
    operation_key = "users.get_user",
    invoke = crate::routes::users::get_user::handlers::__ores_invoke_get_user,
    kind = "query",
    field = "get_user",
    stream = "unary"
)]
pub async fn get_user_graphql(/* authored GraphQL context/input */) -> /* typed output */ {
    crate::routes::users::get_user::handlers::__ores_invoke_get_user(/* typed invocation */).await
}
```

The macro validates resolver shape and projection metadata. `ores-stack` owns the cross-file checks the proc macro cannot prove: the stable operation key must exist exactly once in the handlers-authoritative inventory, the declared `invoke` path must be the generated invoker for that operation, `graphql.rs` must be the sibling projection of that route operation, the resolver must call that exact invoker rather than the semantic function directly, stream modes must agree, GraphQL field names/kinds must be valid, and each operation may be projected to GraphQL only once in v1.

Query and mutation projections are unary. Subscription projections require `server_stream`. This projection contract does not by itself claim the separately gated server-stream Lambda/PPR runtime ABI.

`ores-stack sync` writes canonical projection evidence to `generated/graphql/server-graphql-index.json`. The manifest authority is `graphql.rs`, endpoint is fixed at `/v1/graphql`, and entries are sorted deterministically for drift checking and API documentation. API servers with semantic operations but intentionally no GraphQL exposure still commit an empty generated manifest so absence is distinguishable from stale or missing generation.
