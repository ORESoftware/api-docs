//! Transport-neutral server-stream dispatch.
//!
//! Generated `rpc.rs` code owns the closed operation-key switch. Each
//! server-stream arm calls this module to decode the same typed request sections
//! as unary dispatch and then maps semantic stream items onto the canonical
//! `RpcStreamFrame` protocol.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::{
    operation_dispatch::OperationDispatchContext, OperationContext, OperationInvokeError,
    OperationRequestData, OperationServerStream, OperationSpec, RpcPayloadCodec, RpcStreamFrame,
    RpcStreamMode, RpcV1Call, RpcV1ServerStream, TypedOperationContext,
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

    // The generated key switch is the first authority, but this generic
    // boundary independently verifies the selected OperationSpec. A stale or
    // incorrectly generated arm therefore cannot execute the wrong semantic
    // handler merely because its Rust types happen to line up.
    if call.key != O::KEY {
        return one(RpcStreamFrame::RemoteError {
            id,
            code: "operation_key_mismatch".to_owned(),
            message: Some(format!(
                "selected operation spec {:?} does not match incoming key {:?}",
                O::KEY,
                call.key
            )),
        });
    }

    // Likewise, never let the stream dispatcher become a compatibility path
    // for a stale unary spec. The proc macro checks authored handlers at build
    // time; this guard protects generated/embedded callers at runtime too.
    if O::STREAM != RpcStreamMode::ServerStream {
        return one(RpcStreamFrame::RemoteError {
            id,
            code: "operation_stream_mode_mismatch".to_owned(),
            message: Some(format!(
                "operation spec {:?} declares {:?}, expected server_stream",
                O::KEY,
                O::STREAM
            )),
        });
    }

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
        call.headers
            .clone()
            .map(Value::Object)
            .unwrap_or(Value::Null),
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
    use crate::{next_rpc_v1_server_stream_frame, NoSection};
    use serde::Deserialize;

    #[derive(Clone, Debug, Deserialize, serde::Serialize)]
    struct Failure {
        code: String,
        message: String,
    }

    macro_rules! operation_spec {
        ($name:ident, $key:literal, $stream:expr) => {
            struct $name;
            impl OperationSpec for $name {
                type Path = NoSection;
                type Query = NoSection;
                type RequestHeaders = NoSection;
                type RequestBody = NoSection;
                type ResponseBody = serde_json::Value;
                type ResponseHeaders = NoSection;
                type ResponseTrailers = NoSection;
                type Error = serde_json::Value;

                const KEY: &'static str = $key;
                const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
                const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
                const STREAM: RpcStreamMode = $stream;
            }
        };
    }

    operation_spec!(WatchEvents, "demo.events.watch_stream", RpcStreamMode::ServerStream);
    operation_spec!(UnaryEvents, "demo.events.unary", RpcStreamMode::Unary);

    async fn impossible_watch_invoke(
        _context: TypedOperationContext<(), WatchEvents>,
    ) -> Result<
        OperationServerStream<serde_json::Value, serde_json::Value>,
        OperationInvokeError<serde_json::Value>,
    > {
        panic!("contract guard must reject before invoking the semantic handler")
    }

    async fn impossible_unary_invoke(
        _context: TypedOperationContext<(), UnaryEvents>,
    ) -> Result<
        OperationServerStream<serde_json::Value, serde_json::Value>,
        OperationInvokeError<serde_json::Value>,
    > {
        panic!("stream-mode guard must reject before invoking the semantic handler")
    }

    #[derive(Debug)]
    struct Items<T> {
        inner: std::vec::IntoIter<T>,
    }

    impl<T> Items<T> {
        fn new(items: impl IntoIterator<Item = T>) -> Self {
            Self {
                inner: items.into_iter().collect::<Vec<_>>().into_iter(),
            }
        }
    }

    impl<T: Unpin> Stream for Items<T> {
        type Item = T;

        fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<T>> {
            Poll::Ready(self.inner.next())
        }
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

    #[tokio::test]
    async fn mismatched_operation_key_fails_closed_before_invoke() {
        let mut stream = dispatch_typed_json_server_stream_operation_in::<_, WatchEvents, _, _>(
            OperationContext::rpc_without_ingress(()),
            RpcV1Call::new("s-key", "demo.events.some_other_stream"),
            impossible_watch_invoke,
        )
        .await;

        assert!(matches!(
            next_rpc_v1_server_stream_frame(&mut stream).await,
            Some(RpcStreamFrame::RemoteError { code, .. }) if code == "operation_key_mismatch"
        ));
        assert!(next_rpc_v1_server_stream_frame(&mut stream).await.is_none());
    }

    #[tokio::test]
    async fn unary_spec_fails_closed_before_stream_invoke() {
        let mut stream = dispatch_typed_json_server_stream_operation_in::<_, UnaryEvents, _, _>(
            OperationContext::rpc_without_ingress(()),
            RpcV1Call::new("s-mode", UnaryEvents::KEY),
            impossible_unary_invoke,
        )
        .await;

        assert!(matches!(
            next_rpc_v1_server_stream_frame(&mut stream).await,
            Some(RpcStreamFrame::RemoteError { code, .. }) if code == "operation_stream_mode_mismatch"
        ));
        assert!(next_rpc_v1_server_stream_frame(&mut stream).await.is_none());
    }

    #[tokio::test]
    async fn response_frames_emit_data_then_exactly_one_end() {
        let semantic = OperationServerStream::new(Items::new([Ok::<_, serde_json::Value>(
            serde_json::json!({"event":"one"}),
        )]));
        let mut stream: RpcV1ServerStream = Box::pin(ResponseFrames {
            id: "s-data".into(),
            inner: semantic,
            terminal: false,
        });

        assert_eq!(
            next_rpc_v1_server_stream_frame(&mut stream).await,
            Some(RpcStreamFrame::Data {
                id: "s-data".into(),
                body: serde_json::json!({"event":"one"}),
            })
        );
        assert_eq!(
            next_rpc_v1_server_stream_frame(&mut stream).await,
            Some(RpcStreamFrame::End {
                id: "s-data".into(),
            })
        );
        assert!(next_rpc_v1_server_stream_frame(&mut stream).await.is_none());
    }

    #[tokio::test]
    async fn semantic_error_is_terminal_and_drops_later_items() {
        let semantic = OperationServerStream::new(Items::new([
            Err(serde_json::json!({"code":"boom","message":"failed"})),
            Ok(serde_json::json!({"event":"must-not-escape"})),
        ]));
        let mut stream: RpcV1ServerStream = Box::pin(ResponseFrames {
            id: "s-error".into(),
            inner: semantic,
            terminal: false,
        });

        assert_eq!(
            next_rpc_v1_server_stream_frame(&mut stream).await,
            Some(RpcStreamFrame::RemoteError {
                id: "s-error".into(),
                code: "boom".into(),
                message: Some("failed".into()),
            })
        );
        assert!(next_rpc_v1_server_stream_frame(&mut stream).await.is_none());
    }
}
