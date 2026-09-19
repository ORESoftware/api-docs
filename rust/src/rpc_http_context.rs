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
/// Who vouched for a set of ingress headers.
///
/// "Trusted" is not one thing: API Gateway, a load balancer, a CDN and an
/// operator-run reverse proxy each guarantee different headers, and a test
/// harness guarantees none. A policy deciding whether to believe
/// `x-forwarded-for` or an access JWT needs to know which of them is speaking.
///
/// `#[non_exhaustive]`: more ingress kinds are expected. Match with a wildcard
/// arm that refuses, never one that trusts.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IngressProvenance {
    /// The adapter did not say. What every constructor that predates this enum
    /// produces, so existing servers keep their behaviour. A policy that cares
    /// about provenance should treat this as the weakest claim.
    #[default]
    Unspecified,
    /// A reverse proxy operated as part of the deployment.
    ReverseProxy,
    /// A cloud load balancer.
    LoadBalancer,
    /// A managed API gateway (for example Amazon API Gateway).
    ApiGateway,
    /// A provider function URL (for example an AWS Lambda function URL).
    FunctionUrl,
    /// A CDN or edge network that authenticates and forwards requests.
    Edge,
    /// A test harness. Never admissible as a production identity source.
    Test,
}

#[derive(Clone)]
pub struct RpcV1HttpContext {
    request_headers: HeaderMap,
    provenance: IngressProvenance,
}

/// Redacted on purpose. Ingress headers are where credentials live
/// (`authorization`, `cookie`, access JWTs, request signatures), and a context
/// is an easy thing to `{:?}` into a log line. Only header *names* are shown,
/// sorted and de-duplicated; values never are. Read them through
/// [`RpcV1HttpContext::request_headers`] when you really need them.
impl std::fmt::Debug for RpcV1HttpContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = self
            .request_headers
            .keys()
            .map(http::HeaderName::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        formatter
            .debug_struct("RpcV1HttpContext")
            .field("provenance", &self.provenance)
            .field("header_names", &names)
            .field("header_values", &"<redacted>")
            .finish()
    }
}

impl RpcV1HttpContext {
    /// Ingress headers whose provenance the adapter does not state
    /// ([`IngressProvenance::Unspecified`]). Prefer
    /// [`Self::from_headers_with_provenance`] in new adapters.
    #[must_use]
    pub fn from_headers(request_headers: HeaderMap) -> Self {
        Self::from_headers_with_provenance(request_headers, IngressProvenance::Unspecified)
    }

    /// Ingress headers together with who vouched for them.
    #[must_use]
    pub fn from_headers_with_provenance(
        request_headers: HeaderMap,
        provenance: IngressProvenance,
    ) -> Self {
        Self {
            request_headers,
            provenance,
        }
    }

    #[must_use]
    pub fn provenance(&self) -> IngressProvenance {
        self.provenance
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
