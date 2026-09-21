//! Provider-neutral server-stream operation primitive and RPC frame helpers.
//!
//! This module intentionally depends only on `futures-core`, serde/JSON and the
//! operation runtime. It does not know about Axum, Tokio, AWS, GCP, sockets, or
//! any other carrier. Provider and local-PPR adapters decide how to poll and
//! transport the frames.

use std::{
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
use serde_json::{Map, Value};

use crate::{OperationSpec, RpcStreamFrame};

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

/// Canonical authored return type for a `server_stream` operation.
///
/// This alias couples each yielded item/error directly to the same generated
/// [`OperationSpec`] that defines the request and client-facing response types.
/// A handler should normally return `ServerStreamResult<MyOperation>` rather
/// than spelling the associated response/error types a second time.
pub type ServerStreamResult<O> =
    OperationServerStream<<O as OperationSpec>::ResponseBody, <O as OperationSpec>::Error>;

/// Provider-neutral stream emitted by a generated API dispatch trampoline.
pub type RpcV1ServerStream = Pin<Box<dyn Stream<Item = RpcStreamFrame> + Send + 'static>>;

/// Poll one frame without forcing a provider/local host to link its own stream
/// extension crate. This keeps generated host adapters dependent only on the
/// public operation-runtime ABI.
pub async fn next_rpc_v1_server_stream_frame(
    stream: &mut RpcV1ServerStream,
) -> Option<RpcStreamFrame> {
    poll_fn(|context| stream.as_mut().poll_next(context)).await
}

/// Turn a finite set of frames into the provider-neutral async stream ABI.
#[must_use]
pub fn rpc_v1_server_stream_from_frames(
    frames: impl IntoIterator<Item = RpcStreamFrame>,
) -> RpcV1ServerStream {
    struct Frames {
        inner: std::vec::IntoIter<RpcStreamFrame>,
    }
    impl Stream for Frames {
        type Item = RpcStreamFrame;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.inner.next())
        }
    }
    Box::pin(Frames {
        inner: frames.into_iter().collect::<Vec<_>>().into_iter(),
    })
}

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

    #[test]
    fn finite_frame_stream_preserves_order() {
        let mut stream = rpc_v1_server_stream_from_frames([
            RpcStreamFrame::Data {
                id: "s".to_owned(),
                body: serde_json::json!(1),
            },
            RpcStreamFrame::End { id: "s".to_owned() },
        ]);
        let waker = std::task::Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            stream.as_mut().poll_next(&mut context),
            Poll::Ready(Some(RpcStreamFrame::Data { .. }))
        ));
        assert!(matches!(
            stream.as_mut().poll_next(&mut context),
            Poll::Ready(Some(RpcStreamFrame::End { .. }))
        ));
        assert!(matches!(
            stream.as_mut().poll_next(&mut context),
            Poll::Ready(None)
        ));
    }
}
