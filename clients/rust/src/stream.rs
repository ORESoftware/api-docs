use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcStreamCarrier {
    WebSocket,
    Tcp,
}

impl RpcStreamCarrier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WebSocket => "websocket",
            Self::Tcp => "tcp",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RpcStreamFrame {
    Data {
        id: String,
        body: serde_json::Value,
    },
    End {
        id: String,
    },
    RemoteError {
        id: String,
        code: String,
        message: Option<String>,
    },
    Cancel {
        id: String,
    },
    Call {
        id: String,
    },
}

impl RpcStreamFrame {
    fn id(&self) -> &str {
        match self {
            Self::Data { id, .. }
            | Self::End { id }
            | Self::RemoteError { id, .. }
            | Self::Cancel { id }
            | Self::Call { id } => id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RpcStreamCall {
    pub id: String,
    pub key: String,
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
}

pub struct RpcStreamSession<E> {
    incoming: Box<dyn Iterator<Item = Result<RpcStreamFrame, E>>>,
    cancel: Option<Box<dyn FnMut() -> Result<(), E>>>,
}

impl<E> RpcStreamSession<E> {
    pub fn new<I>(incoming: I) -> Self
    where
        I: Iterator<Item = Result<RpcStreamFrame, E>> + 'static,
    {
        Self {
            incoming: Box::new(incoming),
            cancel: None,
        }
    }

    pub fn with_cancel<F>(mut self, cancel: F) -> Self
    where
        F: FnMut() -> Result<(), E> + 'static,
    {
        self.cancel = Some(Box::new(cancel));
        self
    }
}

pub trait FramedRpcStream {
    type Error;

    fn carrier(&self) -> RpcStreamCarrier;

    fn open(
        &self,
        call: RpcStreamCall,
    ) -> Result<RpcStreamSession<Self::Error>, Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcStreamContext {
    pub id: String,
    pub key: String,
    pub carrier: RpcStreamCarrier,
    pub ended: bool,
    pub cancelled: bool,
}

#[derive(Debug)]
pub enum RpcStreamError<E> {
    Carrier(E),
    Remote {
        code: String,
        message: Option<String>,
    },
    Protocol(String),
}

impl<E: std::fmt::Debug> std::fmt::Display for RpcStreamError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Carrier(error) => write!(formatter, "carrier: {error:?}"),
            Self::Remote { code, message } => match message {
                Some(message) => write!(formatter, "remote {code}: {message}"),
                None => write!(formatter, "remote {code}"),
            },
            Self::Protocol(message) => formatter.write_str(message),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for RpcStreamError<E> {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcStreamPrepareError {
    OperationNotAllowed(String),
}

impl std::fmt::Display for RpcStreamPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OperationNotAllowed(key) => {
                write!(formatter, "RPC stream operation not generated for this audience: {key}")
            }
        }
    }
}

impl std::error::Error for RpcStreamPrepareError {}

pub struct RpcStreamClient<E, T, F>
where
    F: Fn(serde_json::Value) -> Result<T, String>,
{
    session: RpcStreamSession<E>,
    id: String,
    key: String,
    carrier: RpcStreamCarrier,
    decode: F,
    ended: bool,
    cancelled: bool,
    done: bool,
}

impl<E, T, F> RpcStreamClient<E, T, F>
where
    F: Fn(serde_json::Value) -> Result<T, String>,
{
    #[must_use]
    pub fn context(&self) -> RpcStreamContext {
        RpcStreamContext {
            id: self.id.clone(),
            key: self.key.clone(),
            carrier: self.carrier,
            ended: self.ended,
            cancelled: self.cancelled,
        }
    }

    pub fn cancel(&mut self) -> Result<(), RpcStreamError<E>> {
        if self.ended || self.cancelled {
            return Ok(());
        }
        if let Some(cancel) = self.session.cancel.as_mut() {
            cancel().map_err(RpcStreamError::Carrier)?;
        }
        self.cancelled = true;
        self.done = true;
        Ok(())
    }
}

impl<E, T, F> Iterator for RpcStreamClient<E, T, F>
where
    F: Fn(serde_json::Value) -> Result<T, String>,
{
    type Item = Result<T, RpcStreamError<E>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let frame = match self.session.incoming.next() {
            Some(Ok(frame)) => frame,
            Some(Err(error)) => {
                self.done = true;
                return Some(Err(RpcStreamError::Carrier(error)));
            }
            None => {
                self.done = true;
                if self.ended || self.cancelled {
                    return None;
                }
                return Some(Err(RpcStreamError::Protocol(
                    "stream transport ended without an end or cancel frame".to_owned(),
                )));
            }
        };

        if frame.id() != self.id {
            self.done = true;
            return Some(Err(RpcStreamError::Protocol(format!(
                "frame for correlation id {} arrived on the stream for {}",
                frame.id(),
                self.id
            ))));
        }

        match frame {
            RpcStreamFrame::Data { body, .. } => {
                Some((self.decode)(body).map_err(RpcStreamError::Protocol))
            }
            RpcStreamFrame::End { .. } => {
                self.ended = true;
                self.done = true;
                None
            }
            RpcStreamFrame::RemoteError { code, message, .. } => {
                self.done = true;
                Some(Err(RpcStreamError::Remote { code, message }))
            }
            RpcStreamFrame::Cancel { .. } => {
                self.cancelled = true;
                self.done = true;
                None
            }
            RpcStreamFrame::Call { .. } => {
                self.done = true;
                Some(Err(RpcStreamError::Protocol(
                    "a call frame cannot arrive inside its response stream".to_owned(),
                )))
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RpcStreamRequest {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
}

pub struct RpcStreamCallBuilder<'a, S, T, F>
where
    S: FramedRpcStream,
    F: Fn(serde_json::Value) -> Result<T, String>,
{
    owner: &'a OresRpcStreamClient<S>,
    key: String,
    request: RpcStreamRequest,
    decode: F,
    opened: bool,
    _item: PhantomData<T>,
}

impl<'a, S, T, F> RpcStreamCallBuilder<'a, S, T, F>
where
    S: FramedRpcStream,
    F: Fn(serde_json::Value) -> Result<T, String>,
{
    pub fn add_query_field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.request.query.push((name.into(), value.into()));
        self
    }

    pub fn with_body(mut self, body: serde_json::Value) -> Self {
        self.request.body = Some(body);
        self
    }

    pub fn add_body_field(
        mut self,
        name: impl Into<String>,
        value: impl serde::Serialize,
    ) -> Self {
        let encoded = match serde_json::to_value(value) {
            Ok(value) => value,
            Err(error) => {
                self.request.body = Some(serde_json::json!({
                    "__ores_stream_build_error": error.to_string()
                }));
                return self;
            }
        };
        if self.request.body.is_none() {
            self.request.body = Some(serde_json::json!({}));
        }
        if let Some(object) = self
            .request
            .body
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        {
            object.insert(name.into(), encoded);
        } else {
            self.request.body = Some(serde_json::json!({
                "__ores_stream_build_error":
                    "add_body_field requires an object RPC body"
            }));
        }
        self
    }

    /// Sole transport-open / network-I/O boundary.
    pub fn stream(
        mut self,
    ) -> Result<RpcStreamClient<S::Error, T, F>, RpcStreamError<S::Error>> {
        if self.opened {
            return Err(RpcStreamError::Protocol(
                "an RPC stream call builder can only be opened once".to_owned(),
            ));
        }
        self.opened = true;

        if let Some(error) = self
            .request
            .body
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .and_then(|body| body.get("__ores_stream_build_error"))
            .and_then(serde_json::Value::as_str)
        {
            return Err(RpcStreamError::Protocol(error.to_owned()));
        }

        let id = self.owner.next_id();
        let call = RpcStreamCall {
            id: id.clone(),
            key: self.key.clone(),
            method: self.request.method,
            path: self.request.path,
            query: self.request.query,
            body: self.request.body,
        };
        let carrier = self.owner.stream.carrier();
        let session = self
            .owner
            .stream
            .open(call)
            .map_err(RpcStreamError::Carrier)?;

        Ok(RpcStreamClient {
            session,
            id,
            key: self.key,
            carrier,
            decode: self.decode,
            ended: false,
            cancelled: false,
            done: false,
        })
    }
}

pub struct OresRpcStreamClient<S> {
    stream: S,
    operations: BTreeSet<String>,
    id_prefix: String,
    sequence: AtomicU64,
}

impl<S> OresRpcStreamClient<S>
where
    S: FramedRpcStream,
{
    pub fn new(
        stream: S,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            stream,
            operations: operations.into_iter().map(Into::into).collect(),
            id_prefix: "stream-".to_owned(),
            sequence: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn with_id_prefix(mut self, id_prefix: impl Into<String>) -> Self {
        self.id_prefix = id_prefix.into();
        self
    }

    pub fn prepare<T, F>(
        &self,
        key: impl Into<String>,
        request: RpcStreamRequest,
        decode: F,
    ) -> Result<RpcStreamCallBuilder<'_, S, T, F>, RpcStreamPrepareError>
    where
        F: Fn(serde_json::Value) -> Result<T, String>,
    {
        let key = key.into();
        if !self.operations.contains(&key) {
            return Err(RpcStreamPrepareError::OperationNotAllowed(key));
        }
        Ok(RpcStreamCallBuilder {
            owner: self,
            key,
            request,
            decode,
            opened: false,
            _item: PhantomData,
        })
    }

    fn next_id(&self) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{}{sequence}", self.id_prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct TestStream {
        opens: Rc<Cell<usize>>,
        frames: Vec<RpcStreamFrame>,
    }

    impl FramedRpcStream for TestStream {
        type Error = &'static str;

        fn carrier(&self) -> RpcStreamCarrier {
            RpcStreamCarrier::WebSocket
        }

        fn open(
            &self,
            call: RpcStreamCall,
        ) -> Result<RpcStreamSession<Self::Error>, Self::Error> {
            self.opens.set(self.opens.get() + 1);
            let frames = self
                .frames
                .iter()
                .cloned()
                .map(|mut frame| {
                    match &mut frame {
                        RpcStreamFrame::Data { id, .. }
                        | RpcStreamFrame::End { id }
                        | RpcStreamFrame::RemoteError { id, .. }
                        | RpcStreamFrame::Cancel { id }
                        | RpcStreamFrame::Call { id } => *id = call.id.clone(),
                    }
                    Ok(frame)
                })
                .collect::<Vec<_>>();
            Ok(RpcStreamSession::new(frames.into_iter()))
        }
    }

    #[test]
    fn prepare_is_inert_until_stream() {
        let opens = Rc::new(Cell::new(0));
        let client = OresRpcStreamClient::new(
            TestStream {
                opens: opens.clone(),
                frames: vec![
                    RpcStreamFrame::Data {
                        id: String::new(),
                        body: serde_json::json!({"value": 7}),
                    },
                    RpcStreamFrame::End { id: String::new() },
                ],
            },
            ["demo.watch_stream"],
        )
        .with_id_prefix("s-");

        let builder = client
            .prepare(
                "demo.watch_stream",
                RpcStreamRequest {
                    method: "GET".to_owned(),
                    path: "/v1/watch".to_owned(),
                    ..RpcStreamRequest::default()
                },
                |value| {
                    value
                        .get("value")
                        .and_then(serde_json::Value::as_i64)
                        .ok_or_else(|| "missing value".to_owned())
                },
            )
            .expect("prepare");

        assert_eq!(opens.get(), 0);
        let mut stream = builder.stream().expect("stream");
        assert_eq!(opens.get(), 1);
        assert_eq!(stream.next().expect("item").expect("decoded"), 7);
        assert!(stream.next().is_none());
        assert!(stream.context().ended);
    }
}
