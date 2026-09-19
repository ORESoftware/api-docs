# Web page Lambda contract — draft

> **Scope: `*-web-server.rs` / `*-admin-web-server.rs` only. This is not the API-server Lambda/RPC contract.**

This draft extends the filesystem page contract so one browser URL folder may contain:

```text
src/pages/x/y/z/
  page.rs      # authored request/page renderer
  gen.rs       # optional authored build-time static path enumeration
  lambda.rs    # generated; never authored
```

The URL remains derived only from the path of `page.rs`. `lambda.rs` is a deployment projection of that same page and is never a second routing authority.

## Ownership

- `page.rs`: authored page behavior and `#[ores_page(...)]` documentation metadata.
- `gen.rs`: optional authored deterministic prerender enumeration (`#[ores_generate]`).
- `lambda.rs`: generated executable entrypoint for exactly that page route.
- `api-docs`: filesystem grammar, page ABI, page metadata/docs, provider-neutral source generation.
- `ores-stack`: discovery, admission, materialization/check mode, generated Cargo build plan, packaging and fleet rollout.
- `*-lambdas`: provider lifecycle adapters and middleware for AWS/GCP/etc.

Browser pages remain separate from RPC operations. No `rpc.rs`, `RpcV1` dispatcher, operation key, or API route map is generated for a page merely because a page Lambda is enabled.

## Generated `lambda.rs`

The generated file must contain executable `main` entrypoints selected by mutually exclusive build features, while provider lifecycle code is delegated to a stable runtime facade exposed by the owning `*-lambdas` package. Conceptually:

```rust
// @generated ... DO NOT EDIT.
// WEB SERVER PAGE LAMBDA ONLY — not an API-server RPC adapter.

#[path = "page.rs"]
mod __ores_page;

pub const ORES_PAGE_ROUTE: &str = "/x/y/z";

async fn __ores_handle_page(request: PageHttpRequest) -> Result<PageHttpResponse, RuntimeError> {
    invoke_page(request, ORES_PAGE_ROUTE, |ctx| __ores_page::__ores_page_boxed(ctx)).await
}

#[cfg(feature = "ores-page-lambda-aws")]
#[tokio::main]
async fn main() -> Result<(), RuntimeError> {
    provider_runtime::aws::run_page(__ores_handle_page).await
}

#[cfg(feature = "ores-page-lambda-gcp")]
#[tokio::main]
async fn main() -> Result<(), RuntimeError> {
    provider_runtime::gcp::run_page(__ores_handle_page).await
}
```

The actual generator rejects builds with zero or more than one provider feature enabled.

The generated Cargo build plan aliases the organization-specific provider package to one stable crate name (for example `ores_page_lambda_runtime`), so generated source does not contain organization names.

There is exactly one sibling `lambda.rs` per `page.rs`; `ores-stack` compiles that same generated source into separate AWS and GCP artifacts by selecting the appropriate provider feature in generated build manifests. The authored page source is identical for standalone Axum, AWS Lambda, and GCP hosting.

## HTTP normalization seam

Provider adapters normalize their event/request into a provider-neutral `PageHttpRequest` before page invocation. The first contract should include at least:

- method (GET/HEAD initially; explicit rejection for unsupported methods),
- original URI/path and query,
- canonical lowercase header names,
- bounded body bytes,
- remote/request/provider metadata that middleware is allowed to trust,
- route parameters extracted from the validated filesystem pattern.

The page adapter returns `PageHttpResponse` containing status, headers, and bytes/HTML. Provider adapters own translation to API Gateway/Lambda Function URL or GCP HTTP response envelopes.

Do not let request headers masquerade as trusted provider identity. The provider host must explicitly mark ingress provenance.

## Routing model

Standalone web servers can register every validated page on one Axum router. A page Lambda is different: its deployment artifact already identifies one page route, so it should not construct the entire application router.

The Lambda runtime therefore needs a small page-route matcher/middleware chain rather than the full standalone router:

1. normalize provider HTTP event;
2. require method/path compatibility with this generated page target;
3. extract validated route params;
4. run web middleware appropriate to the page realm;
5. construct `PageContext`;
6. call `page.rs`'s macro-generated boxed entrypoint;
7. finalize HTML/assets/headers;
8. convert the response to the provider envelope.

Common page response finalization (CSS/WASM injection, cache policy, error mapping) should be shared with the standalone generated Axum router rather than duplicated as text in two generators.

## AWS and GCP

AWS Rust targets use the OS-only Lambda runtime (`provided.al2023`) and the Rust Lambda runtime client. The provider adapter should support at least API Gateway HTTP API v2 and Lambda Function URLs before adding ALB or direct non-HTTP invocation.

GCP must use a provider host that can run the Rust binary through the supported HTTP/custom-runtime/OS-only deployment path. The generated page code must not assume a managed Rust language runtime. The GCP adapter owns `PORT`/runtime lifecycle details.

## `ores-stack` build/package contract

`ores-stack` should eventually provide commands along these lines:

```text
ores-stack web-lambda inspect
ores-stack web-lambda sync
ores-stack web-lambda sync --check
ores-stack web-lambda build --provider aws
ores-stack web-lambda build --provider gcp
ores-stack web-lambda package --provider aws
ores-stack web-lambda package --provider gcp
```

`sync` materializes `lambda.rs` next to `page.rs`. `--check` must never write and must fail on missing/stale/hand-edited generated files. A symlink at `lambda.rs` is refused.

Cargo compilation can use an `ores-stack` generated package/manifest under `.generated/web-lambda/<unit>/Cargo.toml` whose `[[bin]]` path points at `src/pages/.../lambda.rs`. This avoids permanently hand-editing the product `Cargo.toml` for every page Lambda.

Artifacts and deployment plans must carry at least source SHA, filesystem-route-manifest digest, page source SHA, generated lambda SHA, provider, architecture, runtime, and immutable dependency pins.

## Configuration

Do not encode deployment decisions in `#[ores_page]`. Page semantics/documentation belong there; fleet deployment selection belongs in a separate config (working name `.ores-web-lambda.toml`).

The config should select routes by canonical path/source/tag and declare targets such as AWS or GCP. It may set deployment concerns (memory, timeout, architecture, ingress mode, middleware profile, networking), but it must never change the page route or page behavior.

TypeSpec and JSON Schema Draft 2020-12 should be independently authored peers for both config and generated deployment-plan documents and checked with `typespec-json-schema-validator`.

## Migration / fleet rollout

1. Land the shared contract in `api-docs`.
2. Add `ores-stack web-lambda inspect/sync --check` without changing existing web builds.
3. Add provider adapters in the fleet `*-lambdas` template and prove AWS + GCP with one representative web server.
4. Move any web-page `gen.rs` accidentally created in `*-api-server.rs` into the sibling `*-web-server.rs`; do not silently reinterpret API `gen.rs`.
5. Run read-only audit across the 25 orgs and produce a migration manifest before writing product repositories.
6. Opt in a few routes first, compare standalone Axum and Lambda response fixtures, then expand.

## Acceptance tests

- path `src/pages/x/y/z/page.rs` deterministically maps to `/x/y/z` and sibling generated `lambda.rs`;
- optional/dynamic/catch-all routes preserve the existing filesystem grammar;
- generated Lambda source never imports RPC server/dispatch symbols;
- generated source is byte-stable and `rustfmt --check` stable;
- generated file contains a generated marker and cannot be confused with authored code;
- AWS and GCP adapters produce semantically identical page responses for the same normalized request fixture;
- middleware/auth failures are fail-closed and do not call `page.rs`;
- standalone Axum and provider adapters share response-finalization fixtures;
- `--check` reports stale/missing/hand-edited/symlink output without writing;
- static-only routes continue to use `gen.rs` only at build time; `gen.rs` is never exposed as a request handler.
