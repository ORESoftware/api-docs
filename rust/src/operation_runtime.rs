//! Typed runtime boundary shared by ordinary HTTP adapters and `/v1/rpc`.
//!
//! The canonical authored operation now receives one typed operation context.
//! The two-argument `OperationContext<S> + Input` helper remains available for
//! migration compatibility and is also used internally by the typed wrapper to
//! execute the shared policy boundary. RPC never creates a synthetic REST
//! request.
//!
//! This module is part of the `operation-runtime` feature and must not depend
//! on Axum, Tower, or any other server framework: serverless adapters link it
//! on its own.

use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, OnceLock},
};

use http::HeaderMap;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    operation_policy::{
        OperationDescriptor, OperationOutcomeKind, OperationPolicy, OperationPolicyOutcome,
        OperationPolicyRejection, OperationPolicyRequest, ProviderIdentity,
    },
    rpc_http_context::IngressProvenance,
    OptionalJson, RpcV1Call, RpcV1HttpContext, RpcV1Receipt,
};

/// How the invocation reached the operation.
///
/// This is deliberately *not* where an execution environment such as AWS
/// Lambda is recorded: a Lambda can be reached over HTTP, by an RPC envelope,
/// or by a queue event. See [`ExecutionEnvironmentKind`].
///
/// `#[non_exhaustive]`: further carriers are expected, and a downstream
/// exhaustive `match` must not turn each one into an ecosystem-wide source
/// break. Generated code that matches on this must map the wildcard arm to a
/// hard error, never to a silent default.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperationTransportKind {
    Http,
    Rpc,
    /// A provider event that is neither an HTTP request nor an RPC envelope
    /// (queue record, bus event, object notification).
    Event,
}

/// Where the operation is executing, independent of how it was reached.
///
/// An AWS Lambda behind API Gateway reports `transport = Http`,
/// `environment = Lambda`; the same function invoked directly with an RPC
/// envelope reports `transport = Rpc`, `environment = Lambda`. Policies use
/// this to decide, for example, that a capability needing process-global
/// correctness state is not admissible in `Lambda` without external storage.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExecutionEnvironmentKind {
    /// A long-lived server process. The default, so every constructor that
    /// predates this enum keeps its meaning.
    #[default]
    Server,
    Lambda,
    Worker,
    Cli,
    Test,
}

/// Transport-neutral base context. The canonical typed view is
/// `TypedOperationContext<S, O>`.
#[derive(Clone)]
pub struct OperationContext<S> {
    state: S,
    transport: OperationTransportKind,
    environment: ExecutionEnvironmentKind,
    /// The single source of truth for ingress-vouched headers. `None` means no
    /// ingress vouched for anything, which is not the same as `Some` of an
    /// empty map. `trusted_headers()` and `rpc_http_context()` are both views
    /// of this one value, so they cannot diverge.
    trusted_ingress: Option<RpcV1HttpContext>,
    /// Platform-established caller identity, asserted by the adapter. Separate
    /// from `trusted_ingress`: a direct invocation has this and no ingress.
    provider_identity: Option<ProviderIdentity>,
    policy: Option<Arc<dyn OperationPolicy>>,
    policy_values: BTreeMap<String, Value>,
}

impl<S: std::fmt::Debug> std::fmt::Debug for OperationContext<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationContext")
            .field("state", &self.state)
            .field("transport", &self.transport)
            .field("environment", &self.environment)
            // Values are never printed: trusted headers carry credentials
            // (authorization, cookies, access JWTs, request signatures) and
            // policy values carry identity claims. `{:?}` on a context is an
            // easy thing to log by accident, especially from an adapter.
            .field("trusted_ingress", &self.trusted_ingress)
            .field("provider_identity", &self.provider_identity)
            .field("has_policy", &self.policy.is_some())
            .field(
                "policy_value_keys",
                &self.policy_values.keys().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

fn empty_headers() -> &'static HeaderMap {
    static EMPTY: OnceLock<HeaderMap> = OnceLock::new();
    EMPTY.get_or_init(HeaderMap::new)
}

impl<S> OperationContext<S> {
    fn new(
        state: S,
        transport: OperationTransportKind,
        trusted_ingress: Option<RpcV1HttpContext>,
    ) -> Self {
        Self {
            state,
            transport,
            environment: ExecutionEnvironmentKind::default(),
            trusted_ingress,
            provider_identity: None,
            policy: None,
            policy_values: BTreeMap::new(),
        }
    }

    /// HTTP invocation with no ingress-vouched headers.
    #[must_use]
    pub fn http(state: S) -> Self {
        Self::new(state, OperationTransportKind::Http, None)
    }

    #[must_use]
    pub fn http_with_headers(state: S, trusted_headers: HeaderMap) -> Self {
        Self::new(
            state,
            OperationTransportKind::Http,
            Some(RpcV1HttpContext::from_headers(trusted_headers)),
        )
    }

    /// HTTP invocation whose ingress is named. New adapters use this rather
    /// than `http_with_headers`, so a policy can tell API Gateway from a test
    /// harness instead of seeing an undifferentiated "trusted".
    #[must_use]
    pub fn http_from_ingress(state: S, ingress: RpcV1HttpContext) -> Self {
        Self::new(state, OperationTransportKind::Http, Some(ingress))
    }

    #[must_use]
    pub fn rpc(state: S, rpc_http_context: RpcV1HttpContext) -> Self {
        Self::new(state, OperationTransportKind::Rpc, Some(rpc_http_context))
    }

    /// RPC envelope that did not arrive through a header-bearing ingress, for
    /// example a direct AWS Lambda invocation. Caller identity on that path is
    /// the provider's (IAM), never a header, so no trusted ingress exists.
    #[must_use]
    pub fn rpc_without_ingress(state: S) -> Self {
        Self::new(state, OperationTransportKind::Rpc, None)
    }

    /// Provider event that is neither HTTP nor an RPC envelope.
    #[must_use]
    pub fn event(state: S) -> Self {
        Self::new(state, OperationTransportKind::Event, None)
    }

    #[must_use]
    pub fn with_policy(mut self, policy: Arc<dyn OperationPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Replace the ingress-vouched headers. Because there is one source of
    /// truth, `rpc_http_context()` observes the replacement too.
    #[must_use]
    pub fn with_trusted_headers(mut self, trusted_headers: HeaderMap) -> Self {
        self.trusted_ingress = Some(RpcV1HttpContext::from_headers(trusted_headers));
        self
    }

    /// Declare that no ingress vouched for any header.
    #[must_use]
    pub fn without_trusted_ingress(mut self) -> Self {
        self.trusted_ingress = None;
        self
    }

    /// Assert the platform-established caller identity (for example the IAM
    /// principal from a Lambda request context). An adapter must assert it from
    /// the provider's own context -- never copy it out of a request header or
    /// an envelope field the caller controls.
    #[must_use]
    pub fn with_provider_identity(mut self, identity: ProviderIdentity) -> Self {
        self.provider_identity = Some(identity);
        self
    }

    #[must_use]
    pub fn with_environment(mut self, environment: ExecutionEnvironmentKind) -> Self {
        self.environment = environment;
        self
    }

    #[must_use]
    pub fn state(&self) -> &S {
        &self.state
    }

    #[must_use]
    pub fn transport(&self) -> OperationTransportKind {
        self.transport
    }

    #[must_use]
    pub fn environment(&self) -> ExecutionEnvironmentKind {
        self.environment
    }

    /// Compatibility view of the trusted ingress for RPC invocations.
    ///
    /// Retained so existing dispatchers keep compiling; new code should use
    /// [`Self::trusted_ingress`] or [`Self::trusted_headers`], which behave the
    /// same for every transport. Returns `None` for non-RPC transports, as it
    /// always has.
    #[must_use]
    pub fn rpc_http_context(&self) -> Option<&RpcV1HttpContext> {
        match self.transport {
            OperationTransportKind::Rpc => self.trusted_ingress.as_ref(),
            _ => None,
        }
    }

    /// Headers the ingress vouched for, or `None` when nothing vouched for any.
    #[must_use]
    pub fn trusted_ingress(&self) -> Option<&HeaderMap> {
        self.trusted_ingress
            .as_ref()
            .map(RpcV1HttpContext::request_headers)
    }

    #[must_use]
    pub fn has_trusted_ingress(&self) -> bool {
        self.trusted_ingress.is_some()
    }

    /// Platform-established caller identity, when the adapter asserted one.
    #[must_use]
    pub fn provider_identity(&self) -> Option<&ProviderIdentity> {
        self.provider_identity.as_ref()
    }

    /// Who vouched for the trusted headers; `None` when nothing did.
    #[must_use]
    pub fn ingress_provenance(&self) -> Option<IngressProvenance> {
        self.trusted_ingress
            .as_ref()
            .map(RpcV1HttpContext::provenance)
    }

    /// Headers the ingress vouched for; empty when there is no trusted ingress.
    /// Use [`Self::has_trusted_ingress`] to tell those two cases apart.
    #[must_use]
    pub fn trusted_headers(&self) -> &HeaderMap {
        self.trusted_ingress().unwrap_or_else(|| empty_headers())
    }

    #[must_use]
    pub fn policy_value(&self, key: &str) -> Option<&Value> {
        self.policy_values.get(key)
    }

    #[must_use]
    pub fn into_state(self) -> S {
        self.state
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum OperationInvokeError<E> {
    Policy(OperationPolicyRejection),
    PolicyInputEncode { message: String },
    Operation(E),
}

/// Shared policy composition point. Generated `__ores_invoke_*` wrappers call
/// this helper directly or through `invoke_typed_context_operation`.
pub async fn invoke_operation_with_policy<S, Input, Success, Failure, Invoke, Fut>(
    operation: &'static OperationDescriptor,
    mut context: OperationContext<S>,
    input: Input,
    invoke: Invoke,
) -> Result<Success, OperationInvokeError<Failure>>
where
    Input: Serialize,
    Invoke: FnOnce(OperationContext<S>, Input) -> Fut,
    Fut: Future<Output = Result<Success, Failure>>,
{
    let input_value =
        serde_json::to_value(&input).map_err(|error| OperationInvokeError::PolicyInputEncode {
            message: error.to_string(),
        })?;
    let transport = context.transport;
    let environment = context.environment;
    let ingress_provenance = context.ingress_provenance();
    // Owned copies for the post-hooks: `context` is moved into the operation.
    let provider_identity = context.provider_identity.clone();

    let policy = context.policy.clone();
    let mut permit_values = BTreeMap::new();
    if let Some(policy) = policy.as_ref() {
        let admission = policy
            .before(OperationPolicyRequest {
                operation,
                transport,
                environment,
                trusted_headers: context.trusted_headers(),
                has_trusted_ingress: context.has_trusted_ingress(),
                ingress_provenance,
                provider_identity: provider_identity.as_ref(),
                input: &input_value,
            })
            .await;
        match admission {
            Ok(permit) => {
                permit_values.clone_from(&permit.values);
                context.policy_values = permit.values;
            }
            Err(rejection) => {
                // Denials are auditable on the same terms as admitted calls,
                // through a separate hook so `after()` keeps meaning "an
                // admitted invocation ended".
                policy
                    .after_rejection(OperationPolicyOutcome {
                        operation,
                        transport,
                        environment,
                        ok: false,
                        outcome: OperationOutcomeKind::PolicyRejected,
                        ingress_provenance,
                        provider_identity: provider_identity.as_ref(),
                        permit_values: &permit_values,
                        rejection: Some(&rejection),
                    })
                    .await;
                return Err(OperationInvokeError::Policy(rejection));
            }
        }
    }

    let result = invoke(context, input).await;

    if let Some(policy) = policy.as_ref() {
        policy
            .after(OperationPolicyOutcome {
                operation,
                transport,
                environment,
                ok: result.is_ok(),
                outcome: if result.is_ok() {
                    OperationOutcomeKind::Success
                } else {
                    OperationOutcomeKind::OperationError
                },
                ingress_provenance,
                provider_identity: provider_identity.as_ref(),
                permit_values: &permit_values,
                rejection: None,
            })
            .await;
    }

    result.map_err(OperationInvokeError::Operation)
}

#[derive(Debug, Error)]
pub enum RpcV1OperationAdapterError {
    #[error("typed RPC input decode failed: {0}")]
    InputDecode(serde_json::Error),
    #[error("typed RPC success encode failed: {0}")]
    SuccessEncode(serde_json::Error),
    #[error("typed RPC error encode failed: {0}")]
    ErrorEncode(serde_json::Error),
}

/// Decode the compatibility JSON envelope into one generated input struct.
/// New context-centric generated `rpc.rs` code instead populates
/// `OperationRequestData` section-by-section so middleware-decoded values can be
/// reused without re-deserialization.
pub fn decode_rpc_operation_input<T>(call: &RpcV1Call) -> Result<T, RpcV1OperationAdapterError>
where
    T: DeserializeOwned,
{
    let mut input = Map::new();
    if let Some(path) = &call.path {
        input.insert("path".into(), Value::Object(path.clone()));
    }
    if let Some(query) = &call.query {
        input.insert("query".into(), Value::Object(query.clone()));
    }
    if let Some(headers) = &call.headers {
        input.insert("headers".into(), Value::Object(headers.clone()));
    }
    if let Some(body) = call.body.value() {
        input.insert("body".into(), body.clone());
    }
    serde_json::from_value(Value::Object(input)).map_err(RpcV1OperationAdapterError::InputDecode)
}

/// Compatibility semantic adapter for JSON-only routes.
pub async fn invoke_shared_rpc_operation<Ctx, Input, Success, Failure, Invoke, Fut>(
    context: Ctx,
    call: RpcV1Call,
    invoke: Invoke,
) -> RpcV1Receipt
where
    Input: DeserializeOwned,
    Success: Serialize,
    Failure: Serialize,
    Invoke: FnOnce(Ctx, Input) -> Fut,
    Fut: Future<Output = Result<Success, Failure>>,
{
    let input = match decode_rpc_operation_input::<Input>(&call) {
        Ok(input) => input,
        Err(error) => {
            return adapter_failure(&call, 400, "rpc_input_decode_failed", error.to_string())
        }
    };

    match invoke(context, input).await {
        Ok(output) => match serde_json::to_value(output) {
            Ok(value) => {
                let mut receipt =
                    RpcV1Receipt::success(call.id, call.key, OptionalJson::present(value));
                receipt.status = Some(200);
                receipt.trace_id = call.trace_id;
                receipt.span_id = call.span_id;
                receipt
            }
            Err(error) => adapter_failure(
                &call,
                500,
                "rpc_success_encode_failed",
                RpcV1OperationAdapterError::SuccessEncode(error).to_string(),
            ),
        },
        Err(error) => match serde_json::to_value(error) {
            Ok(value) => {
                let mut object = match value {
                    Value::Object(object) => object,
                    other => {
                        let mut object = Map::new();
                        object.insert("detail".into(), other);
                        object
                    }
                };
                object
                    .entry("code".to_owned())
                    .or_insert_with(|| Value::String("operation_error".into()));
                let mut receipt = RpcV1Receipt::failure(call.id, call.key, 500, object);
                receipt.trace_id = call.trace_id;
                receipt.span_id = call.span_id;
                receipt
            }
            Err(error) => adapter_failure(
                &call,
                500,
                "rpc_error_encode_failed",
                RpcV1OperationAdapterError::ErrorEncode(error).to_string(),
            ),
        },
    }
}

fn adapter_failure(call: &RpcV1Call, status: u16, code: &str, message: String) -> RpcV1Receipt {
    let mut error = Map::new();
    error.insert("code".into(), Value::String(code.to_owned()));
    error.insert("message".into(), Value::String(message));
    let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), status, error);
    receipt.trace_id = call.trace_id.clone();
    receipt.span_id = call.span_id.clone();
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_policy::{
        IdentityProvider, OperationPolicyFuture, OperationPolicyPermit, OperationPolicyRequest,
    };
    use crate::RpcStreamMode;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct PathInput {
        user_id: String,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct HeadersInput {
        #[serde(rename = "if-none-match")]
        if_none_match: Option<String>,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct FindUserInput {
        path: PathInput,
        headers: HeadersInput,
    }

    #[derive(Debug, Serialize)]
    struct FindUserOutput {
        user_id: String,
    }

    #[derive(Debug, Serialize)]
    struct FindUserError {
        code: String,
    }

    #[test]
    fn semantic_sections_decode_into_one_typed_input() {
        let mut call = RpcV1Call::new("call-1", "demo.users.find_user");
        call.path = Some(Map::from_iter([(
            "user_id".to_owned(),
            Value::String("u-1".into()),
        )]));
        call.headers = Some(Map::from_iter([(
            "if-none-match".to_owned(),
            Value::String("etag-1".into()),
        )]));
        let input = decode_rpc_operation_input::<FindUserInput>(&call).expect("typed input");
        assert_eq!(input.path.user_id, "u-1");
        assert_eq!(input.headers.if_none_match.as_deref(), Some("etag-1"));
    }

    #[tokio::test]
    async fn direct_invocation_never_needs_an_http_request() {
        let mut call = RpcV1Call::new("call-2", "demo.users.find_user");
        call.path = Some(Map::from_iter([(
            "user_id".to_owned(),
            Value::String("u-2".into()),
        )]));
        call.headers = Some(Map::new());

        let receipt =
            invoke_shared_rpc_operation((), call, |(), input: FindUserInput| async move {
                Ok::<FindUserOutput, FindUserError>(FindUserOutput {
                    user_id: input.path.user_id,
                })
            })
            .await;
        assert!(receipt.ok);
        assert_eq!(receipt.status, Some(200));
    }

    struct CountingPolicy {
        before: AtomicUsize,
        after: AtomicUsize,
        last_transport: std::sync::Mutex<Option<OperationTransportKind>>,
    }

    impl OperationPolicy for CountingPolicy {
        fn before<'a>(
            &'a self,
            request: OperationPolicyRequest<'a>,
        ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>>
        {
            self.before.fetch_add(1, Ordering::SeqCst);
            *self.last_transport.lock().expect("transport lock") = Some(request.transport);
            Box::pin(async { Ok(OperationPolicyPermit::default()) })
        }

        fn after<'a>(
            &'a self,
            outcome: OperationPolicyOutcome<'a>,
        ) -> OperationPolicyFuture<'a, ()> {
            self.after.fetch_add(1, Ordering::SeqCst);
            *self.last_transport.lock().expect("transport lock") = Some(outcome.transport);
            Box::pin(async {})
        }
    }

    #[tokio::test]
    async fn policy_wraps_the_authored_operation_once_and_preserves_transport() {
        static DESCRIPTOR: OperationDescriptor = OperationDescriptor {
            key: "demo.users.find_user",
            codecs: &["json"],
            default_codec: "json",
            audiences: &["server"],
            scope: "regular",
            stream: RpcStreamMode::Unary,
        };
        let policy = Arc::new(CountingPolicy {
            before: AtomicUsize::new(0),
            after: AtomicUsize::new(0),
            last_transport: std::sync::Mutex::new(None),
        });
        let context = OperationContext::http(()).with_policy(policy.clone());
        let input = FindUserInput {
            path: PathInput {
                user_id: "u-3".into(),
            },
            headers: HeadersInput {
                if_none_match: None,
            },
        };
        let result =
            invoke_operation_with_policy(&DESCRIPTOR, context, input, |_, input| async move {
                Ok::<_, FindUserError>(FindUserOutput {
                    user_id: input.path.user_id,
                })
            })
            .await
            .expect("operation result");
        assert_eq!(result.user_id, "u-3");
        assert_eq!(policy.before.load(Ordering::SeqCst), 1);
        assert_eq!(policy.after.load(Ordering::SeqCst), 1);
        assert_eq!(
            *policy.last_transport.lock().expect("transport lock"),
            Some(OperationTransportKind::Http)
        );
    }

    #[test]
    fn existing_constructors_default_to_the_server_environment() {
        assert_eq!(
            OperationContext::http(()).environment(),
            ExecutionEnvironmentKind::Server
        );
        assert_eq!(
            OperationContext::rpc((), RpcV1HttpContext::from_headers(HeaderMap::new()))
                .environment(),
            ExecutionEnvironmentKind::Server
        );
        assert_eq!(
            ExecutionEnvironmentKind::default(),
            ExecutionEnvironmentKind::Server
        );
        let lambda = OperationContext::http(()).with_environment(ExecutionEnvironmentKind::Lambda);
        assert_eq!(lambda.environment(), ExecutionEnvironmentKind::Lambda);
        // Environment never rewrites transport.
        assert_eq!(lambda.transport(), OperationTransportKind::Http);
    }

    #[test]
    fn no_ingress_is_distinguishable_from_an_empty_vouched_header_set() {
        let none = OperationContext::http(());
        assert!(!none.has_trusted_ingress());
        assert!(none.trusted_ingress().is_none());
        assert!(none.trusted_headers().is_empty());

        let empty = OperationContext::http_with_headers((), HeaderMap::new());
        assert!(empty.has_trusted_ingress());
        assert!(empty.trusted_ingress().is_some());
        assert!(empty.trusted_headers().is_empty());

        let cleared = empty.without_trusted_ingress();
        assert!(!cleared.has_trusted_ingress());

        assert!(!OperationContext::rpc_without_ingress(()).has_trusted_ingress());
        let event = OperationContext::event(());
        assert_eq!(event.transport(), OperationTransportKind::Event);
        assert!(!event.has_trusted_ingress());
    }

    /// Regression for the duplicated state this replaced: `rpc()` used to copy
    /// the ingress headers into a second field, so replacing the trusted
    /// headers left `rpc_http_context()` reporting the stale originals.
    #[test]
    fn trusted_headers_and_rpc_http_context_cannot_diverge() {
        let mut original = HeaderMap::new();
        original.insert("x-real-ip", http::HeaderValue::from_static("198.51.100.1"));
        let context = OperationContext::rpc((), RpcV1HttpContext::from_headers(original));
        assert_eq!(
            context
                .rpc_http_context()
                .expect("rpc context")
                .request_headers(),
            context.trusted_headers()
        );

        let mut replaced = HeaderMap::new();
        replaced.insert("x-real-ip", http::HeaderValue::from_static("203.0.113.7"));
        let context = context.with_trusted_headers(replaced.clone());
        assert_eq!(context.trusted_headers(), &replaced);
        assert_eq!(
            context
                .rpc_http_context()
                .expect("rpc context")
                .request_headers(),
            &replaced
        );
    }

    #[test]
    fn rpc_http_context_stays_none_for_non_rpc_transports() {
        let context = OperationContext::http_with_headers((), HeaderMap::new());
        assert!(context.rpc_http_context().is_none());
        assert!(context.has_trusted_ingress());
    }

    #[test]
    fn ingress_provenance_is_carried_and_never_outlives_the_headers_it_described() {
        // Nothing vouched: no provenance at all.
        assert_eq!(OperationContext::http(()).ingress_provenance(), None);
        assert_eq!(
            OperationContext::rpc_without_ingress(()).ingress_provenance(),
            None
        );

        // Constructors that predate the enum make the weakest claim, not a
        // strong one by accident.
        assert_eq!(
            OperationContext::http_with_headers((), HeaderMap::new()).ingress_provenance(),
            Some(IngressProvenance::Unspecified)
        );
        assert_eq!(
            OperationContext::rpc((), RpcV1HttpContext::from_headers(HeaderMap::new()))
                .ingress_provenance(),
            Some(IngressProvenance::Unspecified)
        );
        assert_eq!(IngressProvenance::default(), IngressProvenance::Unspecified);

        // A new adapter names its ingress.
        let named = OperationContext::http_from_ingress(
            (),
            RpcV1HttpContext::from_headers_with_provenance(
                HeaderMap::new(),
                IngressProvenance::ApiGateway,
            ),
        );
        assert_eq!(named.transport(), OperationTransportKind::Http);
        assert_eq!(
            named.ingress_provenance(),
            Some(IngressProvenance::ApiGateway)
        );

        // Replacing the headers must not keep vouching for them as API Gateway:
        // whoever swapped them in did not say where they came from.
        let replaced = named.with_trusted_headers(HeaderMap::new());
        assert_eq!(
            replaced.ingress_provenance(),
            Some(IngressProvenance::Unspecified)
        );
        assert_eq!(
            replaced.without_trusted_ingress().ingress_provenance(),
            None
        );
    }

    struct ProvenancePolicy {
        seen: std::sync::Mutex<Vec<(bool, Option<IngressProvenance>)>>,
    }

    impl OperationPolicy for ProvenancePolicy {
        fn before<'a>(
            &'a self,
            request: OperationPolicyRequest<'a>,
        ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>>
        {
            self.seen
                .lock()
                .expect("lock")
                .push((request.has_trusted_ingress, request.ingress_provenance));
            Box::pin(async { Ok(OperationPolicyPermit::default()) })
        }

        fn after<'a>(
            &'a self,
            _outcome: OperationPolicyOutcome<'a>,
        ) -> OperationPolicyFuture<'a, ()> {
            Box::pin(async {})
        }
    }

    #[tokio::test]
    async fn policy_sees_who_vouched_for_the_headers() {
        static DESCRIPTOR: OperationDescriptor = OperationDescriptor {
            key: "demo.users.find_user",
            codecs: &["json"],
            default_codec: "json",
            audiences: &["server"],
            scope: "regular",
            stream: RpcStreamMode::Unary,
        };
        let policy = Arc::new(ProvenancePolicy {
            seen: std::sync::Mutex::new(Vec::new()),
        });
        let contexts = [
            OperationContext::rpc_without_ingress(()),
            OperationContext::http_with_headers((), HeaderMap::new()),
            OperationContext::http_from_ingress(
                (),
                RpcV1HttpContext::from_headers_with_provenance(
                    HeaderMap::new(),
                    IngressProvenance::FunctionUrl,
                ),
            ),
        ];
        for context in contexts {
            invoke_operation_with_policy(
                &DESCRIPTOR,
                context.with_policy(policy.clone()),
                (),
                |_, ()| async { Ok::<_, ()>(()) },
            )
            .await
            .expect("operation result");
        }
        assert_eq!(
            *policy.seen.lock().expect("lock"),
            vec![
                (false, None),
                (true, Some(IngressProvenance::Unspecified)),
                (true, Some(IngressProvenance::FunctionUrl)),
            ]
        );
    }

    /// Authorizes on platform identity, records everything the post-hooks see.
    #[derive(Default)]
    struct IamAuditPolicy {
        events: std::sync::Mutex<Vec<String>>,
    }

    impl IamAuditPolicy {
        fn record(&self, hook: &str, outcome: &OperationPolicyOutcome<'_>) {
            self.events.lock().expect("lock").push(format!(
                "{hook} outcome={:?} ok={} principal={:?} admitted={:?} rejection={:?}",
                outcome.outcome,
                outcome.ok,
                outcome
                    .provider_identity
                    .map(|identity| identity.principal.as_str()),
                outcome.permit_values.get("principal"),
                outcome.rejection.map(|rejection| rejection.code.as_str()),
            ));
        }
    }

    impl OperationPolicy for IamAuditPolicy {
        fn before<'a>(
            &'a self,
            request: OperationPolicyRequest<'a>,
        ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>>
        {
            // The decision the identity seam exists for: no header-bearing
            // ingress, so identity must come from the platform or not at all.
            let decision = match request.provider_identity {
                Some(identity)
                    if identity.provider == IdentityProvider::AwsIam
                        && identity.account.as_deref() == Some("111122223333") =>
                {
                    Ok(OperationPolicyPermit {
                        values: BTreeMap::from([(
                            "principal".to_owned(),
                            Value::String(identity.principal.clone()),
                        )]),
                    })
                }
                Some(_) => Err(OperationPolicyRejection::new(
                    403,
                    "principal_not_allowed",
                    "platform identity is not authorized",
                )),
                None => Err(OperationPolicyRejection::new(
                    401,
                    "no_identity",
                    "no platform identity and no trusted ingress",
                )),
            };
            assert!(!request.has_trusted_ingress);
            Box::pin(async move { decision })
        }

        fn after<'a>(
            &'a self,
            outcome: OperationPolicyOutcome<'a>,
        ) -> OperationPolicyFuture<'a, ()> {
            self.record("after", &outcome);
            Box::pin(async {})
        }

        fn after_rejection<'a>(
            &'a self,
            outcome: OperationPolicyOutcome<'a>,
        ) -> OperationPolicyFuture<'a, ()> {
            self.record("after_rejection", &outcome);
            Box::pin(async {})
        }
    }

    #[tokio::test]
    async fn direct_invoke_authorizes_on_platform_identity_and_every_ending_is_audited() {
        static DESCRIPTOR: OperationDescriptor = OperationDescriptor {
            key: "demo.users.find_user",
            codecs: &["json"],
            default_codec: "json",
            audiences: &["server"],
            scope: "regular",
            stream: RpcStreamMode::Unary,
        };
        let policy = Arc::new(IamAuditPolicy::default());
        let role = "arn:aws:iam::111122223333:role/batch-runner";
        let direct = |identity: Option<ProviderIdentity>| {
            let context = OperationContext::rpc_without_ingress(())
                .with_environment(ExecutionEnvironmentKind::Lambda)
                .with_policy(policy.clone());
            match identity {
                Some(identity) => context.with_provider_identity(identity),
                None => context,
            }
        };
        let allowed =
            || ProviderIdentity::new(IdentityProvider::AwsIam, role).with_account("111122223333");

        // Admitted, operation succeeds. The handler can read the identity too.
        let seen = invoke_operation_with_policy(
            &DESCRIPTOR,
            direct(Some(allowed())),
            (),
            |context, ()| async move {
                Ok::<_, &'static str>(
                    context
                        .provider_identity()
                        .map(|identity| identity.principal.clone()),
                )
            },
        )
        .await
        .expect("admitted");
        assert_eq!(seen.as_deref(), Some(role));

        // Admitted, operation fails.
        let failed =
            invoke_operation_with_policy(&DESCRIPTOR, direct(Some(allowed())), (), |_, ()| async {
                Err::<(), _>("boom")
            })
            .await;
        assert!(matches!(
            failed,
            Err(OperationInvokeError::Operation("boom"))
        ));

        // Wrong account: rejected, the operation must never run.
        let wrong = ProviderIdentity::new(IdentityProvider::AwsIam, "arn:aws:iam::999:role/x")
            .with_account("999");
        let rejected =
            invoke_operation_with_policy(&DESCRIPTOR, direct(Some(wrong)), (), |_, ()| async {
                panic!("operation ran despite rejection") as Result<(), ()>
            })
            .await;
        assert!(matches!(rejected, Err(OperationInvokeError::Policy(ref r)) if r.status == 403));

        // No identity at all.
        let anonymous =
            invoke_operation_with_policy(&DESCRIPTOR, direct(None), (), |_, ()| async {
                panic!("operation ran despite rejection") as Result<(), ()>
            })
            .await;
        assert!(matches!(anonymous, Err(OperationInvokeError::Policy(ref r)) if r.status == 401));

        let admitted = format!("Some(String({role:?}))");
        assert_eq!(
            *policy.events.lock().expect("lock"),
            vec![
                format!("after outcome=Success ok=true principal=Some({role:?}) admitted={admitted} rejection=None"),
                format!("after outcome=OperationError ok=false principal=Some({role:?}) admitted={admitted} rejection=None"),
                // Rejections reach `after_rejection` only -- never `after` -- and
                // carry no permit values.
                "after_rejection outcome=PolicyRejected ok=false principal=Some(\"arn:aws:iam::999:role/x\") admitted=None rejection=Some(\"principal_not_allowed\")".to_owned(),
                "after_rejection outcome=PolicyRejected ok=false principal=None admitted=None rejection=Some(\"no_identity\")".to_owned(),
            ]
        );
    }

    /// A policy written before `after_rejection` existed must behave exactly as
    /// it did: rejection invokes neither of its hooks.
    #[tokio::test]
    async fn legacy_policy_without_after_rejection_is_unaffected_by_rejection() {
        struct RejectAll(AtomicUsize);
        impl OperationPolicy for RejectAll {
            fn before<'a>(
                &'a self,
                _request: OperationPolicyRequest<'a>,
            ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>>
            {
                Box::pin(async { Err(OperationPolicyRejection::new(403, "nope", "nope")) })
            }
            fn after<'a>(
                &'a self,
                _outcome: OperationPolicyOutcome<'a>,
            ) -> OperationPolicyFuture<'a, ()> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {})
            }
        }
        static DESCRIPTOR: OperationDescriptor = OperationDescriptor {
            key: "demo.users.find_user",
            codecs: &["json"],
            default_codec: "json",
            audiences: &["server"],
            scope: "regular",
            stream: RpcStreamMode::Unary,
        };
        let policy = Arc::new(RejectAll(AtomicUsize::new(0)));
        let result = invoke_operation_with_policy(
            &DESCRIPTOR,
            OperationContext::http(()).with_policy(policy.clone()),
            (),
            |_, ()| async { Ok::<(), ()>(()) },
        )
        .await;
        assert!(matches!(result, Err(OperationInvokeError::Policy(_))));
        assert_eq!(
            policy.0.load(Ordering::SeqCst),
            0,
            "after() must not run on rejection"
        );
    }

    struct PermitWithClaims;

    impl OperationPolicy for PermitWithClaims {
        fn before<'a>(
            &'a self,
            _request: OperationPolicyRequest<'a>,
        ) -> OperationPolicyFuture<'a, Result<OperationPolicyPermit, OperationPolicyRejection>>
        {
            Box::pin(async {
                Ok(OperationPolicyPermit {
                    values: BTreeMap::from([(
                        "subject_claims".to_owned(),
                        Value::String("SENTINEL-POLICY-CLAIM".into()),
                    )]),
                })
            })
        }

        fn after<'a>(
            &'a self,
            _outcome: OperationPolicyOutcome<'a>,
        ) -> OperationPolicyFuture<'a, ()> {
            Box::pin(async {})
        }
    }

    /// `{:?}` on a context is an easy thing to log by accident. It must never
    /// print a header value or a policy value -- only names and keys.
    #[tokio::test]
    async fn debug_output_never_contains_header_or_policy_values() {
        static DESCRIPTOR: OperationDescriptor = OperationDescriptor {
            key: "demo.users.find_user",
            codecs: &["json"],
            default_codec: "json",
            audiences: &["server"],
            scope: "regular",
            stream: RpcStreamMode::Unary,
        };
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("authorization", "Bearer SENTINEL-BEARER"),
            ("cookie", "session=SENTINEL-COOKIE"),
            ("cf-access-jwt-assertion", "SENTINEL-JWT"),
            ("x-request-id", "SENTINEL-REQUEST-ID"),
        ] {
            headers.insert(name, http::HeaderValue::from_static(value));
        }

        let ingress = RpcV1HttpContext::from_headers(headers.clone());
        let rendered = format!("{ingress:?} {ingress:#?}");
        assert!(!rendered.contains("SENTINEL"), "leaked: {rendered}");
        // Names stay visible: that is what makes the output useful.
        assert!(rendered.contains("authorization") && rendered.contains("cookie"));

        // Capture the context *after* policy has populated policy_values.
        let context = OperationContext::rpc((), ingress).with_policy(Arc::new(PermitWithClaims));
        let rendered =
            invoke_operation_with_policy(&DESCRIPTOR, context, (), |context, ()| async move {
                Ok::<_, ()>(format!("{context:?} {context:#?}"))
            })
            .await
            .expect("operation result");
        assert!(!rendered.contains("SENTINEL"), "leaked: {rendered}");
        assert!(
            rendered.contains("subject_claims"),
            "policy keys stay visible: {rendered}"
        );
        assert!(rendered.contains("x-request-id"));
    }
}
