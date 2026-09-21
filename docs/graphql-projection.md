# Authored GraphQL projection contract

GraphQL is an explicit projection of the same semantic operations used by HTTP, RPC and Lambda. It is **not inferred from URL paths** and does not make `route.rs` authoritative for a graph shape.

The only GraphQL HTTP endpoint is `POST /v1/graphql` (plus the WebSocket upgrade at that same path when subscriptions are enabled). A route folder may contain:

```text
src/routes/users/get_user/
├── handlers.rs     # semantic operation authority
├── route.rs        # optional REST/Axum projection
├── rpc.rs          # generated RPC projection
├── lambda.rs       # generated Lambda projection
└── graphql.rs      # authored GraphQL projection
```

A resolver is handwritten and annotated with the compile-time projection identity:

```rust
use ores_api_docs_graphql_macros::ores_graphql;

#[ores_graphql(
    operation = handlers::get_user,
    kind = "query",
    field = "get_user",
    stream = "unary"
)]
pub async fn get_user_graphql(/* async-graphql context/input */) -> /* typed output */ {
    // Adapter code only. Call the same generated shared-operation boundary used
    // by REST/RPC/Lambda rather than duplicating business semantics here.
}
```

The macro rejects missing metadata, inferred/overridable endpoints, invalid GraphQL names, non-async resolvers, self receivers, query/mutation streams other than `unary`, and subscriptions other than `server_stream`.

`ores-stack` owns the cross-file checks the proc macro cannot perform: the referenced handler must exist in sibling `handlers.rs`, its stable operation key and stream mode must agree, every `(kind, field)` must be unique service-wide, and the generated projection manifest must be canonical. The manifest authority is `graphql.rs`, endpoint is fixed to `/v1/graphql`, and entries are sorted by `(kind, field, operation_key)` before serialization.

The projection manifest is a docs/conformance input, not a generated GraphQL API design. Object relationships, nullability choices, pagination, DataLoaders, federation and resolver composition remain authored GraphQL concerns.
