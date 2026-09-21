//! Typed server-stream semantic dispatch.
//!
//! This is the streaming sibling of `operation_dispatch`: it decodes the same
//! request sections and enters the same generated `__ores_invoke_*` policy
//! boundary, but preserves the returned stream instead of buffering it into a
//! unary `RpcV1Receipt`.

use std::future::Future;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::{
    OperationContext, OperationInvokeError, OperationRequestData, OperationServerStreamOutput,
    OperationSpec, RpcPayloadCodec, RpcV1Call, RpcV1ServerStream, RpcV1ServerStreamStart,
    TypedOperationContext,
};

pub async fn dispatch_typed_json_server_stream_operation_in<S, O, Stream, Invoke, Fut>(
    base: OperationContext<S>,
    call: RpcV1Call,
    invoke: Invoke,
) -> RpcV1ServerStreamStart
where
    O: OperationSpec,
    O::Path: DeserializeOwned,
    O::Query: DeserializeOwned,
    O::RequestHeaders: DeserializeOwned,
    O::RequestBody: DeserializeOwned,
    O::ResponseBody: Serialize,
    O::Error: Serialize,
    Stream: OperationServerStreamOutput<Item = O::ResponseBody, Error = O::Error>,
    Invoke: FnOnce(TypedOperationContext<S, O>) -> Fut,
    Fut: Future<Output = Result<Stream, OperationInvokeError<O::Error>>>,
{
    let path = match decode_section::<O::Path>(
        &call,
        "path",
        call.path.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(start) => return start,
    };
    let query = match decode_section::<O::Query>(
        &call,
        "query",
        call.query.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(start) => return start,
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
        Err(start) => return start,
    };
    let body_value = call.body.value().cloned().unwrap_or(Value::Null);
    let body = match decode_section::<O::RequestBody>(&call, "body", body_value.clone()) {
        Ok(value) => value,
        Err(start) => return start,
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
        Ok(stream) => RpcV1ServerStreamStart::Stream(RpcV1ServerStream::from_typed(&call, stream)),
        Err(OperationInvokeError::Policy(rejection)) => RpcV1ServerStreamStart::rejected(
            &call,
            rejection.status,
            &rejection.code,
            rejection.message,
        ),
        Err(error) => {
            let encoded = serde_json::to_value(error).unwrap_or_else(|encode_error| {
                serde_json::json!({
                    "code": "operation_error_encode_failed",
                    "message": encode_error.to_string(),
                })
            });
            let code = encoded
                .as_object()
                .and_then(|object| object.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("operation_error");
            RpcV1ServerStreamStart::rejected(&call, 500, code, encoded.to_string())
        }
    }
}

fn decode_section<T>(
    call: &RpcV1Call,
    section: &'static str,
    value: Value,
) -> Result<T, RpcV1ServerStreamStart>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|error| {
        RpcV1ServerStreamStart::rejected(
            call,
            400,
            "request_decode_failed",
            format!("{section}: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NoSection, OperationDescriptor, OperationServerStream, OperationTransportKind,
        RpcStreamMode,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Event {
        n: u32,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct EventError {
        code: String,
    }

    struct Watch;

    impl OperationSpec for Watch {
        type Path = NoSection;
        type Query = NoSection;
        type RequestHeaders = NoSection;
        type RequestBody = NoSection;
        type ResponseBody = Event;
        type ResponseHeaders = NoSection;
        type ResponseTrailers = NoSection;
        type Error = EventError;

        const KEY: &'static str = "demo.watch";
        const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
        const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
    }

    static DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        key: "demo.watch",
        codecs: &["json"],
        default_codec: "json",
        audiences: &["server"],
        scope: "regular",
        stream: RpcStreamMode::ServerStream,
    };

    #[test]
    fn dispatch_preserves_stream_items_instead_of_buffering_receipt() {
        let call = RpcV1Call::new("call-1", Watch::KEY);
        let base = OperationContext::rpc_without_ingress(());
        assert_eq!(base.transport(), OperationTransportKind::Rpc);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let start = dispatch_typed_json_server_stream_operation_in::<WatchState, Watch, _, _, _>(
                OperationContext::rpc_without_ingress(WatchState),
                call,
                |context| async move {
                    crate::invoke_typed_context_operation(&DESCRIPTOR, context, |_context| async {
                        Ok::<_, EventError>(OperationServerStream::from_iter([
                            Ok(Event { n: 1 }),
                            Ok(Event { n: 2 }),
                        ]))
                    })
                    .await
                },
            )
            .await;
            let RpcV1ServerStreamStart::Stream(mut stream) = start else {
                panic!("stream must start");
            };
            assert!(matches!(
                stream.next_frame().await,
                Some(crate::RpcV1ServerStreamFrame::Data { .. })
            ));
            assert!(matches!(
                stream.next_frame().await,
                Some(crate::RpcV1ServerStreamFrame::Data { .. })
            ));
            assert!(matches!(
                stream.next_frame().await,
                Some(crate::RpcV1ServerStreamFrame::End { .. })
            ));
        });
    }

    #[derive(Clone)]
    struct WatchState;
}
