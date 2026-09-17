//! Typed runtime boundary shared by ordinary HTTP adapters and `/v1/rpc`.
//!
//! Product operations receive `OperationContext<S>` plus one generated input
//! struct. The HTTP adapter constructs `OperationContext::http(state)` after
//! Axum extraction. The generated RPC adapter constructs
//! `OperationContext::rpc(state, rpc_http_context)`, decodes the semantic RPC
//! fields into that same input type, and calls the same `__ores_invoke_*`
//! function. No synthetic REST request is created.

use std::future::Future;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{OptionalJson, RpcV1Call, RpcV1HttpContext, RpcV1Receipt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationTransportKind {
    Http,
    Rpc,
}

/// Transport-neutral operation context.
///
/// `S` is normally the cloneable Axum application state (often an `Arc<_>`).
/// Trusted ingress headers remain available for RPC policy through
/// `rpc_http_context`; application request headers live in the typed operation
/// input and are therefore contract-checked separately.
#[derive(Clone, Debug)]
pub struct OperationContext<S> {
    state: S,
    transport: OperationTransportKind,
    rpc_http_context: Option<RpcV1HttpContext>,
}

impl<S> OperationContext<S> {
    #[must_use]
    pub fn http(state: S) -> Self {
        Self {
            state,
            transport: OperationTransportKind::Http,
            rpc_http_context: None,
        }
    }

    #[must_use]
    pub fn rpc(state: S, rpc_http_context: RpcV1HttpContext) -> Self {
        Self {
            state,
            transport: OperationTransportKind::Rpc,
            rpc_http_context: Some(rpc_http_context),
        }
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
    pub fn into_state(self) -> S {
        self.state
    }
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

/// Decode the transport envelope into the canonical generated operation input.
///
/// The input struct is expected to use the transport-neutral field names
/// `path`, `query`, `headers`, and `body` for the sections it declares. Missing
/// sections are omitted rather than synthesized as null, allowing generated
/// Rust input structs to model only the sections used by that operation.
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

/// Invoke one typed inner operation from `/v1/rpc`.
///
/// The same generated `__ores_invoke_*` function is passed here by server glue.
/// This initial semantic adapter serializes typed success/error values as JSON
/// values inside the v1 envelope. Codec-specific byte framing (Protobuf and
/// MessagePack) is layered around the same `Input`/`Success`/`Error` types and
/// must not change which operation function executes.
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
        Err(error) => return adapter_failure(&call, 400, "rpc_input_decode_failed", error.to_string()),
    };

    match invoke(context, input).await {
        Ok(output) => match serde_json::to_value(output) {
            Ok(value) => {
                let mut receipt = RpcV1Receipt::success(
                    call.id,
                    call.key,
                    OptionalJson::present(value),
                );
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

fn adapter_failure(
    call: &RpcV1Call,
    status: u16,
    code: &str,
    message: String,
) -> RpcV1Receipt {
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
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, PartialEq)]
    struct PathInput {
        user_id: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct HeadersInput {
        #[serde(rename = "if-none-match")]
        if_none_match: Option<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
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

        let receipt = invoke_shared_rpc_operation(
            (),
            call,
            |(), input: FindUserInput| async move {
                Ok::<FindUserOutput, FindUserError>(FindUserOutput {
                    user_id: input.path.user_id,
                })
            },
        )
        .await;
        assert!(receipt.ok);
        assert_eq!(receipt.status, Some(200));
        assert_eq!(
            receipt.body.value().and_then(|value| value.get("user_id")),
            Some(&Value::String("u-2".into()))
        );
    }
}
