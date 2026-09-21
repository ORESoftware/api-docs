//! Canonical context-centric operation API.
//!
//! New authored operations take exactly one `TypedOperationContext<S, O>`.
//! HTTP middleware / Axum adapters and generated `rpc.rs` files populate the
//! same request cache, then the generated `__ores_invoke_*` wrapper runs shared
//! policy and calls the authored operation. No request body argument and no
//! synthetic HTTP request are required.

use std::{future::Future, sync::Arc};

use serde::de::DeserializeOwned;

use crate::{
    invoke_operation_with_policy, ExecutionEnvironmentKind, OperationContext, OperationDescriptor,
    OperationInvokeError, OperationRequestData, OperationRequestError, OperationSpec,
    OperationTransportKind, ServerStreamResult, TypedOperationRequest,
};

#[derive(Clone)]
pub struct TypedOperationContext<S, O: OperationSpec> {
    base: OperationContext<S>,
    request: TypedOperationRequest<O>,
}

impl<S, O: OperationSpec> TypedOperationContext<S, O> {
    #[must_use]
    pub fn new(base: OperationContext<S>, request: OperationRequestData) -> Self {
        Self {
            base,
            request: TypedOperationRequest::new(request),
        }
    }

    #[must_use]
    pub fn state(&self) -> &S {
        self.base.state()
    }

    #[must_use]
    pub fn transport(&self) -> OperationTransportKind {
        self.base.transport()
    }

    /// Where the operation is executing, independent of `transport()`.
    #[must_use]
    pub fn environment(&self) -> ExecutionEnvironmentKind {
        self.base.environment()
    }

    #[must_use]
    pub fn request_data(&self) -> &OperationRequestData {
        self.request.data()
    }

    pub fn path(&self) -> Result<Arc<O::Path>, OperationRequestError> {
        self.request.path()
    }

    pub fn query(&self) -> Result<Arc<O::Query>, OperationRequestError> {
        self.request.query()
    }

    pub fn headers(&self) -> Result<Arc<O::RequestHeaders>, OperationRequestError> {
        self.request.headers()
    }

    /// Normally this is a cheap typed cache lookup because ores-middleware or
    /// generated RPC decoding already populated the body. JSON has a lazy
    /// fallback from cached raw bytes for compatibility; binary codecs use the
    /// generated codec bridge and must populate the typed cache before invoke.
    pub fn body(&self) -> Result<Arc<O::RequestBody>, OperationRequestError>
    where
        O::RequestBody: DeserializeOwned,
    {
        self.request.body()
    }

    #[must_use]
    pub fn policy_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.base.policy_value(key)
    }

    #[must_use]
    pub fn trusted_headers(&self) -> &http::HeaderMap {
        self.base.trusted_headers()
    }

    /// False when no ingress vouched for any header (direct invocation,
    /// in-process call); `trusted_headers()` is empty in that case.
    #[must_use]
    pub fn has_trusted_ingress(&self) -> bool {
        self.base.has_trusted_ingress()
    }

    /// Platform-established caller identity, when the adapter asserted one.
    #[must_use]
    pub fn provider_identity(&self) -> Option<&crate::ProviderIdentity> {
        self.base.provider_identity()
    }

    /// Who vouched for the trusted headers; `None` when nothing did.
    #[must_use]
    pub fn ingress_provenance(&self) -> Option<crate::IngressProvenance> {
        self.base.ingress_provenance()
    }

    #[must_use]
    pub fn into_parts(self) -> (OperationContext<S>, OperationRequestData) {
        (self.base, self.request.data().clone())
    }
}

/// Shared generated invoker for the canonical one-argument unary operation shape.
///
/// Policy sees the normalized semantic request already produced by the same
/// contract that generated the client SDK. The authored operation receives the
/// typed context only after policy admission succeeds. The future output is
/// intentionally expressed through `O` so the compiler couples the handler's
/// success/error types to the generated operation contract.
pub async fn invoke_typed_context_operation<S, O, Invoke, Fut>(
    descriptor: &'static OperationDescriptor,
    context: TypedOperationContext<S, O>,
    invoke: Invoke,
) -> Result<O::ResponseBody, OperationInvokeError<O::Error>>
where
    O: OperationSpec,
    Invoke: FnOnce(TypedOperationContext<S, O>) -> Fut,
    Fut: Future<Output = Result<O::ResponseBody, O::Error>>,
{
    let (base, request) = context.into_parts();
    let policy_input = request.semantic_input();
    invoke_operation_with_policy(
        descriptor,
        base,
        policy_input,
        move |base, _policy_input| async move {
            invoke(TypedOperationContext::<S, O>::new(base, request)).await
        },
    )
    .await
}

/// Shared generated invoker for a canonical `server_stream` operation.
///
/// Policy admission happens before the stream is returned. The authored handler
/// itself returns `ServerStreamResult<O>`; semantic per-item failures remain in
/// that stream while policy/admission failures stay in `OperationInvokeError`.
/// Expressing the future output through `O` makes a unary `Result<...>` handler
/// or a stream with the wrong item/error types fail during `cargo check`.
pub async fn invoke_typed_context_server_stream_operation<S, O, Invoke, Fut>(
    descriptor: &'static OperationDescriptor,
    context: TypedOperationContext<S, O>,
    invoke: Invoke,
) -> Result<ServerStreamResult<O>, OperationInvokeError<O::Error>>
where
    O: OperationSpec,
    Invoke: FnOnce(TypedOperationContext<S, O>) -> Fut,
    Fut: Future<Output = ServerStreamResult<O>>,
{
    let (base, request) = context.into_parts();
    let policy_input = request.semantic_input();
    invoke_operation_with_policy(
        descriptor,
        base,
        policy_input,
        move |base, _policy_input| async move {
            Ok::<ServerStreamResult<O>, O::Error>(
                invoke(TypedOperationContext::<S, O>::new(base, request)).await,
            )
        },
    )
    .await
}
