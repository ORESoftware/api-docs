# HTTP-first `route.rs` and generated RPC

This is the server-side companion to `rust-filesystem-routing.md` for the initial `ores-stack` pilot in `canonical-cloud`, `sonus-auris`, and `fiducia-cloud`.

## Authoring rule: one file owns one path

`src/routes/**/route.rs` is path-centric, like a Next.js route module. It is **not** one file per RPC operation.

```text
src/routes/api/v1/quotes/route.rs         -> /api/v1/quotes
src/routes/api/v1/quotes/[quoteId]/route.rs -> /api/v1/quotes/{quoteId}
```

A single file may export any admitted HTTP verbs:

```rust
use axum::{extract::State, Json};
use canonical_interfaces::{CreateQuoteRequest, CreateQuoteResponse, ListQuotesResponse};

pub async fn get(
    State(state): State<AppState>,
) -> Json<ListQuotesResponse> {
    // ordinary HTTP implementation
    todo!()
}

pub async fn post(
    State(state): State<AppState>,
    Json(input): Json<CreateQuoteRequest>,
) -> Json<CreateQuoteResponse> {
    // ordinary HTTP implementation
    todo!()
}
```

Supported reserved public functions are currently `get`, `post`, `put`, `patch`, `delete`, `head`, and `options`. Helpers remain private. Every reserved verb must be `async`.

## Operation identity is still per verb

The filesystem determines the canonical path. The exported function determines the HTTP verb. `api-docs` then resolves the unique operation by `(canonical path, HTTP verb)`.

For example, `/api/v1/quotes` may resolve to:

```text
GET  /api/v1/quotes -> list_quotes
POST /api/v1/quotes -> create_quote
```

This preserves stable object-key RPC operation names even though both operations live in one authored `route.rs` file.

The generated checker fails closed when:

- a public verb exists in `route.rs` but the contract has no operation at that `(path, method)`,
- the contract has an HTTP verb at that path but the file does not export it,
- two operation keys attempt to own the same `(path, method)`,
- an operation used by filesystem RPC generation declares more than one HTTP method,
- the filesystem path and contract path disagree,
- a filesystem route attempts to own the reserved RPC transport path `/rpc/v1`.

## Type ownership

HTTP route implementation is handwritten. Wire types are not duplicated in `route.rs`.

TypeSpec and JSON Schema Draft 2020-12 remain peer authorities for path/query/header/body/result shapes. `typespec-json-schema-validator` is the convergence/admission gate. The generated Rust contract surface (normally distributed through the sibling `*-interfaces` package) exports the structs/enums used directly by Axum extractors and responses.

`api-docs` therefore generates routing/RPC glue **around** typed HTTP handlers; it does not ask product code to hand-maintain a second RPC request/response model.

## RPC is a projection of the same route functions

Generation creates the normal Axum method router directly from the reserved verb functions exported by each `route.rs`. It also creates one `RpcV1HttpRouteBinding` per resolved operation.

At runtime `/rpc/v1`:

1. validates the RPC envelope and operation key,
2. rejects any operation whose route-map path is the RPC transport path itself,
3. resolves the generated `(operation, method, path)` binding,
4. converts RPC path/query/application headers/body into an in-process HTTP request,
5. preserves trusted outer-ingress headers while refusing RPC application-header overrides of runtime-owned headers,
6. invokes the **REST-operation router**, which contains the same exported `route.rs` functions used by ordinary HTTP but does not contain `/rpc/v1`,
7. converts the HTTP response into the RPC receipt.

The generated composition builds the REST-operation router first, gives a clone of that REST-only router to the RPC registry, and only then merges the `/rpc/v1` transport router. RPC therefore cannot recursively dispatch into itself through the generated topology.

This keeps `Path<T>`, `Query<T>`, `Json<T>`, `State<T>`, generated contract types, and the authored route functions identical on the ordinary REST and RPC paths. RPC does not need a handwritten `handle(RpcV1Call)` twin.

## Middleware invariant

Middleware has two different jobs and must be placed deliberately.

**Operation middleware** is policy that must be identical whether an operation was reached through its normal REST route or through RPC: authorization, route-specific rate limiting, tracing spans, request policy, timeouts, idempotency admission, and similar business/API middleware. The generated `__ores_filesystem_api_http_and_rpc_router!` macro supports a third argument: a router transform applied to the REST-operation router **before** that router is cloned into the RPC registry. Use that form for shared operation middleware.

Conceptually:

```rust
let app = __ores_filesystem_api_http_and_rpc_router!(
    state,
    route_map,
    |router| router
        .layer(shared_authorization_layer)
        .layer(shared_rate_limit_layer)
        .layer(shared_observability_layer),
)?;
```

The resulting middleware stack runs once for a normal REST request and once for an RPC-dispatched operation. The RPC envelope endpoint itself remains a distinct transport boundary.

**Transport/ingress middleware** is policy for the outer HTTP listener or specifically for `/rpc/v1`: body-size limits, transport authentication, proxy trust, connection policy, envelope telemetry, and similar concerns. Middleware applied after the generated REST/RPC router has been composed belongs to this layer. It should not be relied on as the only implementation of operation authorization or route-specific rate limiting because RPC operation dispatch happens inside the REST-operation service.

This distinction prevents accidental policy drift while keeping the RPC transport visibly different from ordinary REST routing.

## Why not call a second RPC handler directly?

A future adapter may decode an RPC envelope and call the exported route function without constructing an in-process HTTP request. That is only safe if the same operation middleware is wrapped around that function in a transport-neutral layer. The invariant is more important than the adapter mechanism:

```text
REST adapter ─┐
              ├─> same exported operation function + same operation middleware
RPC adapter  ─┘
```

Do not introduce parallel REST and RPC business functions. If direct function dispatch is added, generated code should preserve one authored operation function and two thin transport adapters.

## Monolith and small function builds

The generated inventory is deterministic and contains both:

```text
(operation key, HTTP method, canonical path, source route.rs)
```

and the executable RPC-to-HTTP binding table.

A standalone API server includes every discovered `route.rs`. A small Lambda/function target may include only one selected route file (all verbs for that path) or a narrower operation slice when the build configuration requests it. Both modes preserve the same operation key, contract digest, auth/middleware rules, and HTTP handler implementation.

This is intentionally a build-shape choice, not a different application architecture.

## Relationship to `ores-stack`

`api-docs` owns filesystem route meaning, static source analysis, operation matching, generated Axum registration, RPC projection metadata/runtime support, the reserved RPC transport boundary, and the shared-operation-middleware composition contract.

`ores-stack` consumes those deterministic manifests and chooses build topology: monolith, route-file slice, or operation slice. It should not invent a second path grammar or RPC naming scheme, and RPC client generation must be sourced from API route contracts rather than browser page routes.
