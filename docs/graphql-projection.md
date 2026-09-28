# Authored GraphQL projection contract

GraphQL is an **optional, explicit authored projection** over stable semantic operations used by REST, RPC, Lambda, and MCP-facing tooling. It is **not inferred from REST URL paths**, and an operation has no GraphQL exposure unless an authored GraphQL resolver opts it in.

The semantic operation remains the shared business-logic and policy authority. GraphQL may adapt GraphQL-specific arguments, selection sets, relationships, nullability, pagination, federation, DataLoader behavior, and field composition, but it must invoke the same generated `__ores_invoke_*` boundary used by the other transports. That boundary is where shared auth/RBAC/tenancy/rate-limit/idempotency/tracing/audit policy runs.

## Transport and authority layout

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

`src/graphql/**/resolver.rs` is the only v1 authored GraphQL leaf filename. A resolver binds by stable `operation_key` to an existing semantic operation from either `src/routes/rest/**/handlers.rs` or `src/rpc/**/funcs.rs`, and it must call that operation's exact generated, crate-local `__ores_invoke_*` policy boundary. It may not call the semantic function directly or point at another crate's similarly named helper.

The standalone-server ingress routes are distinct from semantic authority:

- `src/routes/rpc/v1/route.rs` mounts RPC at `POST /v1/rpc`.
- `src/routes/graphql/v1/route.rs` mounts GraphQL HTTP at `POST /v1/graphql`; GraphQL subscription upgrade behavior uses the admitted GraphQL transport/runtime contract.
- `src/routes/ws/v1/route.rs` is the general standalone-server WebSocket upgrade ingress. It is governed by `.ores-ws.toml`, is not GraphQL resolver authority, and is not a Lambda leaf root.
- `src/routes/rest/**` contains ordinary REST route leaves.

The WebSocket ingress namespace does **not** change the GraphQL projection manifest. WebSocket runtime configuration and upgrade admission are separate from GraphQL resolver projection evidence. If WebSocket messages invoke admitted API operations, they must cross the same identity/middleware/operation policy boundary rather than bypassing it.

## One semantic contract, explicit transport projections

The operation's generated `OperationSpec` remains the semantic request/response/error and stream-shape contract. GraphQL does not create a second independent copy of those types. The authored resolver may adapt GraphQL-specific input/output structure where GraphQL semantics require it, but the cross-file admission join must still prove that the resolver points at the intended stable operation key and exact generated invoker.

This keeps the conceptual model:

```text
                         +-> HTTP/REST or Lambda HTTP projection
                         +-> typed RPC projection
semantic operation -----+-> optional authored GraphQL projection
+ OperationSpec         +-> MCP-facing description/tool projection
+ shared policy boundary+-> direct provider Lambda invocation where admitted
```

A transport adapter may add transport-only mechanics, but it may not change operation authorization, validation, error meaning, telemetry identity, or semantic stream cardinality.

## Compiler-visible GraphQL identity

`#[ores_graphql(...)]` validates the authored resolver and emits a hidden typed `GraphqlProjectionDescriptor`. The descriptor carries only GraphQL projection identity:

- fixed GraphQL endpoint (`/v1/graphql`);
- stable semantic `operation_key`;
- exact crate-local generated `__ores_invoke_*` boundary;
- GraphQL kind (`query`, `mutation`, or `subscription`);
- GraphQL field name;
- admitted semantic stream mode.

The descriptor is a generated Rust projection, **not a third authored contract authority** and not a serialized interchange format. Persisted GraphQL projection manifests continue to be governed by the independently authored TypeSpec and Draft 2020-12 JSON Schema authorities.

The descriptor intentionally does **not** duplicate request, response, error, codec, media type, auth policy, audience, or scope strings. Those facts are joined from the stable operation identity and its generated semantic contract/policy descriptor. This avoids allowing GraphQL metadata to drift into a second business-operation contract.

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
    return crate::routes::rest::users::get_user::handlers::__ores_invoke_get_user(
        /* typed invocation */
    )
    .await;
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
    return crate::rpc::search::rebuild_index::funcs::__ores_invoke_rebuild_index(
        /* typed invocation */
    )
    .await;
}
```

The macro validates local resolver shape and projection metadata. `ores-stack` owns the cross-file admission join: stable operation key uniqueness, exact generated invoker identity, source-tree identity, stream compatibility, duplicate GraphQL field detection, and rejection of direct semantic-function bypass.

## Streaming and incremental delivery

Query and mutation projections are unary. Subscription projections require `server_stream`.

Do not conflate these with HTTP response chunking or GraphQL incremental delivery. A transport using HTTP chunks does not turn a unary query/mutation into a semantic stream. If deferred/streamed GraphQL query delivery such as an explicitly admitted incremental transport is added later, model that as a separate GraphQL delivery/framing axis rather than reusing RPC stream cardinality.

For subscriptions, `ResponseBody` remains one semantic emitted item through the underlying `OperationSpec`; the GraphQL adapter maps that admitted server stream into the GraphQL subscription transport without buffering it into a unary response.

## Errors, policy, and telemetry

GraphQL must preserve the shared operation boundary:

1. GraphQL ingress parses/adapts the request.
2. Resolver metadata identifies the admitted semantic operation.
3. The resolver invokes the exact generated `__ores_invoke_*` function.
4. Shared validation/auth/RBAC/tenancy/rate-limit/idempotency/tracing/audit policy runs there.
5. The semantic operation executes once.
6. GraphQL maps the typed success/error/stream result into GraphQL response semantics.

GraphQL-specific presentation may add GraphQL error-path/extensions metadata, but it must not reinterpret an authorization rejection as success, suppress required audit/telemetry, or bypass the semantic error contract.

## Deterministic generation and build identity

`ores-stack sync` writes deterministic projection evidence to `generated/graphql/server-graphql-index.json`. Persisted manifest authority remains singular `resolver.rs` plus the peer TypeSpec/JSON Schema contract; generated files are evidence/projections and stay read-only.

REST, RPC, and GraphQL remain distinct Lambda/build families and remain separate in `ores-stack dev`. Each exact Lambda leaf keeps its own `tmp/`, build lock, hash receipt, and executable identity. WebSocket is a standalone-server ingress/runtime concern and is deliberately not a fourth Lambda leaf kind.

A GraphQL leaf build identity must include its GraphQL kind and stream shape so a REST or RPC executable cannot be accidentally reused as a GraphQL leaf even when all three expose the same semantic operation.

## Promotion gates

GraphQL support is optional, but when enabled it must fail closed unless all of the following hold:

1. the resolver uses a valid non-introspection GraphQL field name;
2. the operation key is stable and resolves to exactly one admitted semantic operation;
3. `invoke` is crate-local and resolves to that operation's generated `__ores_invoke_*` boundary;
4. query/mutation are unary and subscription is `server_stream`;
5. generated/persisted GraphQL evidence agrees with the authored resolver and peer authorities;
6. GraphQL and non-GraphQL transports share policy/error/telemetry semantics at the operation boundary;
7. generated client/schema output is deterministic from the admitted GraphQL schema/operations;
8. deployment/build identity keeps REST, RPC, and GraphQL leaves distinct.
