//! Trusted ingress metadata, independent of any HTTP server framework.
//!
//! This type used to live in `rpc_axum`. It only ever wrapped an
//! `http::HeaderMap`, so it belongs to the transport-neutral operation runtime:
//! an AWS Lambda adapter, a worker, or a test harness needs to hand trusted
//! ingress headers to an operation without linking Axum. `rpc_axum` re-exports
//! it, so `ores_api_docs::rpc_axum::RpcV1HttpContext` and the crate-root
//! re-export both keep resolving for already-generated consumer code.

use http::HeaderMap;

/// Trusted metadata supplied by the concrete ingress rather than by the
/// application RPC envelope.
///
/// Product dispatchers should use these headers for proxy-derived client
/// identity, transport authentication, request correlation, and other values
/// whose trust depends on the HTTP ingress. `RpcV1Call::headers` remains the
/// typed application-header surface and must not be treated as a substitute for
/// ingress metadata such as `cf-connecting-ip` or `x-real-ip`.
///
/// `OperationContext` stores at most one of these and derives
/// `trusted_headers()` from it, so there is a single source of truth for what
/// the ingress vouched for. An ingress that vouches for nothing (a direct
/// Lambda invocation, an in-process call) is represented by the *absence* of
/// this value, which is distinguishable from an ingress that forwarded an empty
/// header set.
#[derive(Clone, Debug)]
pub struct RpcV1HttpContext {
    request_headers: HeaderMap,
}

impl RpcV1HttpContext {
    #[must_use]
    pub fn from_headers(request_headers: HeaderMap) -> Self {
        Self { request_headers }
    }

    #[must_use]
    pub fn request_headers(&self) -> &HeaderMap {
        &self.request_headers
    }

    #[must_use]
    pub fn into_request_headers(self) -> HeaderMap {
        self.request_headers
    }
}
