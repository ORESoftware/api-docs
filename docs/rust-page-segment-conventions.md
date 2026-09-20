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

`ores-stack dev --process-per-request` / `--ppr` uses a stable local HTTP supervisor. The supervisor owns `ores-middleware`, request limits, HTTP admission, correlation, and request/response transport. After middleware admission it launches a short-lived Lambda host process for the matched page/API operation. The child has no listener and receives one normalized invocation through inherited stdio/pipe handles, invokes generated `lambda.rs`, emits one normalized response, and exits.

The temporary local `main.rs` wrapper is build material only and must not be committed. Cargo still needs source on a filesystem, so an implementation may materialize it in an ignored/temp build directory while sharing a Cargo target directory for incremental compilation. Passing a raw client socket into the child is intentionally avoided: the supervisor remains the sole network and middleware trust boundary.

## Loading is not fake Suspense

A normal request/response Lambda that waits for the page to finish cannot honestly claim `loading.rs` behavior. The loading ABI is therefore separate from `run()`. It becomes visible when either:

- the PPR transport gains a framed streaming protocol that can emit a fallback before the final document; or
- the browser soft-navigation/HMR layer requests the loading boundary while the new page Lambda executes.

Until then, `loading.rs` is compiled, hashed, and packaged as part of page semantics but is not rendered post-hoc.

## Invalidation and trust boundaries

Edits to `page.rs`, inherited `layout.rs`, `template.rs`, `error.rs`, `loading.rs`, or `not_found.rs` change the page render identity. Build/dev tooling must also register those authored files as rerun inputs. Segment discovery rejects symlinks and path escapes under the same repository-root confinement rules as page discovery.

Runtime configuration (`.ores-mw.toml`, `.ores-stack.toml`, `.ores-sops.toml`, and other admitted server contracts) is not copied into `lambda.rs`. A fresh PPR child re-reads runtime configuration where the application state/runtime owns it, while the supervisor itself re-admits middleware configuration because middleware executes before the Lambda child boundary.
