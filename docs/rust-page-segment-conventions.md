# Rust filesystem page segment conventions

The Rust web-server filesystem contract uses `src/pages/**/page.rs` as the only route leaf. Optional convention files in the page directory or any ancestor under `src/pages` modify how that leaf is rendered. Discovery is build-time only; standalone servers, generated web Lambdas, and local process-per-request hosts consume generated symbols and never walk the source tree at request time.

| Rust file | Server/Lambda behavior | Client-navigation behavior |
| --- | --- | --- |
| `layout.rs` | Wraps the descendant document. Ancestors are applied leaf-to-root so the root layout is outermost. | Stable segment identity. A soft-navigation runtime should preserve a mounted layout while navigating between sibling descendants. |
| `template.rs` | Wraps the descendant document inside the segment layout. | New instance on every navigation through the segment; use for intentional reset/remount behavior. |
| `error.rs` | Handles non-not-found `PageError` values from the descendant subtree. It does not catch failures from the same segment's own template/layout; those propagate to the parent boundary. | Segment error-boundary identity. |
| `loading.rs` | Exposes the nearest pending-state fallback through a separate generated function. It is not rendered after blocking for page completion. | A streaming or soft-navigation host may show it while the descendant page future is genuinely pending. |
| `not_found.rs` | Handles the typed `PageError::NotFound` signal from the descendant subtree before generic `error.rs`. | Segment 404 fallback. |
| `page.rs` | Unique route leaf. | Destination content. |

Rust uses `not_found.rs`, not JavaScript's `not-found.tsx`, because authored Rust identifiers and files follow snake_case.

## Composition order

For a page under `src/pages/account/settings/page.rs`, segment ancestry is resolved once from root to leaf. Execution starts at the leaf and moves outward. For each segment:

1. an explicit `PageError::NotFound` may be recovered by that segment's `not_found.rs`;
2. another `PageError` may be recovered by that segment's `error.rs`;
3. a successful document is wrapped by that segment's `template.rs`;
4. a successful document is wrapped by that segment's `layout.rs`;
5. the resulting `PageResult` moves to the parent segment.

This means a parent `error.rs` can catch a failure produced by a child segment's page, error fallback, template, or layout, while an `error.rs` does not pretend to catch its own segment's layout failure.

## Stable ABIs

Authored files export these functions:

```rust
// layout.rs
pub fn layout(ctx: PageContext, child: PageDocument) -> PageLayoutFuture;

// template.rs
pub fn template(ctx: PageContext, child: PageDocument) -> PageTemplateFuture;

// error.rs
pub fn error(ctx: PageContext, error: PageError) -> PageErrorBoundaryFuture;

// loading.rs
pub fn loading(ctx: PageContext) -> PageLoadingFuture;

// not_found.rs
pub fn not_found(ctx: PageContext) -> PageNotFoundFuture;
```

The generated web `lambda.rs` does not copy these files or implement a second composition engine. It calls the same compiled exported page entry as the standalone Axum server. That page entry already closes over the admitted segment chain. The Lambda additionally exposes the nearest loading fallback so a host with genuine pending-state semantics can use it.

## Process-per-request development

`ores-stack dev` without PPR runs the normal long-lived server binaries.

`ores-stack dev --process-per-request` / `--ppr` uses a stable local supervisor/proxy. On every matched `page.rs`, API `route.rs`, API Lambda, or RPC request it refreshes the corresponding generated Lambda source, generates a disposable local `main.rs`, compiles that isolated Lambda build unit, launches it, and lets that child execute the generated `lambda.rs::run(...)` entry. The child exits after the request.

The local `main.rs` is **adapter code, not application source authority**. It must never be committed. Cargo normally needs a filesystem path, so the portable implementation materializes `main.rs` under an ignored temporary/build directory and reuses a shared Cargo target directory for incremental compilation. Platforms that can compile from an anonymous/memory-backed file descriptor may add that as an optimization without changing the Lambda ABI.

The transport between the stable supervisor and the disposable local `main.rs` is intentionally **not** part of the provider-neutral Lambda contract. Local development may hand the accepted client socket/FD/handle directly to the child, pass a duplicated handle plus side-channel metadata, use a socketpair, or use a framed pipe/stdin protocol. The only invariant is that the generated local wrapper reconstructs the admitted request correctly, calls the generated `lambda.rs` API, produces a valid HTTP/Lambda response, and exits. AWS/GCP wrappers are free to use their native provider invocation mechanisms instead.

`ores-middleware` remains owned by the stable supervisor/proxy for PPR mode. A local transport choice must preserve the middleware lifecycle: request admission occurs before invoking the Lambda child, and response/finalization semantics must not be bypassed merely because a raw socket/handle is available. A direct socket handoff is therefore valid when the selected local adapter preserves those lifecycle guarantees; it is not globally forbidden.

## Loading is not fake Suspense

A normal request/response Lambda that waits for the page to finish cannot honestly claim `loading.rs` behavior. The loading ABI is therefore separate from `run()`. It becomes visible when either:

- the PPR transport gains a streaming protocol/socket adapter that can emit a fallback before the final document; or
- the browser soft-navigation/HMR layer requests the loading boundary while the new page Lambda executes.

Until then, `loading.rs` is compiled, hashed, and packaged as part of page semantics but is not rendered post-hoc.

## Invalidation and trust boundaries

Edits to `page.rs`, inherited `layout.rs`, `template.rs`, `error.rs`, `loading.rs`, or `not_found.rs` change the page render identity. Build/dev tooling must also register those authored files as rerun inputs. Segment discovery rejects symlinks and path escapes under the same repository-root confinement rules as page discovery.

Runtime configuration (`.ores-mw.toml`, `.ores-stack.toml`, `.ores-sops.toml`, and other admitted server contracts) is not copied into `lambda.rs`. A fresh PPR child re-reads runtime configuration where the application state/runtime owns it, while the supervisor itself re-admits middleware configuration because middleware executes before the Lambda child boundary.
