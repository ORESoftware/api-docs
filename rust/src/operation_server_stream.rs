//! Provider-neutral server-stream operation primitive and RPC frame helpers.
//!
//! This module intentionally depends only on `futures-core`, serde/JSON and the
//! operation runtime. It does not know about Axum, Tokio, AWS, GCP, sockets, or
//! any other carrier. Provider and local-PPR adapters decide how to poll and
//! transport the frames.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
use serde_json::{Map, Value};

use crate::RpcStreamFrame;

/// The authored return type for a `server_stream` operation.
///
/// `T` is the operation contract's `ResponseBody`; `E` is its semantic error
/// type. The wrapper itself is transport-neutral and can contain any async
/// stream implementation.
pub struct OperationServerStream<T, E> {
    inner: Pin<Box<dyn Stream<Item = Result<T, E>> + Send + 'static>>,
}

impl<T, E> OperationServerStream<T, E> {
    #[must_use]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<T, E> Stream for OperationServerStream<T, E> {
    type Item = Result<T, E>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(context)
    }
}

/// Provider-neutral stream emitted by a generated API dispatch trampoline.
pub type RpcV1ServerStream = Pin<Box<dyn Stream<Item = RpcStreamFrame> + Send + 'static>>;

/// Canonical JSON representation shared with generated TypeScript/Dart/Go/Rust
/// stream clients (`v`, correlation `id`, frame type `t`, and optional payload).
#[must_use]
pub fn rpc_stream_frame_json(frame: &RpcStreamFrame) -> Value {
    let mut object = Map::new();
    object.insert("v".to_owned(), Value::from(1_u64));
    match frame {
        RpcStreamFrame::Data { id, body } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            object.insert("t".to_owned(), Value::String("data".to_owned()));
            object.insert("body".to_owned(), body.clone());
        }
        RpcStreamFrame::End { id } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            object.insert("t".to_owned(), Value::String("end".to_owned()));
        }
        RpcStreamFrame::RemoteError { id, code, message } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            object.insert("t".to_owned(), Value::String("error".to_owned()));
            object.insert("code".to_owned(), Value::String(code.clone()));
            if let Some(message) = message {
                object.insert("message".to_owned(), Value::String(message.clone()));
            }
        }
        RpcStreamFrame::Cancel { id } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            object.insert("t".to_owned(), Value::String("cancel".to_owned()));
        }
        RpcStreamFrame::Call { id } => {
            object.insert("id".to_owned(), Value::String(id.clone()));
            object.insert("t".to_owned(), Value::String("call".to_owned()));
        }
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_wire_frame_matches_client_contract() {
        assert_eq!(
            rpc_stream_frame_json(&RpcStreamFrame::Data {
                id: "stream-1".to_owned(),
                body: serde_json::json!({"event_id":"e1"}),
            }),
            serde_json::json!({
                "v": 1,
                "id": "stream-1",
                "t": "data",
                "body": {"event_id":"e1"}
            })
        );
        assert_eq!(
            rpc_stream_frame_json(&RpcStreamFrame::End {
                id: "stream-1".to_owned(),
            }),
            serde_json::json!({"v":1,"id":"stream-1","t":"end"})
        );
    }
}
