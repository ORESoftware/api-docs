//! Typed runtime boundary shared by ordinary HTTP adapters and `/v1/rpc`.
//!
//! The canonical authored operation now receives one typed operation context.
//! The two-argument `OperationContext<S> + Input` helper remains available for
//! migration compatibility and is also used internally by the typed wrapper to
//! execute the shared policy boundary. RPC never creates a synthetic REST
//! request.

use std::{collections::BTreeMap, future::Future, sync::Arc};

use http::HeaderMap;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    operation_policy::{
        OperationDescriptor, OperationPolicy, OperationPolicyOutcome, OperationPolicyRejection,
        OperationPolicyRequest,
    },
    OptionalJson, RpcV1Call, RpcV1HttpContext, RpcV1Receipt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationTransportKind {
    Http,
    Rpc,
}

/// Transport-neutral base context. The canonical typed view is
/// `TypedOperationContext<S, O>`.
#[derive(Clone)]
pub struct OperationContext<S> {
    state: S,
    transport: OperationTransportKind,
    rpc_http_context: Option<RpcV1HttpContext>,
    trusted_headers: HeaderMap,
    policy: Option<Arc<dyn OperationPolicy>>,
    policy_values: BTreeMap<String, Value>,
}

impl<S: std::fmt::Debug> std::fmt::Debug for OperationContext<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationContext")
            .field("state", &self.state)
            .field("transport", &self.transport)
            .field("trusted_headers", &self.trusted_headers)
            .field("has_policy", &self.policy.is_some())
            .field("policy_values", &self.policy_values)
            .finish_non_exhaustive()
    }
}

impl<S> OperationContext<S> {
    #[must_use]
    pub fn http(state: S) -> Self {
        Self {
            state,
            transport: OperationTransportKind::Http,
            rpc_http_context: None,
            trusted_headers: HeaderMap::new(),
            policy: None,
            policy_values: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn http_with_headers(state: S, trusted_headers: HeaderMap) -> Self {
        Self {
            state,
            transport: OperationTransportKind::Http,
            rpc_http_context: None,
            trusted_headers,
            policy: None,
            policy_values: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn rpc(state: S, rpc_http_context: RpcV1HttpContext) -> Self {
        let trusted_headers = rpc_http_context.request_headers().clone();
        Self {
            state,
            transport: OperationTransportKind::Rpc,
            rpc_http_context: Some(rpc_http_context),
            trusted_headers,
            policy: None,
            policy_values: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_policy(mut self, policy: Arc<dyn OperationPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }

    #[must_use]
    pub fn with_trusted_headers(mut self, trusted_headers: HeaderMap) -> Self {
        self.trusted_headers = trusted_headers;
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
    pub fn rpc_http_context(&self) -> Option<&RpcV1HttpContext> {
        self.rpc_http_context.as_ref()
    }

    #[must_use]
    pub fn trusted_headers(&self) -> &HeaderMap {
        &self.trusted_headers
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

    let policy = context.policy.clone();
    if let Some(policy) = policy.as_ref() {
        let permit = policy
            .before(OperationPolicyRequest {
                operation,
                transport,
                trusted_headers: &context.trusted_headers,
                input: &input_value,
            })
            .await
            .map_err(OperationInvokeError::Policy)?;
        context.policy_values = permit.values;
    }

    let result = invoke(context, input).await;

    if let Some(policy) = policy.as_ref() {
        policy
            .after(OperationPolicyOutcome {
                operation,
                transport,
                ok: result.is_ok(),
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
        OperationPolicyFuture, OperationPolicyPermit, OperationPolicyRequest,
    };
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
}
