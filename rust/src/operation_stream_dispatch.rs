//! Transport-neutral server-stream dispatch.
//!
//! Generated `rpc.rs` code owns the closed operation-key switch. Each
//! server-stream arm calls this module to decode the same typed request sections
//! as unary dispatch and then maps semantic stream items onto the canonical
//! `RpcStreamFrame` protocol.

use std::{future::Future, pin::Pin, task::{Context, Poll}};

use futures_core::Stream;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use crate::{
    operation_dispatch::OperationDispatchContext, OperationContext, OperationInvokeError,
    OperationRequestData, OperationServerStream, OperationSpec, RpcPayloadCodec, RpcStreamFrame,
    RpcV1Call, RpcV1ServerStream, TypedOperationContext,
};

pub async fn dispatch_typed_json_server_stream_operation<S, O, Invoke, Fut>(
    state: S,
    dispatch_context: impl OperationDispatchContext<S>,
    call: RpcV1Call,
    invoke: Invoke,
) -> RpcV1ServerStream
where
    O: OperationSpec,
    O::Path: DeserializeOwned,
    O::Query: DeserializeOwned,
    O::RequestHeaders: DeserializeOwned,
    O::RequestBody: DeserializeOwned,
    O::ResponseBody: Serialize + Send + 'static,
    O::Error: Serialize + Send + 'static,
    Invoke: FnOnce(TypedOperationContext<S, O>) -> Fut,
    Fut: Future<
        Output = Result<
            OperationServerStream<O::ResponseBody, O::Error>,
            OperationInvokeError<O::Error>,
        >,
    >,
{
    dispatch_typed_json_server_stream_operation_in::<S, O, Invoke, Fut>(
        dispatch_context.into_operation_context(state),
        call,
        invoke,
    )
    .await
}

pub async fn dispatch_typed_json_server_stream_operation_in<S, O, Invoke, Fut>(
    base: OperationContext<S>,
    call: RpcV1Call,
    invoke: Invoke,
) -> RpcV1ServerStream
where
    O: OperationSpec,
    O::Path: DeserializeOwned,
    O::Query: DeserializeOwned,
    O::RequestHeaders: DeserializeOwned,
    O::RequestBody: DeserializeOwned,
    O::ResponseBody: Serialize + Send + 'static,
    O::Error: Serialize + Send + 'static,
    Invoke: FnOnce(TypedOperationContext<S, O>) -> Fut,
    Fut: Future<
        Output = Result<
            OperationServerStream<O::ResponseBody, O::Error>,
            OperationInvokeError<O::Error>,
        >,
    >,
{
    let id = call.id.clone();
    let path = match decode_section::<O::Path>(
        &call,
        "path",
        call.path.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(frame) => return one(frame),
    };
    let query = match decode_section::<O::Query>(
        &call,
        "query",
        call.query.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(frame) => return one(frame),
    };
    let headers = match decode_section::<O::RequestHeaders>(
        &call,
        "headers",
        call.headers.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(frame) => return one(frame),
    };
    let body_value = call.body.value().cloned().unwrap_or(Value::Null);
    let body = match decode_section::<O::RequestBody>(&call, "body", body_value.clone()) {
        Ok(value) => value,
        Err(frame) => return one(frame),
    };

    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    request.insert_path::<O>(path);
    request.insert_query::<O>(query);
    request.insert_headers::<O>(headers);
    request.insert_body::<O>(body);
    request.set_semantic_input(serde_json::json!({
        "path": &call.path,
        "query": &call.query,
        "headers": &call.headers,
        "body": body_value,
    }));
    let context = TypedOperationContext::<S, O>::new(base, request);

    match invoke(context).await {
        Ok(stream) => Box::pin(ResponseFrames {
            id,
            inner: stream,
            terminal: false,
        }),
        Err(error) => one(invoke_error_frame(&id, error)),
    }
}

fn decode_section<T>(
    call: &RpcV1Call,
    section: &'static str,
    value: Value,
) -> Result<T, RpcStreamFrame>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|error| RpcStreamFrame::RemoteError {
        id: call.id.clone(),
        code: "request_decode_failed".to_owned(),
        message: Some(format!("{section}: {error}")),
    })
}

fn one(frame: RpcStreamFrame) -> RpcV1ServerStream {
    crate::rpc_v1_server_stream_from_frames([frame])
}

fn invoke_error_frame<E: Serialize>(id: &str, error: OperationInvokeError<E>) -> RpcStreamFrame {
    match error {
        OperationInvokeError::Policy(rejection) => RpcStreamFrame::RemoteError {
            id: id.to_owned(),
            code: rejection.code,
            message: Some(rejection.message),
        },
        OperationInvokeError::PolicyInputEncode { message } => RpcStreamFrame::RemoteError {
            id: id.to_owned(),
            code: "policy_input_encode_failed".to_owned(),
            message: Some(message),
        },
        OperationInvokeError::Operation(error) => semantic_error_frame(id, error),
    }
}

fn semantic_error_frame<E: Serialize>(id: &str, error: E) -> RpcStreamFrame {
    match serde_json::to_value(error) {
        Ok(Value::Object(mut object)) => {
            let code = object
                .remove("code")
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "operation_error".to_owned());
            let message = object
                .remove("message")
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .or_else(|| (!object.is_empty()).then(|| Value::Object(object).to_string()));
            RpcStreamFrame::RemoteError {
                id: id.to_owned(),
                code,
                message,
            }
        }
        Ok(value) => RpcStreamFrame::RemoteError {
            id: id.to_owned(),
            code: "operation_error".to_owned(),
            message: Some(value.to_string()),
        },
        Err(error) => RpcStreamFrame::RemoteError {
            id: id.to_owned(),
            code: "operation_error_encode_failed".to_owned(),
            message: Some(error.to_string()),
        },
    }
}

struct ResponseFrames<T, E> {
    id: String,
    inner: OperationServerStream<T, E>,
    terminal: bool,
}

impl<T, E> Stream for ResponseFrames<T, E>
where
    T: Serialize,
    E: Serialize,
{
    type Item = RpcStreamFrame;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminal {
            return Poll::Ready(None);
        }
        match Pin::new(&mut self.inner).poll_next(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(item))) => match serde_json::to_value(item) {
                Ok(body) => Poll::Ready(Some(RpcStreamFrame::Data {
                    id: self.id.clone(),
                    body,
                })),
                Err(error) => {
                    self.terminal = true;
                    Poll::Ready(Some(RpcStreamFrame::RemoteError {
                        id: self.id.clone(),
                        code: "response_encode_failed".to_owned(),
                        message: Some(error.to_string()),
                    }))
                }
            },
            Poll::Ready(Some(Err(error))) => {
                self.terminal = true;
                Poll::Ready(Some(semantic_error_frame(&self.id, error)))
            }
            Poll::Ready(None) => {
                self.terminal = true;
                Poll::Ready(Some(RpcStreamFrame::End {
                    id: self.id.clone(),
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct Failure {
        code: String,
        message: String,
    }

    #[test]
    fn semantic_error_preserves_code_and_message() {
        assert_eq!(
            semantic_error_frame(
                "s1",
                Failure {
                    code: "boom".into(),
                    message: "failed".into(),
                },
            ),
            RpcStreamFrame::RemoteError {
                id: "s1".into(),
                code: "boom".into(),
                message: Some("failed".into()),
            }
        );
    }
}
