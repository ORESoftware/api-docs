use crate::{PageContext, PageDocument, PageResult};
use std::{future::Future, pin::Pin};

/// Async result of one authored `layout.rs` wrapper.
///
/// A layout receives the same immutable request/page context plus the already
/// rendered child document. It may wrap HTML, add/remove response headers, or
/// adjust status while preserving the normal PageResult error boundary.
pub type PageLayoutFuture = Pin<Box<dyn Future<Output = PageResult> + Send + 'static>>;

/// Stable provider-neutral ABI for `src/pages/**/layout.rs`.
///
/// Generated compile glue discovers layouts at build/check time and applies
/// them leaf-to-root, so a root layout is the outermost wrapper. No production
/// request handler walks the source filesystem.
pub type PageLayoutFn = fn(PageContext, PageDocument) -> PageLayoutFuture;
