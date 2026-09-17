use std::{future::Future, sync::atomic::Ordering};

use ores_api_docs::{
    OperationContext, OperationInvokeError, OperationRequestData, OperationSpec, OptionalJson,
    RpcPayloadCodec, RpcV1Call, RpcV1HttpContext, RpcV1Receipt, TypedOperationContext,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use crate::{model::OperationEnvelope, state::AppState};

pub trait ProofTraceEnvelope {
    fn prepend_proof_trace(&mut self, trace_id: &'static str);
}

impl<T> ProofTraceEnvelope for OperationEnvelope<T> {
    fn prepend_proof_trace(&mut self, trace_id: &'static str) {
        self.prepend_trace_id(trace_id);
    }
}

pub async fn invoke_json_rpc<O, Invoke, Fut>(
    state: AppState,
    http_context: RpcV1HttpContext,
    call: RpcV1Call,
    rpc_trace_id: &'static str,
    invoke: Invoke,
) -> RpcV1Receipt
where
    O: OperationSpec,
    O::Path: DeserializeOwned,
    O::Query: DeserializeOwned,
    O::RequestHeaders: DeserializeOwned,
    O::RequestBody: DeserializeOwned,
    O::ResponseBody: Serialize + ProofTraceEnvelope,
    O::Error: Serialize,
    Invoke: FnOnce(TypedOperationContext<AppState, O>) -> Fut,
    Fut: Future<Output = Result<O::ResponseBody, OperationInvokeError<O::Error>>>,
{
    state
        .counters
        .rpc_adapter_hits
        .fetch_add(1, Ordering::SeqCst);

    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    let path = match decode_object_or_null::<O::Path>(call.path.as_ref()) {
        Ok(value) => value,
        Err(message) => return decode_failure(&call, "path_decode_failed", message),
    };
    let query = match decode_object_or_null::<O::Query>(call.query.as_ref()) {
        Ok(value) => value,
        Err(message) => return decode_failure(&call, "query_decode_failed", message),
    };
    let headers = match decode_object_or_null::<O::RequestHeaders>(call.headers.as_ref()) {
        Ok(value) => value,
        Err(message) => return decode_failure(&call, "headers_decode_failed", message),
    };
    let body = match serde_json::from_value::<O::RequestBody>(
        call.body.value().cloned().unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(error) => return decode_failure(&call, "body_decode_failed", error.to_string()),
    };

    request.insert_path::<O>(path);
    request.insert_query::<O>(query);
    request.insert_headers::<O>(headers);
    request.insert_body::<O>(body);
    request.set_semantic_input(serde_json::json!({
        "path": call.path,
        "query": call.query,
        "headers": call.headers,
        "body": call.body.value(),
    }));

    let base = OperationContext::rpc(state.clone(), http_context).with_policy(state.policy.clone());
    let ctx = TypedOperationContext::<AppState, O>::new(base, request);
    match invoke(ctx).await {
        Ok(mut output) => {
            output.prepend_proof_trace(rpc_trace_id);
            match serde_json::to_value(output) {
                Ok(value) => {
                    let mut receipt = RpcV1Receipt::success(
                        call.id.clone(),
                        call.key.clone(),
                        OptionalJson::present(value),
                    );
                    receipt.status = Some(200);
                    receipt.trace_id = call.trace_id.clone();
                    receipt.span_id = call.span_id.clone();
                    receipt
                }
                Err(error) => decode_failure(
                    &call,
                    "response_encode_failed",
                    error.to_string(),
                ),
            }
        }
        Err(error) => {
            let value = serde_json::to_value(error).unwrap_or_else(|encode_error| {
                serde_json::json!({
                    "code":"operation_error_encode_failed",
                    "message":encode_error.to_string()
                })
            });
            let mut object = value.as_object().cloned().unwrap_or_else(|| {
                Map::from_iter([("detail".into(), value)])
            });
            object
                .entry("code".to_owned())
                .or_insert_with(|| Value::String("operation_error".into()));
            let mut receipt = RpcV1Receipt::failure(
                call.id.clone(),
                call.key.clone(),
                400,
                object,
            );
            receipt.trace_id = call.trace_id.clone();
            receipt.span_id = call.span_id.clone();
            receipt
        }
    }
}

fn decode_object_or_null<T>(value: Option<&Map<String, Value>>) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.cloned().map(Value::Object).unwrap_or(Value::Null))
        .map_err(|error| error.to_string())
}

fn decode_failure(call: &RpcV1Call, code: &str, message: String) -> RpcV1Receipt {
    let error = Map::from_iter([
        ("code".into(), Value::String(code.to_owned())),
        ("message".into(), Value::String(message)),
    ]);
    let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), 400, error);
    receipt.trace_id = call.trace_id.clone();
    receipt.span_id = call.span_id.clone();
    receipt
}
