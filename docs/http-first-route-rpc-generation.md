# HTTP-first `route.rs` and generated RPC

This is the server-side companion to `rust-filesystem-routing.md` for the initial `ores-stack` pilot in `canonical-cloud`, `sonus-auris`, `fiducia-cloud`, and `quaestor-ledger`.

## Authoring rule: one file owns one path

`src/routes/**/route.rs` is path-centric, like a Next.js route module. It is **not** one file per RPC operation.

```text
src/routes/api/v1/quotes/route.rs            -> /api/v1/quotes
src/routes/api/v1/quotes/[quoteId]/route.rs  -> /api/v1/quotes/{quoteId}
```

A single file may export admitted HTTP verbs. RPC-enabled verbs bind to a shared typed operation in the same file:

```rust
#[ores_operation(
    key = "canonical_cloud.quotes.create_quote",
    codecs("json", "protobuf", "messagepack"),
    default_codec = "protobuf",
    audiences("browser", "server")
)]
async fn create_quote(
    ctx: OperationContext<'_, AppState>,
    input: CreateQuoteInput,
) -> Result<CreateQuoteOutput, CreateQuoteError> {
    // Transport-independent application logic lives here exactly once.
    todo!()
}

#[ores_route(operation = create_quote)]
pub async fn post(
    State(state): State<AppState>,
    headers: CreateQuoteHeaders,
    Json(body): Json<CreateQuoteRequest>,
) -> Result<CreateQuoteHttpResponse, CreateQuoteError> {
    __ores_invoke_create_quote(
        OperationContext::http(&state),
        CreateQuoteInput { headers, body },
    )
    .await
    .map(Into::into)
}
```

Supported reserved public functions remain `get`, `post`, `put`, `patch`, `delete`, `head`, and `options`. Helpers and shared operations are not public HTTP exports.

## One implementation, two transport adapters

The preferred RPC execution model is `shared_operation`.

For every RPC-enabled HTTP verb, `api-docs` statically proves this graph:

```text
ordinary HTTP request                 POST /v1/rpc
        |                                  |
        v                                  v
Axum HTTP transport adapter        generated RPC adapter
        |                                  |
        +---------------+------------------+
                        |
                        v
             generated __ores_invoke_x
                        |
        auth / RBAC / tenancy / validation
        rate limit / idempotency / tracing
                        |
                        v
                 authored operation x
```

The RPC adapter does **not** manufacture a second HTTP request and does not call the Axum HTTP handler. Both adapters call the same generated operation invoker, and that invoker calls the same authored typed operation.

This removes two drift risks at once:

- RPC cannot grow a second copy of business logic.
- RPC does not need to pretend that `POST /v1/rpc` is the operation's REST URI/method in order to reuse code.

`http_projection_legacy` remains a migration-only compatibility model for older `#[ores_rpc]` routes. New routes should use `#[ores_operation]` plus `#[ores_route(operation = ...)]`.

## Middleware ownership

Middleware is split by responsibility instead of by transport accident.

HTTP-transport-only concerns remain on the Axum HTTP path, for example CORS, HTTP compression, Host handling, redirects, and raw HTTP body limits.

RPC-transport-only concerns remain on `/v1/rpc`, for example envelope version admission, operation lookup, codec/framing checks, and RPC batch limits.

Operation-level concerns belong in the generated `__ores_invoke_x` boundary and therefore execute for both transports: authenticated identity, authorization/RBAC, tenancy, business rate limits, idempotency, audit logging, operation tracing, and contract validation.

A route may not opt an RPC adapter out of operation policy independently from its HTTP adapter.

## Operation identity is still per verb

The filesystem determines the canonical HTTP path. The exported function determines the HTTP verb. `api-docs` resolves the unique operation by `(canonical path, HTTP verb)`, then verifies that the HTTP adapter's `#[ores_route(operation = ...)]` target carries the matching `#[ores_operation(key = ...)]`.

### Optional: declaring the mounted path on the adapter

```rust
#[ores_route(operation = handlers::find_user, path = "/v1/users/{id}")]
pub async fn get(req: Request) -> Response { /* ... */ }
```

`path` is optional HTTP-projection metadata: the template this adapter is
mounted at, in the Axum 0.8 syntax services already author (`{name}` for one
segment, `{*name}` for the rest, last segment only). The verb is still the
function name and is not repeated. The macro validates the template at compile
time, so a typo is a compile error in the adapter rather than a route that never
matches.

It exists for generators that must know the projection without being able to
find its registration. A service that centralizes routing -- one `match` or one
`Router` for the whole API, with route folders that are organizational rather
than URL-shaped -- has no per-folder `Router::route(...)` for a generator to
read, and should not have to restructure into one just to be discoverable.

It is a *declaration*, not a second routing mechanism, and it does not make the
adapter mount itself. Where another authored source states the path for the same
adapter (a `Router::route(...)` registration in the same file, or a URL-shaped
filesystem location), they must agree: a generator that sees two authored
answers fails closed instead of picking one.

For example, `/api/v1/quotes` may resolve to:

```text
GET  /api/v1/quotes -> list_quotes
POST /api/v1/quotes -> create_quote
```

Each shared operation has its own stable RPC key even though both operations live in one authored `route.rs` file.

The generated checker fails closed when:

- a public verb exists in `route.rs` but the contract has no operation at that `(path, method)`,
- the contract has an HTTP verb at that path but the file does not export it,
- two operation keys attempt to own the same `(path, method)`,
- a shared operation is not bound to exactly one HTTP adapter,
- an HTTP adapter references a missing shared operation,
- the route-map `rpc_key` and `#[ores_operation]` key disagree,
- admin operation metadata attempts browser exposure,
- the filesystem path and contract path disagree.

## Type ownership

HTTP transport adaptation and operation implementation are handwritten. Wire types are not duplicated in attributes.

TypeSpec and JSON Schema Draft 2020-12 remain peer authorities for path/query/header/body/result shapes. `typespec-json-schema-validator` is the convergence/admission gate. The generated Rust contract surface (normally distributed through the sibling `*-interfaces` package) exports the structs/enums used by the operation input/output and Axum adapter.

`#[ores_operation]` carries only semantic metadata that cannot be inferred safely: stable operation key, allowed/default codecs, client audiences, and regular/admin scope. Request/response types come from the Rust function signatures and the admitted peer contracts.

The normalized operation IR records request path/query/header/body schemas, response body/header/trailer/error schemas, source `route.rs`, HTTP adapter name, shared operation function, generated invoker name, codec policy, audience, scope, contract digest, and pinned API-server commit.

## Headers, trailers, and payload codecs

The shared operation input/output model is transport-independent. Generated HTTP and RPC adapters translate their transport representation into the same typed values.

Application request headers and response headers/trailers are contract fields. Runtime-owned transport headers cannot be set through the generated application header API.

JSON, Protobuf, and MessagePack are codecs for the same semantic request/response types. Selecting a codec selects the serializer and transport content metadata together; callers do not manually set a contradictory `content-type`.

HTTP trailers and RPC receipt trailers are projected from the same typed response-trailer contract.

## Generated files stay local to the route

While RPC generation is still evolving, generated server glue may be tracked beside the authoritative route:

```text
src/routes/api/v1/quotes/
  route.rs     # authored path + typed operations + HTTP adapters
  rpc.rs       # generated /v1/rpc adapters/bindings
  gen.rs       # other generated route/build glue when needed
```

This keeps diffs isolated: changing one route does not rewrite a single giant server dispatch file. Once generation is sufficiently stable, `rpc.rs`/`gen.rs` may become build-only outputs without changing the authored contract.

Generated RPC client code in `*-lib-core` / `*-pub-lib-core` is separately pinned to the exact source API-server commit and contract digest.

## Monolith and small function builds

The deterministic inventory contains:

```text
(operation key,
 HTTP method,
 canonical path,
 source route.rs,
 HTTP adapter,
 shared operation,
 generated invoker,
 execution model)
```

A standalone API server may include every discovered `route.rs`. A small Lambda/function target may include one selected route file or narrower operation slice. Both preserve the same operation key, contract digest, operation policy, and authored inner implementation.

This is a build-shape choice, not a different application architecture.

## Relationship to `ores-stack`

`api-docs` owns filesystem route meaning, static source analysis, operation matching, shared-operation binding, generated server adapters, typed operation IR, and language SDK projection.

`ores-stack` consumes those deterministic manifests and chooses build topology: monolith, route-file slice, or operation slice. It also records source repository/commit provenance when synchronizing clients into `*-lib-core` / `*-pub-lib-core`.

`ores-stack` must not invent a second path grammar, RPC naming scheme, or business-operation implementation.
