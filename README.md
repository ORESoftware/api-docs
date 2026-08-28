# api-docs

Language-neutral **route map**: a JSON object whose **keys are operations** and
whose **values are HTTP routes**. That map is the interchange format for RPC
across Rust, TypeScript, Dart, and Gleam.

```json
{
  "schema_version": "1.0.0",
  "service": "pmap-api-server",
  "map": {
    "healthz": "/healthz",
    "CheckFieldSanity": "/pmap.v1.Interview/CheckFieldSanity"
  }
}
```

PascalCase keys are Connect-shaped JSON unary (`POST`, `application/json`, not
gRPC). Other keys infer `GET`/`POST` from the name unless `methods` is set.

## Language surfaces (not the interchange)

The JSON map is what every language agrees on. **How you author a key in
source is per-language**, and any combination is valid:

| Surface | Languages that typically use it |
| --- | --- |
| **Annotation / attribute / decorator** on a method | Rust `#[get]`, Axum `post(...)`, Dart `@Rpc`, Java `@GET`, C# `[HttpPost]` |
| **Param type(s)** | Axum extractors, Connect request message, JSON-RPC params |
| **Return type** | Axum `IntoResponse`, Connect response message, OpenRPC result |
| **Function type** | TypeScript `UnaryFn<Req, Res>`, Gleam `fn(Req) -> Res`, Dart `typedef Unary` |
| **Combination** | The usual case: attribute *and* typed req/res *and* a named fn type |

1:1 file↔route correspondence (Leptos/Dioxus / Next-style) is **optional**.
One file may hold several HTTP verbs.

JSON Schema: [`json-schema/route-binding.schema.json`](json-schema/route-binding.schema.json)
and [`json-schema/language-surface.schema.json`](json-schema/language-surface.schema.json).

Rust typed surface:

```rust
pub trait RpcMethod {
    const KEY: &'static str;
    const PATH: &'static str;
    type Params;   // param type
    type Output;   // return type
}
pub type UnaryFn<M> = fn(<M as RpcMethod>::Params) -> <M as RpcMethod>::Output;
```

## Several standards, closely — not one perfectly

Every catalog is checked with **JSON Schema 2020-12** (liberally: the map,
each projection, bindings, and docs headers each have a schema). The same map
is also projected into:

1. **OpenAPI 3.1** — `paths` + `operationId` = map key (`/openapi.json`)
2. **Connect JSON unary** — `POST /{service}/{Method}` (`/connect.json`)
3. **OpenRPC 1.3** — method name = map key; params/result are the types (`/openrpc.json`)
4. **JSON Hyper-Schema links** — `rel` = key, `href` = route, submission/target schemas = param/return
5. **k8s-cluster catalog** — `GET /docs/api`, `/api/docs`, `/api/docs.json`

URI templates in paths (`/v1/matters/{id}`) follow **RFC 6570** level 1, which
is also how OpenAPI writes path params.

## Typed query, path, and JSON payloads

Each map value may declare JSON Schema 2020-12 for the compile surface:

| Field | Meaning |
| --- | --- |
| `path_params` | Object schema whose properties **are** the `{placeholders}` in `path` |
| `query_schema` | Object schema for the query string |
| `request_schema` | JSON body / RPC payload |
| `response_schema` | Success JSON |
| `error_schema` | Documented error JSON |
| `alias_of` | Another key this route aliases (REST alias of a Connect method) |

`scripts/generate-routes.py` turns that shared source into objects whose **keys
are the operations** and whose values hold the HTTP path:

- TypeScript `Routes.get_matter.path` / `RouteHandlers<Ctx>` (missing handler = type error)
- Rust `RouteKey` enum (missing `match` arm = compile error)
- Dart `Routes.byKey['get_matter']`
- Gleam `RouteKey` custom type (missing `case` arm = compile error)

```sh
python3 scripts/generate-routes.py
python3 scripts/generate-routes.py --check
```

Frontend calls `Routes["get_matter"]` (or `lookup("get_matter")`) instead of
hard-coding `/v1/matters/{id}`. Backend handles every generated key.

## opto-sync (RPC uses sync; sync does not use RPC)

opto-sync must **not** depend on this crate. This crate publishes a route map
as an opto-sync document so clients stay on the same keys:

- scope / kind: `ores.api-docs.route-map`
- identity key: `id` (the `service` name)
- LWW keys: `updatedAt,syncedAt` (opto-sync-client defaults)

```rust
let env = ores_api_docs::RouteMapEnvelope::wrap(&map, "1689940800123456789")?;
// pass env through opto-sync-client::reconcile on each replica
let map = env.into_map()?;
```

TypeScript: `envelopeRouteMap(map, updatedAt)`. Schema:
[`json-schema/opto-sync-envelope.schema.json`](json-schema/opto-sync-envelope.schema.json).

## Hardened docs HTTP

Copied from the stronger k8s-cluster pattern (`t2v-v2t.rs` docs headers), not
the weaker `include_str!` services:

- Exact aliases: `/docs/api`, `/api/docs`, `/api/docs.json` (plus `/api-docs`)
- `Cache-Control: no-store`, `nosniff`, `Referrer-Policy: no-referrer`,
  `frame-ancestors 'none'`, `X-Frame-Options: DENY`
- HEAD has the same headers and an empty body
- POST → 405 with `Allow: GET, HEAD` and **no credential reflection**
- **No CDN** (no Scalar, no unpkg, no Swagger UI from the network)
- HTML is escaped locally

## Layout

- `json-schema/` — contracts (validate these; do not treat them as comments)
- `rust/` — crate `ores-api-docs` (Axum router behind feature `axum`)
- `clients/typescript` — Ajv 2020-12
- `clients/dart` — `@Rpc` annotation + `Unary` typedef
- `clients/gleam` — function types (Gleam has no annotations)
- `examples/` — pmap, canonical-cloud, chapter-publishing, cliptown, gha-indie-worker, hhm, hnpt maps
- `generated/` — committed Rust/TS/Dart/Gleam key objects (`generate-routes.py --check`)

```rust
let map = ores_api_docs::RouteMap::from_json_str(include_str!("route-map.json"))?;
let catalog = ores_api_docs::Catalog::from_map(map)?;
let app = axum::Router::new().merge(ores_api_docs::axum_router::router(catalog));
```

## Keeping the map in sync with code

JSON Schema is the contract. `scripts/check-route-sync.py` fails when:

- a map is not valid JSON Schema 2020-12 (`json-schema/route-map.schema.json`)
- an Axum `.route("...", get|post|...)` is missing from the map, or a map path is missing from source

Run it from pre-commit / pre-push (copy `.githooks/` into `.git/hooks/`) or CI:

```sh
python3 scripts/check-route-sync.py
# optional full draft-2020-12:
pip install jsonschema
```

`.merge(docs::router())` may add the standard aliases (`/docs/api`, `/api/docs`, `/api/docs.json`, …) without listing them as product keys.
