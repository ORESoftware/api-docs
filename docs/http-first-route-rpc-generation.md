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
- the filesystem path and contract path disagree.

## Type ownership

HTTP route implementation is handwritten. Wire types are not duplicated in `route.rs`.

TypeSpec and JSON Schema Draft 2020-12 remain peer authorities for path/query/header/body/result shapes. `typespec-json-schema-validator` is the convergence/admission gate. The generated Rust contract surface (normally distributed through the sibling `*-interfaces` package) exports the structs/enums used directly by Axum extractors and responses.

`api-docs` therefore generates routing/RPC glue **around** typed HTTP handlers; it does not ask product code to hand-maintain a second RPC request/response model.

## RPC is a projection of the HTTP handler

Generation creates the normal Axum method router from the reserved verb exports. It also creates one `RpcV1HttpRouteBinding` per resolved operation.

At runtime `/rpc/v1`:

1. validates the RPC envelope and operation key,
2. resolves the generated `(operation, method, path)` binding,
3. converts RPC path/query/application headers/body into an in-process HTTP request,
4. preserves trusted outer-ingress headers while refusing RPC application-header overrides of runtime-owned headers,
5. invokes the **same stateful Axum router** used by ordinary HTTP,
6. converts the HTTP response into the RPC receipt.

That means `Path<T>`, `Query<T>`, `Json<T>`, `State<T>`, normal middleware, authorization, and the generated contract types all stay on the HTTP path. RPC does not need a handwritten `handle(RpcV1Call)` twin.

## Monolith and small function builds

The generated inventory is deterministic and contains both:

```text
(operation key, HTTP method, canonical path, source route.rs)
```

and the executable RPC-to-HTTP binding table.

A standalone API server includes every discovered `route.rs`. A small Lambda/function target may include only one selected route file (all verbs for that path) or a narrower operation slice when the build configuration requests it. Both modes preserve the same operation key, contract digest, auth/middleware rules, and HTTP handler implementation.

This is intentionally a build-shape choice, not a different application architecture.

## Relationship to `ores-stack`

`api-docs` owns filesystem route meaning, static source analysis, operation matching, generated Axum registration, and RPC projection metadata/runtime support.

`ores-stack` consumes those deterministic manifests and chooses build topology: monolith, route-file slice, or operation slice. It should not invent a second path grammar or RPC naming scheme.
