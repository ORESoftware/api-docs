# Rust filesystem routing and RPC boundary

This document defines the fleet convention for Rust `*-web-server.rs`, `*-admin-web-server.rs`, `*-api-server.rs`, and `*-admin-api-server.rs` repositories.

## 1. Two route namespaces, one strict boundary

Browser pages and RPC operations are different namespaces.

- Web/admin-web: `src/pages/**/page.rs` is authoritative for browser URL-to-page-module mapping. A page may **consume** generated `api-docs` clients, but a page is never automatically published as an RPC method.
- API/admin-API: `api-docs` route maps and their TypeSpec + JSON Schema/OpenAPI authorities remain authoritative for operation keys, parameter/body/result types, transports, and contract digest. `src/routes/**/route.rs` is an optional filesystem organization of server handlers and must be checked against that contract.

This makes the web layer feel like Next.js without collapsing the web/API trust boundary.

## 2. Filesystem grammar

For web surfaces the route root is `src/pages` and the leaf is `page.rs`:

| File | Canonical URL |
| --- | --- |
| `src/pages/page.rs` | `/` |
| `src/pages/about/page.rs` | `/about` |
| `src/pages/users/[id]/page.rs` | `/users/{id}` |
| `src/pages/docs/[...slug]/page.rs` | `/docs/{*slug}` |
| `src/pages/docs/[[...slug]]/page.rs` | `/docs` and `/docs/{*slug}` |

For API surfaces the optional route root is `src/routes` and the leaf is `route.rs`:

| File | Canonical path checked against api-docs |
| --- | --- |
| `src/routes/v1/health/route.rs` | `/v1/health` |
| `src/routes/v1/matters/[id]/route.rs` | `/v1/matters/{id}` |

Rules:

1. Static siblings sort before dynamic siblings, then catch-all, then optional catch-all.
2. Parameter names may contain ASCII letters, numbers, `_`, and `-`.
3. Catch-all segments must be final.
4. `[id]` and `[slug]` at the same shape conflict even though the names differ.
5. Conflicts fail the build; route registration order never silently decides meaning.
6. Generated route inventories are sorted and content-digested so two identical trees produce identical manifests.

`ores_api_docs::FsRoute` implements the grammar and emits Axum 0.8 and Dioxus Router 0.7 path syntax. Leptos/Axum adapters consume the canonical/Axum projection.

## 3. Typed API calls from pages

`api-docs` should help here, but as a **client generator/validator**, not by treating pages as RPC servers.

Each page may declare the operation keys it consumes in generated page metadata. During generation, every key is resolved against the sibling `*-interfaces`/`api-docs` route bundle. The generated page module imports service-specific generated request/response/path types and the client-only `ores-api-docs-client` facade. Unknown operation keys or contract-digest drift fail the build.

Conceptually:

```rust
// generated; do not hand-edit
pub struct PageApi<T> {
    transport: T,
}

impl<T: TypedApiTransport> PageApi<T> {
    pub async fn get_matter(
        &self,
        params: generated::GetMatterParams,
    ) -> Result<generated::GetMatterOutput, T::Error> {
        self.transport.call(generated::RouteKey::GetMatter, params).await
    }
}
```

The web server owns only client credentials appropriate to its realm. It does not import `api-docs` Axum router helpers, server dispatch registration, admin credentials, or write ORM authority.

## 4. Dynamic page prerendering

Dynamic pages can be deterministic at build/install time without forcing every framework through the same renderer.

The shared pipeline has two phases:

1. **Route compilation** scans `src/pages/**/page.rs`, validates conflicts, and produces the stable filesystem route manifest.
2. **Prerender enumeration** executes a Rust build helper/xtask that asks each dynamic page for finite static params, normalizes the values, sorts and deduplicates resulting URLs, then writes a content-addressed `generated/prerender-routes.json` before renderer-specific SSG begins.

A dynamic page implements a small framework-neutral provider in its module or sibling metadata:

```rust
pub fn prerender_params(ctx: &PrerenderContext) -> Result<Vec<PageParams>, PrerenderError> {
    // May use a digest-pinned api-docs typed client to read the API server.
    // Must return finite, serializable params.
}
```

The prerender runner records:

- filesystem route-manifest digest,
- sibling RPC contract digest when API calls were used,
- normalized parameter list,
- resulting URL list,
- optional input snapshot/version supplied by the data source.

Network-backed enumeration is therefore reproducible only when its source version/snapshot is explicit. CI should reject an unversioned mutable source for release builds.

### Framework adapters

- **Dioxus 0.7:** generate the `Routable` enum from the manifest and feed the normalized URL list to the existing fullstack SSG `static_routes` build endpoint / `dx bundle --web --ssg` flow.
- **Leptos:** generate route declarations and feed the normalized routes into the `leptos_axum` SSG route generation path (`generate_route_list_with_ssg` / `StaticRouteGenerator`).
- **MASH/Maud + Axum:** register the generated Axum path table for SSR and use a Rust `xtask prerender`/shared binary to render the same normalized URL list to deterministic output paths.

Renderer-specific code is an adapter; route meaning and prerender URL enumeration are shared.

## 5. API `route.rs` and api-docs

API filesystem routing is deliberately secondary to the RPC contract.

A `route.rs` file can implement one or more HTTP methods/operation keys, but generation must verify:

- its filesystem-derived canonical path exists in the route map,
- every declared operation key resolves to that path/method,
- generated path/query/header/body/output types match the TypeSpec + JSON Schema/OpenAPI convergence gate,
- the API/admin-API binary exposes the expected `api-docs` RPC dispatch surface,
- browser web/admin-web binaries do **not** expose that server surface.

This provides Next-like organization without allowing a filename rename to silently mutate a public RPC contract.

## 6. Build integration

Stable Rust/Cargo does not inherently know that a proc macro read an arbitrary directory. Build integration therefore emits `cargo:rerun-if-changed=src/pages` / `src/routes` plus the discovered route files. Generated Rust goes to `OUT_DIR` or the fleet-standard read-only `generated/` tree; authored route files stay under `src/pages`/`src/routes`.

Recommended checks:

```text
route-fs validate      # parse grammar, detect conflicts, deterministic ordering
route-fs manifest      # emit normalized JSON + digest
route-fs rpc-check     # API route.rs ↔ api-docs route-map parity
route-fs prerender     # enumerate normalized dynamic page URLs
route-fs framework     # emit MASH/Axum, Leptos, or Dioxus adapter code
```

The durable implementation is Rust. Do not add a new Python routing generator.

## 7. Migration rule

Existing hand-written routers may coexist during migration, but CI must compare the legacy registered paths with the filesystem manifest and fail on unexplained drift. Once parity is green, the generated router becomes the sole registration inventory. Product-specific authorization remains in the existing reviewed `*-lib-core`; routing generation must never weaken auth or realm separation.
