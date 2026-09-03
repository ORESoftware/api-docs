# api-docs

Language-neutral **route map**: a JSON object whose **keys are operations** and
whose **values are HTTP routes**. The map is the v1 interchange format for RPC
across Rust, TypeScript, Dart, Gleam, and Go. The same normalized map also
generates OpenAPI, OpenRPC, Connect, and JSON Hyper-Schema documents carrying
one shared semantic SHA-256, so a deployed runtime can reject mismatched API
docs before serving traffic.

Two stacks live here. **Do not send a v1 call object as a v2 RIDL frame.**

| Stack | Map `schema_version` | Wire object | Generator and admission |
| --- | --- | --- | --- |
| **v1 unary** | `1.0.0` | `rpc-call` / `rpc-receipt` (`op`: call\|receipt) | `scripts/generate-routes.py`, `scripts/rpc-contract-bundle.py`, strict peer TypeSpec/JSON-Schema/OpenAPI and Proto-projection audit |
| **v2 RIDL** | `2.x` | `rpc-frame` (`t`: call\|data\|end\|error\|cancel) | `python3 -m ridl.cli` / `scripts/ridl`, eight-language golden and malformed corpus |

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

## Language surfaces, not the interchange

The JSON map is what every language agrees on. **How a key is authored in
source is per-language**, and any combination is valid:

| Surface | Languages that typically use it |
| --- | --- |
| **Annotation / attribute / decorator** on a method | Rust `#[get]`, Axum `post(...)`, Dart `@Rpc`, Java `@GET`, C# `[HttpPost]` |
| **Param type(s)** | Axum extractors, Connect request message, JSON-RPC params |
| **Return type** | Axum `IntoResponse`, Connect response message, OpenRPC result |
| **Function type** | TypeScript `UnaryFn<Req, Res>`, Gleam `fn(Req) -> Res`, Dart `typedef Unary` |
| **Combination** | The usual case: attribute, typed request/response, and a named function type |

1:1 file↔route correspondence, such as Leptos/Dioxus or Next-style routing, is
optional. One file may hold several HTTP verbs.

JSON Schema: [`json-schema/route-binding.schema.json`](json-schema/route-binding.schema.json)
and [`json-schema/language-surface.schema.json`](json-schema/language-surface.schema.json).

Rust typed surface:

```rust
pub trait RpcMethod {
    const KEY: &'static str;
    const PATH: &'static str;
    type Params;
    type Output;
}
pub type UnaryFn<M> = fn(<M as RpcMethod>::Params) -> <M as RpcMethod>::Output;
```

## One normalized v1 contract, several standards

`scripts/rpc-contract-bundle.py` parses a route map once into a normalized
semantic contract. It validates every embedded Draft 2020-12 request, response,
error, path, and query schema before generating anything. It then projects the
same in-memory value into:

1. **OpenAPI 3.1** — `paths` + `operationId` = map key
2. **Connect JSON unary** — `POST /{service}/{Method}`
3. **OpenRPC 1.3** — method name = map key; params/result are the schemas
4. **JSON Hyper-Schema links** — one link per declared HTTP method
5. **Rust** route keys and complete RPC mechanism metadata
6. **TypeScript** route keys/types and complete RPC mechanism metadata
7. **Dart** route metadata and complete RPC mechanism metadata
8. **Gleam** route keys and complete RPC mechanism metadata
9. **Go** route metadata and complete RPC mechanism metadata

Every document operation has an `x-ores-rpc` extension containing:

- the operation key;
- the shared contract SHA-256;
- transports;
- TCP framing;
- delivery mode;
- alias information;
- opto-sync queue metadata when present.

Every generated language surface contains the same digest and the same complete
mechanism manifest. The digest ignores JSON formatting, source filename, and
object-key order, but changes when any operation key, path, method, schema,
transport, framing mode, delivery mode, alias, binding, or queue contract
changes.

A server can compare `ores_api_docs::contract_sha256(&map)` with the digest in
its served docs or generated client surface and fail closed on mismatch.

The projections are validated by closed JSON Schema Draft 2020-12 subsets:

- [`json-schema/openapi-3.1-subset.schema.json`](json-schema/openapi-3.1-subset.schema.json)
- [`json-schema/openrpc-1.3-subset.schema.json`](json-schema/openrpc-1.3-subset.schema.json)
- [`json-schema/connect-json-unary.schema.json`](json-schema/connect-json-unary.schema.json)
- [`json-schema/json-hyper-schema-links.schema.json`](json-schema/json-hyper-schema-links.schema.json)

URI templates in paths (`/v1/matters/{id}`) follow RFC 6570 level 1, which is
also how OpenAPI writes path params. Every template variable must be declared
and required by `path_params`.

## Typed query, path, and JSON payloads

Each map value may declare JSON Schema 2020-12 for the compile surface:

| Field | Meaning |
| --- | --- |
| `path_params` | Object schema whose properties are exactly the `{placeholders}` in `path`; every property is required |
| `query_schema` | Object schema for the query string |
| `request_schema` | JSON body / RPC payload |
| `response_schema` | Success JSON |
| `error_schema` | Documented error JSON |
| `alias_of` | Another key this route aliases; alias chains must be acyclic |
| `transports` | `http`, `tcp`, `websocket`, and/or `nats`; omit to infer `http`, or `websocket` for `/ws` |
| `tcp_framing` | `ndjson` by default, or `length-prefixed` with a four-byte big-endian length when TCP is listed |
| `delivery` | `direct` by default, or `opto_sync_queued` |
| `opto_sync` | `{ table, operation: upsert\|delete }` when delivery is queued |
| `binding` | Reviewed per-transport binding metadata preserved in the semantic digest |

## Peer TypeSpec and JSON Schema/OpenAPI authorities

TypeSpec (`idl/typespec/`) and the JSON Schema/OpenAPI track (`json-schema/`
plus the authored route-map operation inventory) are **peer top-level contract
authorities**. Both are human-authored and independently reviewed. Neither is
generated from, subordinate to, or allowed to overwrite the other.

TypeSpec projects toward SQL, Protobuf/gRPC, and wire clients. JSON
Schema/OpenAPI projects toward client interfaces/types, SQL, and write clients.
Protobuf (`idl/protobuf/`) is the TypeSpec-derived binary/streaming projection
plus field-number compatibility ledger. It remains committed and
release-vetoing because proto3 and current emitters cannot reproduce every
checked closed-world, conditional, and compatibility edge exactly.

`scripts/cross-check-rpc-idl.py` performs the existing structural comparison.
`scripts/audit-rpc-idl.py` adds strict semantic admission:

- an absent constraint is a mismatch, not “not comparable”;
- expected representation loss is an exact allow-list in
  [`idl/expected-deltas.json`](idl/expected-deltas.json);
- every Proto assignment must be parsed;
- field numbers must be positive, unique, and present in the ledger;
- enum values must match the ledger;
- the TypeSpec declaration set and reviewed cross-model references are closed.

`idl/authority-contract.json` prevents a hierarchy from being reintroduced and
requires comparisons of normalized models, generated SQL, and client types
between both authorities. It also requires Diesel/SeaORM comparison of schema,
migrations, constraints, and relations. `scripts/compare-authority-artifacts.py`
performs exact manifest comparison. Any unexpected discrepancy means **halt and
evaluate**; CI must not choose a winner or overwrite an authority.

The RPC model cross-check and digest-bound docs/client bundle are implemented.
The two SQL emitters and Diesel/SeaORM artifact production remain explicitly
`not_yet_materialized`; production parity must not be claimed until exact
manifests exist and pass the comparator. Candidate output may be written only
below `generated/idl/projections/` or a runner temporary directory.

## Same v1 envelope on every transport

The JSON call and receipt frames are the same on every wire:

- [`json-schema/rpc-call.schema.json`](json-schema/rpc-call.schema.json)
- [`json-schema/rpc-receipt.schema.json`](json-schema/rpc-receipt.schema.json)

Transport bindings:

- **HTTP** — declared method + expanded path + query + JSON body
- **WebSocket** — one text frame is one call or receipt object
- **TCP** — one object per line for NDJSON, or bounded length-prefixed JSON
- **NATS** — the same JSON on a declared subject; this crate does not open NATS

This crate does not open sockets. Servers map `RouteKey` onto Axum, a
WebSocket handler, a TCP accept loop, or an external NATS adapter.

```rust
let call = ores_api_docs::RpcCall::new("c1", "get_item");
let line = call.to_ndjson()?;
```

## v2 RIDL boundary

The v2 RIDL frame uses the discriminant `t` with call/data/end/error/cancel
variants and is not wire-compatible with the v1 `op` envelope. Its reviewed
emitter set is exactly:

- Dart
- Gleam
- Go
- Kotlin
- Python
- Rust
- Swift
- TypeScript

The standalone Rust v2 reference runtime is a nested crate with a committed
[`runtime/rust/Cargo.lock`](runtime/rust/Cargo.lock), and CI always tests it
with `--locked`.

## Generation and verification

Committed route-key objects remain generated by `scripts/generate-routes.py`:

- TypeScript `Routes.get_matter.path` / `RouteHandlers<Ctx>`
- Rust `RouteKey` enum
- Dart `Routes.byKey['get_matter']`
- Gleam `RouteKey` custom type

The digest-bound bundle additionally emits temporary Rust, TypeScript, Dart,
Gleam, and Go surfaces from the same normalized contract and compiles the Go
outputs in CI.

```sh
python3 scripts/generate-routes.py --check
python3 scripts/test_validate_authority_contract.py -v
python3 scripts/validate-authority-contract.py
python3 scripts/test_compare_authority_artifacts.py -v
python3 scripts/test_cross_check_rpc_idl.py -v
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_audit_rpc_idl.py -v
python3 scripts/audit-rpc-idl.py
python3 scripts/test_rpc_contract_bundle.py -v
python3 scripts/rpc-contract-bundle.py --check
```

To inspect a bundle without committing generated output:

```sh
python3 scripts/rpc-contract-bundle.py \
  --map examples/rpc-transports.route-map.json \
  --out /tmp/ores-rpc-contracts \
  --check
```

Frontend code calls `Routes["get_matter"]`, `lookup("get_matter")`, or the
language equivalent instead of hard-coding `/v1/matters/{id}`. Backend code
handles every generated key.

## Zed package: `oresoftware/api-docs`

Servers and clients pin this library through Zed rather than a Cargo Git URL:

```toml
[dependencies]
"oresoftware/api-docs" = "^2.0.0"
```

`zed install` materializes it at `.vendor/.zed/oresoftware/api-docs`. Cargo can
then use the Rust target:

```toml
ores-api-docs = { path = ".vendor/.zed/oresoftware/api-docs/rust" }
```

The Zed package build itself runs RIDL drift, committed route drift, strict
peer-authority/projection admission, and digest-bound bundle verification.
Per-org `RouteKey` / `Routes` objects still belong in that org's `*-interfaces`
repository. This package is the shared schema, docs, generator, and runtime
mechanism engine. It does not depend on opto-sync or ores-otel.

## opto-sync: RPC uses sync; sync does not use RPC

opto-sync must not depend on this crate, and this crate must not depend on an
opto-sync client. Route maps may travel as opto-sync documents so replicas share
keys; **RPC calls are not opto-sync records**. If opto-sync speaks TCP, it may
carry NDJSON that happens to be an RPC call frame—that is plain JSON, not a type
import.

- scope / kind: `ores.api-docs.route-map`
- identity key: `id`, the service name
- LWW keys: `updatedAt,syncedAt`

```rust
let env = ores_api_docs::RouteMapEnvelope::wrap(&map, "1689940800123456789")?;
let map = env.into_map()?;
```

TypeScript: `envelopeRouteMap(map, updatedAt)`. Schema:
[`json-schema/opto-sync-envelope.schema.json`](json-schema/opto-sync-envelope.schema.json).

## ores-otel: copy fields; no crate edge

This crate must not depend on ores-otel, and ores-otel must not depend on this
crate. Optional W3C `traceId` / `spanId` fields use the same names as ores-otel
log context. Telemetry attributes are a JSON object described by
[`json-schema/telemetry-attributes.schema.json`](json-schema/telemetry-attributes.schema.json).

```rust
let attrs = ores_api_docs::TelemetryAttributes::start(
    "hhm-api-server",
    "get_item",
    ores_api_docs::Transport::Http,
);
```

Do not put payloads, tokens, or PII in telemetry fields.

## Hardened docs HTTP

The Rust router exposes exact aliases such as `/docs/api`, `/api/docs`, and
`/api/docs.json`, with:

- `Cache-Control: no-store`;
- `X-Content-Type-Options: nosniff`;
- `Referrer-Policy: no-referrer`;
- `frame-ancestors 'none'`;
- `X-Frame-Options: DENY`;
- HEAD parity with an empty body;
- POST returning 405 with `Allow: GET, HEAD` and no credential reflection;
- no Scalar, unpkg, Swagger UI, or other runtime CDN dependency;
- locally escaped HTML.

## Layout

- `json-schema/` — JSON Schema/OpenAPI peer-authority inputs, route-map admission, bindings, and validation profiles
- `idl/typespec/` — TypeSpec peer-authority inputs
- `idl/protobuf/` — TypeSpec-derived Protobuf projection, reviewed deltas, Buf policy, and field-number lock
- `idl/authority-contract.json` — unordered peer-authority and SQL/type/ORM convergence policy
- `rust/` — `ores-api-docs` Rust crate and hardened Axum docs router
- `runtime/` — v2 RIDL reference runtimes and conformance fixtures
- `clients/typescript` — TypeScript v1 client and tests
- `clients/dart` — Dart v1 client and tests
- `clients/gleam` — Gleam v1 client and tests
- `ridl/emit/` — reviewed eight-language v2 emitters
- `scripts/rpc_contract/` — normalized v1 model, documentation projections, language emitters, and bundle verifier
- `examples/` — pmap, canonical-cloud, chapter-publishing, cliptown, gha-indie-worker, hhm, hnpt, and multi-transport maps
- `generated/` — committed Rust/TypeScript/Dart/Gleam key objects checked for drift

```rust
let map = ores_api_docs::RouteMap::from_json_str(include_str!("route-map.json"))?;
let digest = ores_api_docs::contract_sha256(&map);
let catalog = ores_api_docs::Catalog::from_map(map)?;
let app = axum::Router::new().merge(ores_api_docs::axum_router::router(catalog));
```

## Keeping maps in sync with code

`json-schema/route-map.schema.json` remains the admission contract for route-map
instances. `scripts/check-route-sync.py` fails when:

- a map is not valid Draft 2020-12;
- an Axum `.route("...", get|post|...)` is missing from the map;
- a map path is missing from source.

```sh
python3 scripts/check-route-sync.py
```

`.merge(docs::router())` may add the standard documentation aliases without
listing them as product operation keys.
