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
    pub input: &'a Value,
}

#[non_exhaustive]
pub struct OperationPolicyOutcome<'a> {
    pub operation: &'a OperationDescriptor,
    pub transport: OperationTransportKind,
    pub environment: ExecutionEnvironmentKind,
    pub ok: bool,
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

    fn after<'a>(&'a self, outcome: OperationPolicyOutcome<'a>) -> OperationPolicyFuture<'a, ()>;
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
