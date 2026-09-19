//! Transport-neutral policy hooks executed by the generated shared-operation invoker.
//!
//! HTTP and RPC adapters may differ in framing, but auth/RBAC/tenancy/rate-limit/
//! idempotency/tracing/audit policy must run at the same `__ores_invoke_*` boundary.
//! Product servers install one `OperationPolicy` on `OperationContext`; generated
//! invokers call it before and after the authored operation.

use std::{collections::BTreeMap, future::Future, pin::Pin};

use http::HeaderMap;
use serde::Serialize;
use serde_json::Value;

use crate::{
    operation_runtime::{ExecutionEnvironmentKind, OperationTransportKind},
    rpc_http_context::IngressProvenance,
    RpcStreamMode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationDescriptor {
    pub key: &'static str,
    pub codecs: &'static [&'static str],
    pub default_codec: &'static str,
    pub audiences: &'static [&'static str],
    pub scope: &'static str,
    pub stream: RpcStreamMode,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OperationPolicyPermit {
    /// Policy-derived identity/tenant/authorization facts exposed to the
    /// authored operation through `OperationContext::policy_value`.
    pub values: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationPolicyRejection {
    pub status: u16,
    pub code: String,
    pub message: String,
}

impl OperationPolicyRejection {
    #[must_use]
    pub fn new(status: u16, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Which system authenticated a [`ProviderIdentity`].
///
/// `#[non_exhaustive]`: match with a wildcard arm that refuses.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdentityProvider {
    /// AWS IAM, as reported by the Lambda / API Gateway request context.
    AwsIam,
    /// Google Cloud IAM.
    GcpIam,
    /// A workload identity issued by the deployment platform (for example a
    /// Kubernetes service account or SPIFFE ID).
    Workload,
    /// A test harness. Never admissible as a production identity source.
    Test,
}

/// A caller identity established by the *platform*, not by a request header.
///
/// This is the identity seam for invocations that have no header-bearing
/// ingress. A direct AWS Lambda invocation is authenticated by IAM before the
/// function runs; the adapter reads the principal from the provider's request
/// context and hands it over here. Policies authorize on it exactly as they
/// would on header-derived identity, through one shared contract -- so no
/// adapter needs an IAM-specific side channel or closure-captured state.
///
/// It is deliberately typed and not a header map: an adapter must *assert* an
/// identity, it cannot forward one. The values are identifiers (an ARN, an
/// account id), not credentials, so `Debug` prints them.
///
/// `#[non_exhaustive]`: build it with [`ProviderIdentity::new`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderIdentity {
    pub provider: IdentityProvider,
    /// Provider-scoped principal identifier, for example an IAM role ARN.
    pub principal: String,
    /// Owning account / project, when the provider reports one.
    pub account: Option<String>,
    /// The resource that made the call on the principal's behalf, for example
    /// the source ARN of an event-source mapping.
    pub source: Option<String>,
}

impl ProviderIdentity {
    #[must_use]
    pub fn new(provider: IdentityProvider, principal: impl Into<String>) -> Self {
        Self {
            provider,
            principal: principal.into(),
            account: None,
            source: None,
        }
    }

    #[must_use]
    pub fn with_account(mut self, account: impl Into<String>) -> Self {
        self.account = Some(account.into());
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// How an invocation ended, as far as policy and audit are concerned.
///
/// `#[non_exhaustive]`: finer classes are expected (for example timeout or
/// cancellation). Treat an unknown class as a failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperationOutcomeKind {
    /// Policy admitted the call and the authored operation returned `Ok`.
    Success,
    /// Policy admitted the call and the authored operation returned `Err`.
    OperationError,
    /// `before()` rejected the call; the authored operation never ran.
    PolicyRejected,
}

/// What a policy sees before the authored operation runs.
///
/// `#[non_exhaustive]`: only the operation runtime constructs this, and new
/// facts (as `environment` and `has_trusted_ingress` were) must be addable
/// without a source break in every product policy.
#[non_exhaustive]
pub struct OperationPolicyRequest<'a> {
    pub operation: &'a OperationDescriptor,
    pub transport: OperationTransportKind,
    /// Where the operation is executing. Orthogonal to `transport`: an AWS
    /// Lambda serving API Gateway reports `Http` + `Lambda`.
    pub environment: ExecutionEnvironmentKind,
    /// Headers the ingress vouched for. Empty when `has_trusted_ingress` is
    /// false.
    pub trusted_headers: &'a HeaderMap,
    /// False when no ingress vouched for any header at all (direct Lambda
    /// invocation, in-process call). Distinguishes that case from an ingress
    /// that forwarded an empty header set, so a policy can refuse to treat
    /// "no proxy" as "proxy sent nothing".
    pub has_trusted_ingress: bool,
    /// Who vouched for `trusted_headers`; `None` exactly when
    /// `has_trusted_ingress` is false. `Some(Unspecified)` means an adapter
    /// supplied headers without saying where they came from -- the weakest
    /// claim, and what every pre-existing constructor produces.
    pub ingress_provenance: Option<IngressProvenance>,
    /// Platform-established caller identity, when the adapter asserted one.
    /// Independent of `trusted_headers`: a direct invocation has this and no
    /// ingress; an API Gateway call with IAM auth can have both.
    pub provider_identity: Option<&'a ProviderIdentity>,
    pub input: &'a Value,
}

/// What a policy sees once an invocation has ended.
///
/// Carries everything an audit record needs, so a policy does not have to
/// stash request facts in `before()` and correlate them later.
#[non_exhaustive]
pub struct OperationPolicyOutcome<'a> {
    pub operation: &'a OperationDescriptor,
    pub transport: OperationTransportKind,
    pub environment: ExecutionEnvironmentKind,
    /// `true` exactly when `outcome` is [`OperationOutcomeKind::Success`].
    /// Retained for policies written before `outcome` existed.
    pub ok: bool,
    pub outcome: OperationOutcomeKind,
    pub ingress_provenance: Option<IngressProvenance>,
    pub provider_identity: Option<&'a ProviderIdentity>,
    /// The values this policy's own `before()` returned in its permit: the
    /// admitted identity / tenant / authorization facts. Empty when the call
    /// was rejected.
    pub permit_values: &'a BTreeMap<String, Value>,
    /// The rejection `before()` returned; `Some` exactly when `outcome` is
    /// [`OperationOutcomeKind::PolicyRejected`].
    pub rejection: Option<&'a OperationPolicyRejection>,
}

pub type OperationPolicyFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Product policy engine shared by HTTP and RPC invocation.
///
/// Implementations typically compose authentication, RBAC, tenancy, rate
/// limiting, idempotency, tracing and audit. Transport-only middleware such as
/// CORS/compression remains outside this hook.
pub trait OperationPolicy: Send + Sync + 'static {
    fn before<'a>(
        &'a self,
        request: OperationPolicyRequest<'a>,
    ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>>;

    /// Runs after an **admitted** invocation, whether the authored operation
    /// succeeded or failed. It is *not* called when `before()` rejected: a
    /// policy that acquires something in `before()` (a rate-limit token, an
    /// idempotency lease) and releases it here must not see a release without a
    /// matching acquire.
    fn after<'a>(&'a self, outcome: OperationPolicyOutcome<'a>) -> OperationPolicyFuture<'a, ()>;

    /// Runs when `before()` rejected the invocation, so that denials are
    /// auditable on the same terms as admitted calls. `outcome.outcome` is
    /// [`OperationOutcomeKind::PolicyRejected`] and `outcome.rejection` is set.
    ///
    /// Defaults to doing nothing, which is exactly the behaviour every policy
    /// had before this hook existed. It cannot change the rejection.
    fn after_rejection<'a>(
        &'a self,
        _outcome: OperationPolicyOutcome<'a>,
    ) -> OperationPolicyFuture<'a, ()> {
        Box::pin(async {})
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAllOperationPolicy;

impl OperationPolicy for AllowAllOperationPolicy {
    fn before<'a>(
        &'a self,
        _request: OperationPolicyRequest<'a>,
    ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>> {
        Box::pin(async { Ok(OperationPolicyPermit::default()) })
    }

    fn after<'a>(&'a self, _outcome: OperationPolicyOutcome<'a>) -> OperationPolicyFuture<'a, ()> {
        Box::pin(async {})
    }
}
