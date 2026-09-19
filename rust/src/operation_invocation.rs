//! Provider-neutral invocation object for generated operation hosts.
//!
//! Provider adapters may understand AWS/GCP envelopes, but product application
//! libraries should not depend on those adapter crates merely to export a
//! generated dispatcher. This module carries only operation-runtime types:
//! the canonical RPC call, transport, execution environment, trusted ingress
//! and provider-established identity.

use crate::{
    ExecutionEnvironmentKind, OperationContext, ProviderIdentity, RpcV1Call, RpcV1HttpContext,
};

/// Provider-neutral normalized invocation consumed by generated API dispatchers.
///
/// It deliberately owns no application state. The application-library
/// dispatcher supplies its concrete state and turns this into the same
/// [`OperationContext`] used by the ordinary server path.
#[derive(Clone, Debug)]
pub struct OperationInvocation {
    call: RpcV1Call,
    transport: InvocationTransport,
    environment: ExecutionEnvironmentKind,
    trusted_ingress: Option<RpcV1HttpContext>,
    provider_identity: Option<ProviderIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InvocationTransport {
    Http,
    Rpc,
    Event,
}

impl OperationInvocation {
    /// HTTP invocation after provider routing/normalization.
    #[must_use]
    pub fn http(
        call: RpcV1Call,
        environment: ExecutionEnvironmentKind,
        trusted_ingress: Option<RpcV1HttpContext>,
        provider_identity: Option<ProviderIdentity>,
    ) -> Self {
        Self {
            call,
            transport: InvocationTransport::Http,
            environment,
            trusted_ingress,
            provider_identity,
        }
    }

    /// RPC/direct invocation. `trusted_ingress = None` is the ordinary direct
    /// Lambda Invoke case: IAM may authorize the call outside the function, but
    /// the runtime does not expose a header-bearing ingress or caller identity.
    #[must_use]
    pub fn rpc(
        call: RpcV1Call,
        environment: ExecutionEnvironmentKind,
        trusted_ingress: Option<RpcV1HttpContext>,
        provider_identity: Option<ProviderIdentity>,
    ) -> Self {
        Self {
            call,
            transport: InvocationTransport::Rpc,
            environment,
            trusted_ingress,
            provider_identity,
        }
    }

    /// Non-HTTP provider event normalized into an operation call.
    #[must_use]
    pub fn event(
        call: RpcV1Call,
        environment: ExecutionEnvironmentKind,
        provider_identity: Option<ProviderIdentity>,
    ) -> Self {
        Self {
            call,
            transport: InvocationTransport::Event,
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
    pub fn environment(&self) -> ExecutionEnvironmentKind {
        self.environment
    }

    #[must_use]
    pub fn trusted_ingress(&self) -> Option<&RpcV1HttpContext> {
        self.trusted_ingress.as_ref()
    }

    #[must_use]
    pub fn provider_identity(&self) -> Option<&ProviderIdentity> {
        self.provider_identity.as_ref()
    }

    /// Supply concrete application state and recover exactly the pair consumed
    /// by the generated N-way operation dispatcher.
    #[must_use]
    pub fn into_parts<S>(self, state: S) -> (OperationContext<S>, RpcV1Call) {
        let mut context = match (self.transport, self.trusted_ingress) {
            (InvocationTransport::Http, Some(ingress)) => {
                OperationContext::http_from_ingress(state, ingress)
            }
            (InvocationTransport::Http, None) => OperationContext::http(state),
            (InvocationTransport::Rpc, Some(ingress)) => OperationContext::rpc(state, ingress),
            (InvocationTransport::Rpc, None) => OperationContext::rpc_without_ingress(state),
            (InvocationTransport::Event, _) => OperationContext::event(state),
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
    use super::*;
    use crate::{IdentityProvider, IngressProvenance, OperationTransportKind};
    use http::HeaderMap;

    #[test]
    fn http_invocation_preserves_trusted_ingress_environment_and_identity() {
        let ingress = RpcV1HttpContext::from_headers_with_provenance(
            HeaderMap::new(),
            IngressProvenance::ApiGateway,
        );
        let identity = ProviderIdentity::new(IdentityProvider::AwsIam, "arn:aws:iam::1:role/demo");
        let invocation = OperationInvocation::http(
            RpcV1Call::new("call-1", "demo.users.find"),
            ExecutionEnvironmentKind::Lambda,
            Some(ingress),
            Some(identity.clone()),
        );

        let (context, call) = invocation.into_parts(());
        assert_eq!(call.key, "demo.users.find");
        assert_eq!(context.transport(), OperationTransportKind::Http);
        assert_eq!(context.environment(), ExecutionEnvironmentKind::Lambda);
        assert_eq!(context.ingress_provenance(), Some(IngressProvenance::ApiGateway));
        assert_eq!(context.provider_identity(), Some(&identity));
    }

    #[test]
    fn direct_rpc_invocation_has_no_fabricated_ingress_or_identity() {
        let invocation = OperationInvocation::rpc(
            RpcV1Call::new("call-2", "demo.jobs.run"),
            ExecutionEnvironmentKind::Lambda,
            None,
            None,
        );
        let (context, _) = invocation.into_parts(());
        assert_eq!(context.transport(), OperationTransportKind::Rpc);
        assert_eq!(context.environment(), ExecutionEnvironmentKind::Lambda);
        assert!(!context.has_trusted_ingress());
        assert!(context.provider_identity().is_none());
    }

    #[test]
    fn event_invocation_never_invents_header_ingress() {
        let invocation = OperationInvocation::event(
            RpcV1Call::new("call-3", "demo.jobs.consume"),
            ExecutionEnvironmentKind::CloudFunction,
            None,
        );
        let (context, _) = invocation.into_parts(());
        assert_eq!(context.transport(), OperationTransportKind::Event);
        assert_eq!(context.environment(), ExecutionEnvironmentKind::CloudFunction);
        assert!(!context.has_trusted_ingress());
    }
}
