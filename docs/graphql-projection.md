# Authored GraphQL projection contract

GraphQL is an explicit authored projection over the same stable semantic operations used by REST, RPC, and Lambda. It is **not inferred from REST URL paths**, and GraphQL schema design never becomes an accidental REST or RPC authority.

The September 21 layout uses three peer source trees:

```text
src/routes/**/
  handlers.rs   # authored semantic authority for REST-associated operations
  route.rs      # authored REST/HTTP projection
  rpc.rs        # generated REST-associated RPC publication
  lambda.rs     # generated provider-neutral Lambda projection/build adapter
  tmp/          # leaf-local generated/build state for dev

src/rpc/**/
  funcs.rs      # authored semantic authority for RPC-native operations
  lambda.rs     # generated provider-neutral Lambda/build projection
  tmp/          # leaf-local generated/build state for dev

src/graphql/**/
  resolvers.rs  # authored GraphQL schema/resolver projection
  lambda.rs     # generated GraphQL transport/build projection when selected
  tmp/          # leaf-local generated/build state for dev
```

`src/graphql/**/resolvers.rs` is the only authored GraphQL source filename. `graphql.rs` and GraphQL `funcs.rs` are not v1 authority files.

A GraphQL resolver binds by stable `operation_key` to an existing semantic operation. Semantic authority may come from either:

- `src/routes/**/handlers.rs` for REST-associated operations; or
- `src/rpc/**/funcs.rs` for RPC-native operations.

The resolver must call the exact generated `__ores_invoke_*` policy boundary for that semantic operation. It may not call the authored semantic function directly. This keeps authentication/admission, middleware, typed contracts, stream mode, and error semantics shared without collapsing REST, RPC, and GraphQL into one executable.

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
pub async fn get_user(/* authored GraphQL context/input */) -> /* typed output */ {
    crate::routes::users::get_user::handlers::__ores_invoke_get_user(/* typed invocation */).await
}
```

A resolver may instead bind to an RPC-native semantic operation:

```rust
#[ores_graphql(
    operation_key = "search.rebuild_index",
    invoke = crate::rpc::search::rebuild::funcs::__ores_invoke_rebuild_index,
    kind = "mutation",
    field = "rebuild_index",
    stream = "unary"
)]
pub async fn rebuild_index(/* authored GraphQL input */) -> /* typed output */ {
    crate::rpc::search::rebuild::funcs::__ores_invoke_rebuild_index(/* typed invocation */).await
}
```

The macro validates resolver shape and projection metadata. `ores-stack` owns the cross-file checks the proc macro cannot prove: the stable operation key must exist exactly once across the semantic inventory, the declared `invoke` path must be the generated invoker for that operation, the source must be under `src/graphql/**/resolvers.rs`, the resolver must call that exact invoker rather than bypassing it, stream modes must agree, GraphQL field names/kinds must be valid, and each operation may be projected to GraphQL only once in v1.

Query and mutation projections are unary. Subscription projections require `server_stream`. The GraphQL projection contract does not itself claim a runtime server-stream ABI that has not been admitted by the separate Lambda/PPR streaming work.

`ores-stack sync` writes canonical projection evidence to `generated/graphql/server-graphql-index.json`. The manifest authority is `resolvers.rs`, endpoint is fixed at `/v1/graphql`, and entries are sorted deterministically for drift checking and API documentation.

REST, RPC, and GraphQL remain distinct deployable binaries in production and in `ores-stack dev`. Sharing semantic authority does not permit a generic transport-switching executable. Lambda is an execution/build projection over admitted transport or semantic units, not a fourth authored semantic authority.
