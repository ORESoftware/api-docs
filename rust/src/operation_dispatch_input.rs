//! Cross-host input/state ABI for generated operation dispatchers.
//!
//! This module deliberately knows nothing about AWS, GCP, Axum, or any other
//! provider/server framework. A host adapter proves how an invocation arrived,
//! normalizes it to [`OperationDispatchInput`], and hands that plus a cloneable
//! [`OperationState`] to generated product-library glue.

use std::{
    any::{type_name, Any},
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
};

use thiserror::Error;

use crate::{
    DispatchError, ExecutionEnvironmentKind, OperationContext, OperationTransportKind,
    ProviderIdentity, RpcV1Call, RpcV1HttpContext, RpcV1Receipt,
};

#[derive(Clone)]
pub struct OperationState {
    inner: Arc<dyn Any + Send + Sync>,
}

impl fmt::Debug for OperationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OperationState")
            .field("type_id", &self.inner.type_id())
            .finish_non_exhaustive()
    }
}

impl OperationState {
    #[must_use]
    pub fn new<S>(state: S) -> Self
    where
        S: Clone + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(state),
        }
    }

    pub fn clone_as<S>(&self) -> Result<S, OperationStateError>
    where
        S: Clone + Send + Sync + 'static,
    {
        self.inner
            .downcast_ref::<S>()
            .cloned()
            .ok_or_else(|| OperationStateError::TypeMismatch {
                expected: type_name::<S>(),
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum OperationStateError {
    #[error("operation state does not contain expected type {expected}")]
    TypeMismatch { expected: &'static str },
}

/// Opaque cold-start failure building an API operation state object.
pub type OperationStateInitError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type OperationStateFuture =
    Pin<Box<dyn Future<Output = Result<OperationState, OperationStateInitError>> + Send + 'static>>;
/// Stable ABI exported by an API-server library as `ores_api_lambda_state`.
pub type OperationStateFn = fn() -> OperationStateFuture;

#[derive(Clone, Debug, Error)]
pub enum OperationHostError {
    #[error(transparent)]
    State(#[from] OperationStateError),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

/// Result of one generated product-library dispatch after the provider host has
/// normalized an invocation and supplied the cold-start application state.
pub type OperationDispatchResult = Result<RpcV1Receipt, OperationHostError>;
/// Owned future returned by a generated API-server dispatch trampoline.
pub type OperationDispatchFuture =
    Pin<Box<dyn Future<Output = OperationDispatchResult> + Send + 'static>>;
/// Stable provider-neutral dispatch ABI for generated API Lambda/Cloud Function
/// hosts. The runtime initializes [`OperationState`] through [`OperationStateFn`]
/// and clones the erased state handle into each invocation; product glue recovers
/// its concrete state type before entering the handlers-authoritative dispatcher.
pub type OperationDispatchFn =
    fn(OperationState, OperationDispatchInput) -> OperationDispatchFuture;

#[derive(Clone)]
pub struct OperationDispatchInput {
    call: RpcV1Call,
    transport: OperationTransportKind,
    environment: ExecutionEnvironmentKind,
    trusted_ingress: Option<RpcV1HttpContext>,
    provider_identity: Option<ProviderIdentity>,
}

/// Request envelopes may contain authorization headers, query credentials, or
/// sensitive bodies, so `Debug` deliberately exposes only routing/correlation
/// metadata. `RpcV1HttpContext` already redacts header values in its own Debug
/// implementation; keep using that redacted surface rather than the raw call.
impl fmt::Debug for OperationDispatchInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OperationDispatchInput")
            .field("call_id", &self.call.id)
            .field("operation_key", &self.call.key)
            .field("transport", &self.transport)
            .field("environment", &self.environment)
            .field("trusted_ingress", &self.trusted_ingress)
            .field("provider_identity", &self.provider_identity)
            .finish_non_exhaustive()
    }
}

impl OperationDispatchInput {
    #[must_use]
    pub fn http(
        call: RpcV1Call,
        trusted_ingress: Option<RpcV1HttpContext>,
        environment: ExecutionEnvironmentKind,
        provider_identity: Option<ProviderIdentity>,
    ) -> Self {
        Self {
            call,
            transport: OperationTransportKind::Http,
            environment,
            trusted_ingress,
            provider_identity,
        }
    }

    #[must_use]
    pub fn rpc(
        call: RpcV1Call,
        trusted_ingress: Option<RpcV1HttpContext>,
        environment: ExecutionEnvironmentKind,
        provider_identity: Option<ProviderIdentity>,
    ) -> Self {
        Self {
            call,
            transport: OperationTransportKind::Rpc,
            environment,
            trusted_ingress,
            provider_identity,
        }
    }

    #[must_use]
    pub fn event(
        call: RpcV1Call,
        environment: ExecutionEnvironmentKind,
        provider_identity: Option<ProviderIdentity>,
    ) -> Self {
        Self {
            call,
            transport: OperationTransportKind::Event,
            environment,
            trusted_ingress: None,
            provider_identity,
        }
    }

    #[must_use]
    pub fn call(&self) -> &RpcV1Call {
        &self.call
    }

    #[must_use]
    pub fn transport(&self) -> OperationTransportKind {
        self.transport
    }

    #[must_use]
    pub fn environment(&self) -> ExecutionEnvironmentKind {
        self.environment
    }

    #[must_use]
    pub fn has_trusted_ingress(&self) -> bool {
        self.trusted_ingress.is_some()
    }

    #[must_use]
    pub fn provider_identity(&self) -> Option<&ProviderIdentity> {
        self.provider_identity.as_ref()
    }

    #[must_use]
    pub fn into_parts<S>(self, state: S) -> (OperationContext<S>, RpcV1Call) {
        let mut context = match (self.transport, self.trusted_ingress) {
            (OperationTransportKind::Http, Some(ingress)) => {
                OperationContext::http_from_ingress(state, ingress)
            }
            (OperationTransportKind::Http, None) => OperationContext::http(state),
            (OperationTransportKind::Rpc, Some(ingress)) => OperationContext::rpc(state, ingress),
            (OperationTransportKind::Rpc, None) => OperationContext::rpc_without_ingress(state),
            (OperationTransportKind::Event, _) => OperationContext::event(state),
        }
        .with_environment(self.environment);
        if let Some(identity) = self.provider_identity {
            context = context.with_provider_identity(identity);
        }
        (context, self.call)
    }
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, HeaderValue};

    use super::*;
    use crate::{IdentityProvider, IngressProvenance, OptionalJson};

    #[test]
    fn state_is_erased_across_the_host_boundary_and_recovered_inside_product_code() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct AppState(&'static str);

        let state = OperationState::new(AppState("db"));
        assert_eq!(state.clone_as::<AppState>().unwrap(), AppState("db"));
        assert!(matches!(
            state.clone_as::<String>(),
            Err(OperationStateError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn state_factory_abi_is_type_erased() {
        fn state() -> OperationStateFuture {
            Box::pin(async { Ok(OperationState::new(7_u64)) })
        }
        let _: OperationStateFn = state;
    }

    #[test]
    fn dispatch_abi_carries_erased_state_and_normalized_input() {
        fn dispatch(
            state: OperationState,
            input: OperationDispatchInput,
        ) -> OperationDispatchFuture {
            Box::pin(async move {
                let state = state.clone_as::<u64>()?;
                if state == 7 {
                    Err(DispatchError::unknown_operation(input.call().key.clone()).into())
                } else {
                    unreachable!("fixture state is fixed")
                }
            })
        }

        let _: OperationDispatchFn = dispatch;
        let future = dispatch(
            OperationState::new(7_u64),
            OperationDispatchInput::rpc(
                RpcV1Call::new("call-1", "demo.jobs.run"),
                None,
                ExecutionEnvironmentKind::Lambda,
                None,
            ),
        );
        drop(future);
    }

    #[test]
    fn http_input_preserves_ingress_environment_and_provider_identity() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
        let ingress =
            RpcV1HttpContext::from_headers_with_provenance(headers, IngressProvenance::ApiGateway);
        let identity = ProviderIdentity::new(
            IdentityProvider::AwsIam,
            "arn:aws:iam::111122223333:role/api",
        );
        let mut call = RpcV1Call::new("req-1", "demo.users.find");
        call.body = OptionalJson::present(serde_json::json!({
            "credential": "dispatch-call-secret"
        }));
        let input = OperationDispatchInput::http(
            call,
            Some(ingress),
            ExecutionEnvironmentKind::Lambda,
            Some(identity),
        );

        let debug = format!("{input:?}");
        assert!(debug.contains("req-1"));
        assert!(debug.contains("demo.users.find"));
        assert!(!debug.contains("Bearer secret"));
        assert!(!debug.contains("dispatch-call-secret"));

        let (context, call) = input.into_parts(7_u64);
        assert_eq!(call.key, "demo.users.find");
        assert_eq!(context.transport(), OperationTransportKind::Http);
        assert_eq!(context.environment(), ExecutionEnvironmentKind::Lambda);
        assert!(context.has_trusted_ingress());
        assert_eq!(
            context.ingress_provenance(),
            Some(IngressProvenance::ApiGateway)
        );
        assert_eq!(
            context
                .provider_identity()
                .map(|identity| identity.principal.as_str()),
            Some("arn:aws:iam::111122223333:role/api")
        );
    }

    #[test]
    fn direct_rpc_input_has_no_ingress_by_construction() {
        let input = OperationDispatchInput::rpc(
            RpcV1Call::new("call-1", "demo.jobs.run"),
            None,
            ExecutionEnvironmentKind::Lambda,
            None,
        );
        let (context, _) = input.into_parts(());
        assert_eq!(context.transport(), OperationTransportKind::Rpc);
        assert!(!context.has_trusted_ingress());
    }

    #[test]
    fn host_errors_keep_state_and_dispatch_failures_distinct() {
        let state: OperationHostError =
            OperationStateError::TypeMismatch { expected: "State" }.into();
        assert!(matches!(state, OperationHostError::State(_)));
        let dispatch: OperationHostError = DispatchError::unknown_operation("demo.nope").into();
        assert!(matches!(dispatch, OperationHostError::Dispatch(_)));
    }
}
