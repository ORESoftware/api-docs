//! Cross-host input/state ABI for generated operation dispatchers.
//!
//! This module deliberately knows nothing about AWS, GCP, Axum, or any other
//! provider/server framework. A host adapter proves how an invocation arrived,
//! normalizes it to [`OperationDispatchInput`], and hands that plus a cloneable
//! [`OperationState`] to generated product-library glue.
//!
//! The generated route dispatcher then recovers its concrete application state
//! inside the product crate and calls the same typed operation invokers used by
//! the long-lived server. This is the API-server analogue of the web-page
//! `PageState`/public-hidden trampoline boundary: a separate Lambda bin never
//! needs the product's private `AppState` type in its public signature.

use std::{
    any::{type_name, Any},
    fmt,
    sync::Arc,
};

use thiserror::Error;

use crate::{
    ExecutionEnvironmentKind, OperationContext, OperationTransportKind, ProviderIdentity,
    RpcV1Call, RpcV1HttpContext,
};

/// Cloneable type-erased application state for a generated operation host.
///
/// The owning application constructs this at cold start with [`Self::new`]. A
/// generated dispatcher recovers the concrete state with [`Self::clone_as`]
/// inside the product crate, so a separate provider bin never has to expose or
/// name `AppState`.
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

    /// Recover a clone of the concrete application state expected by one route
    /// dispatcher. Failure is a build/deployment contract bug, not caller input.
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

/// Provider-neutral invocation normalized far enough that the generated
/// operation dispatcher only needs to supply its concrete application state.
///
/// HTTP adapters resolve `(method, path)` to an operation key before creating
/// this value. RPC adapters already have the key. Both therefore converge on
/// the same `RpcV1Call` and generated key switch.
#[derive(Clone, Debug)]
pub struct OperationDispatchInput {
    call: RpcV1Call,
    transport: OperationTransportKind,
    environment: ExecutionEnvironmentKind,
    trusted_ingress: Option<RpcV1HttpContext>,
    provider_identity: Option<ProviderIdentity>,
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

    /// Attach concrete application state after ingress normalization.
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
            // OperationTransportKind is non-exhaustive. A future transport must
            // get an explicit constructor here rather than silently inheriting
            // the trust semantics of an existing carrier.
            _ => OperationContext::event(state),
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
    use crate::{IdentityProvider, IngressProvenance};

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
    fn http_input_preserves_ingress_environment_and_provider_identity() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
        let ingress = RpcV1HttpContext::from_headers_with_provenance(
            headers,
            IngressProvenance::ApiGateway,
        );
        let identity = ProviderIdentity::new(
            IdentityProvider::AwsIam,
            "arn:aws:iam::111122223333:role/api",
        );
        let call = RpcV1Call::new("req-1", "demo.users.find");
        let input = OperationDispatchInput::http(
            call,
            Some(ingress),
            ExecutionEnvironmentKind::Lambda,
            Some(identity),
        );
        let (context, call) = input.into_parts(7_u64);
        assert_eq!(call.key, "demo.users.find");
        assert_eq!(context.transport(), OperationTransportKind::Http);
        assert_eq!(context.environment(), ExecutionEnvironmentKind::Lambda);
        assert!(context.has_trusted_ingress());
        assert_eq!(context.ingress_provenance(), Some(IngressProvenance::ApiGateway));
        assert_eq!(
            context.provider_identity().map(|identity| identity.principal.as_str()),
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
}
