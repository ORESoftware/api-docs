use crate::{PageContext, PageDocument, PageError, PageResult};
use std::{future::Future, pin::Pin};

/// Async result of an authored `template.rs` wrapper.
pub type PageTemplateFuture = Pin<Box<dyn Future<Output = PageResult> + Send + 'static>>;

/// `template.rs` has the same server-side data shape as `layout.rs`, but a
/// browser navigation runtime must treat the template identity as remounting on
/// every navigation rather than preserving the previous client instance.
pub type PageTemplateFn = fn(PageContext, PageDocument) -> PageTemplateFuture;

/// Async result of one authored `error.rs` boundary.
pub type PageErrorBoundaryFuture = Pin<Box<dyn Future<Output = PageResult> + Send + 'static>>;

/// Stable provider-neutral ABI for `src/pages/**/error.rs`.
///
/// The boundary receives the same request/page context plus the child error and
/// may return fallback UI. Generated composition never exposes error text to an
/// HTTP response unless authored boundary code deliberately renders it.
pub type PageErrorBoundaryFn = fn(PageContext, PageError) -> PageErrorBoundaryFuture;

/// Async result of one authored `loading.rs` fallback.
pub type PageLoadingFuture = Pin<Box<dyn Future<Output = PageResult> + Send + 'static>>;

/// Stable provider-neutral ABI for `src/pages/**/loading.rs`.
///
/// This function renders the nearest segment loading fallback. It is exported
/// separately from the final page entry because a host must have a real
/// streaming/soft-navigation boundary before it can show fallback UI while the
/// page future is still pending; blocking until completion is not Suspense.
pub type PageLoadingFn = fn(PageContext) -> PageLoadingFuture;

/// Async result of one authored `not_found.rs` fallback.
pub type PageNotFoundFuture = Pin<Box<dyn Future<Output = PageResult> + Send + 'static>>;

/// Stable provider-neutral ABI for `src/pages/**/not_found.rs`.
///
/// Generated page composition resolves the nearest boundary for an explicit
/// `PageError::NotFound` before considering generic `error.rs` boundaries.
pub type PageNotFoundFn = fn(PageContext) -> PageNotFoundFuture;
