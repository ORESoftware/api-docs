//! Provider-neutral server-stream operation ABI.
//!
//! Authored `server_stream` operations return [`OperationServerStream<T, E>`].
//! The type contains no socket, HTTP, Lambda, Axum, Tokio, or provider concept:
//! hosts pull one item at a time and choose their own framing/transport.

use std::{future::Future, pin::Pin};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::{OptionalJson, RpcV1Call, RpcV1Receipt};

/// One asynchronous pull from an authored server stream.
pub type OperationServerStreamNext<'a, T, E> =
    Pin<Box<dyn Future<Output = Option<Result<T, E>>> + Send + 'a>>;

/// Source interface behind [`OperationServerStream`].
///
/// Product code may implement this directly when an item requires asynchronous
/// work (database notification, child process, broker subscription, etc.).
pub trait OperationServerStreamSource<T, E>: Send + 'static {
    fn next(&mut self) -> OperationServerStreamNext<'_, T, E>;
}

/// Compile-time trait used by `#[ores_operation(stream = "server_stream")]`.
///
/// The macro binds `Item` to `OperationSpec::ResponseBody` and `Error` to
/// `OperationSpec::Error`. A handler that accidentally returns the unary body
/// therefore fails to compile instead of becoming a runtime trap.
pub trait OperationServerStreamOutput: Send + 'static {
    type Item: Send + 'static;
    type Error: Send + 'static;

    fn next_output(&mut self) -> OperationServerStreamNext<'_, Self::Item, Self::Error>;
}

/// Provider-neutral typed stream returned by an authored server-stream handler.
pub struct OperationServerStream<T, E> {
    inner: Box<dyn OperationServerStreamSource<T, E>>,
}

impl<T, E> OperationServerStream<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    #[must_use]
    pub fn new<S>(source: S) -> Self
    where
        S: OperationServerStreamSource<T, E>,
    {
        Self {
            inner: Box::new(source),
        }
    }

    /// Convenience constructor for already-materialized/synchronous sources.
    #[must_use]
    pub fn from_iter<I>(items: I) -> Self
    where
        I: IntoIterator<Item = Result<T, E>>,
        I::IntoIter: Send + 'static,
    {
        Self::new(IteratorServerStreamSource {
            items: items.into_iter(),
        })
    }

    pub fn next(&mut self) -> OperationServerStreamNext<'_, T, E> {
        self.inner.next()
    }
}

impl<T, E> OperationServerStreamOutput for OperationServerStream<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    type Item = T;
    type Error = E;

    fn next_output(&mut self) -> OperationServerStreamNext<'_, T, E> {
        self.next()
    }
}

struct IteratorServerStreamSource<I> {
    items: I,
}

impl<T, E, I> OperationServerStreamSource<T, E> for IteratorServerStreamSource<I>
where
    T: Send + 'static,
    E: Send + 'static,
    I: Iterator<Item = Result<T, E>> + Send + 'static,
{
    fn next(&mut self) -> OperationServerStreamNext<'_, T, E> {
        let next = self.items.next();
        Box::pin(async move { next })
    }
}

/// Wire-neutral frame produced after a typed stream has crossed the semantic
/// operation boundary. Hosts may encode these as NDJSON, length-prefixed JSON,
/// WebSocket messages, provider streaming responses, etc.
#[derive(Clone, Debug, PartialEq)]
pub enum RpcV1ServerStreamFrame {
    Data {
        id: String,
        key: String,
        body: Value,
    },
    Error {
        id: String,
        key: String,
        status: u16,
        error: Map<String, Value>,
    },
    End {
        id: String,
        key: String,
    },
}

impl RpcV1ServerStreamFrame {
    #[must_use]
    pub fn to_json_value(&self) -> Value {
        match self {
            Self::Data { id, key, body } => serde_json::json!({
                "v": 1,
                "kind": "data",
                "id": id,
                "key": key,
                "body": body,
            }),
            Self::Error {
                id,
                key,
                status,
                error,
            } => serde_json::json!({
                "v": 1,
                "kind": "error",
                "id": id,
                "key": key,
                "status": status,
                "error": error,
            }),
            Self::End { id, key } => serde_json::json!({
                "v": 1,
                "kind": "end",
                "id": id,
                "key": key,
            }),
        }
    }

    pub fn to_ndjson(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut bytes = serde_json::to_vec(&self.to_json_value())?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// Erased stream consumed by provider/local hosts after typed operation policy
/// and request decoding have succeeded.
pub struct RpcV1ServerStream {
    id: String,
    key: String,
    source: Box<dyn ErasedServerStreamSource>,
    done: bool,
}

impl RpcV1ServerStream {
    pub fn from_typed<S>(call: &RpcV1Call, stream: S) -> Self
    where
        S: OperationServerStreamOutput + 'static,
        S::Item: Serialize,
        S::Error: Serialize,
    {
        Self {
            id: call.id.clone(),
            key: call.key.clone(),
            source: Box::new(TypedErasedServerStreamSource { stream }),
            done: false,
        }
    }

    /// Pull one frame. Exactly one `End` frame is emitted after the authored
    /// stream ends. An item error emits one terminal `Error` frame.
    pub async fn next_frame(&mut self) -> Option<RpcV1ServerStreamFrame> {
        if self.done {
            return None;
        }
        match self.source.next_erased().await {
            Some(Ok(body)) => Some(RpcV1ServerStreamFrame::Data {
                id: self.id.clone(),
                key: self.key.clone(),
                body,
            }),
            Some(Err(error)) => {
                self.done = true;
                Some(RpcV1ServerStreamFrame::Error {
                    id: self.id.clone(),
                    key: self.key.clone(),
                    status: 500,
                    error,
                })
            }
            None => {
                self.done = true;
                Some(RpcV1ServerStreamFrame::End {
                    id: self.id.clone(),
                    key: self.key.clone(),
                })
            }
        }
    }
}

/// Result of starting a server stream. Request decode, policy, and handler setup
/// errors stay ordinary correlated receipts; only an admitted stream becomes a
/// pullable stream object.
pub enum RpcV1ServerStreamStart {
    Stream(RpcV1ServerStream),
    Rejected(RpcV1Receipt),
}

impl RpcV1ServerStreamStart {
    #[must_use]
    pub fn rejected(call: &RpcV1Call, status: u16, code: &str, message: String) -> Self {
        let error = Map::from_iter([
            ("code".to_owned(), Value::String(code.to_owned())),
            ("message".to_owned(), Value::String(message)),
        ]);
        let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), status, error);
        receipt.trace_id = call.trace_id.clone();
        receipt.span_id = call.span_id.clone();
        receipt.body = OptionalJson::absent();
        Self::Rejected(receipt)
    }
}

trait ErasedServerStreamSource: Send {
    fn next_erased(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<Value, Map<String, Value>>>> + Send + '_>>;
}

struct TypedErasedServerStreamSource<S> {
    stream: S,
}

impl<S> ErasedServerStreamSource for TypedErasedServerStreamSource<S>
where
    S: OperationServerStreamOutput,
    S::Item: Serialize,
    S::Error: Serialize,
{
    fn next_erased(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<Value, Map<String, Value>>>> + Send + '_>> {
        Box::pin(async move {
            match self.stream.next_output().await {
                Some(Ok(item)) => Some(serde_json::to_value(item).map_err(|error| {
                    Map::from_iter([
                        (
                            "code".to_owned(),
                            Value::String("response_encode_failed".to_owned()),
                        ),
                        ("message".to_owned(), Value::String(error.to_string())),
                    ])
                })),
                Some(Err(error)) => {
                    let value = serde_json::to_value(error).unwrap_or_else(|encode_error| {
                        serde_json::json!({
                            "code": "operation_error_encode_failed",
                            "message": encode_error.to_string(),
                        })
                    });
                    let mut object = value
                        .as_object()
                        .cloned()
                        .unwrap_or_else(|| Map::from_iter([("detail".to_owned(), value)]));
                    object
                        .entry("code".to_owned())
                        .or_insert_with(|| Value::String("operation_error".to_owned()));
                    Some(Err(object))
                }
                None => None,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator_stream_emits_data_then_end() {
        let call = RpcV1Call::new("call-1", "demo.watch");
        let stream = OperationServerStream::<_, Value>::from_iter([
            Ok(serde_json::json!({"n": 1})),
            Ok(serde_json::json!({"n": 2})),
        ]);
        let mut stream = RpcV1ServerStream::from_typed(&call, stream);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            assert!(matches!(
                stream.next_frame().await,
                Some(RpcV1ServerStreamFrame::Data { .. })
            ));
            assert!(matches!(
                stream.next_frame().await,
                Some(RpcV1ServerStreamFrame::Data { .. })
            ));
            assert!(matches!(
                stream.next_frame().await,
                Some(RpcV1ServerStreamFrame::End { .. })
            ));
            assert!(stream.next_frame().await.is_none());
        });
    }
}
