import gleam/option.{type Option}

pub type Carrier {
  Websocket
  Tcp
}

pub type Frame(a) {
  Data(id: String, body: a)
  End(id: String)
  RemoteError(id: String, code: String, message: Option(String))
  Cancel(id: String)
  Call(id: String)
}

pub type StreamCall {
  StreamCall(id: String, key: String, method: String, path: String)
}

pub type Session(a) {
  Session(
    next: fn() -> Result(Option(Frame(a)), String),
    cancel: fn() -> Result(Nil, String),
  )
}

pub type Transport(a) {
  Transport(carrier: Carrier, open: fn(StreamCall) -> Result(Session(a), String))
}

pub type StreamContext {
  StreamContext(
    id: String,
    key: String,
    carrier: Carrier,
    ended: Bool,
    cancelled: Bool,
  )
}

pub type StreamClient(a) {
  StreamClient(
    session: Session(a),
    id: String,
    key: String,
    carrier: Carrier,
  )
}

pub type StreamBuilder(a) {
  StreamBuilder(
    transport: Transport(a),
    id: String,
    key: String,
    method: String,
    path: String,
  )
}

pub type StreamStep(a) {
  Item(value: a, client: StreamClient(a))
  Finished(context: StreamContext)
}

pub fn prepare(
  transport: Transport(a),
  id: String,
  key: String,
  method: String,
  path: String,
) -> StreamBuilder(a) {
  StreamBuilder(transport, id, key, method, path)
}

/// Sole transport-open boundary. Preparing a builder is pure and cannot perform I/O.
pub fn stream(builder: StreamBuilder(a)) -> Result(StreamClient(a), String) {
  let StreamBuilder(transport, id, key, method, path) = builder
  let Transport(carrier, open) = transport
  use session <- result.try(open(StreamCall(id, key, method, path)))
  Ok(StreamClient(session, id, key, carrier))
}

pub fn next(client: StreamClient(a)) -> Result(StreamStep(a), String) {
  let StreamClient(session, id, key, carrier) = client
  let Session(read, _) = session
  use frame <- result.try(read())
  case frame {
    None ->
      Error("stream transport ended without an end or cancel frame")
    Some(Data(frame_id, body)) ->
      case frame_id == id {
        True -> Ok(Item(body, client))
        False -> correlation_error(frame_id, id)
      }
    Some(End(frame_id)) ->
      terminal(frame_id, id, StreamContext(id, key, carrier, True, False))
    Some(Cancel(frame_id)) ->
      terminal(frame_id, id, StreamContext(id, key, carrier, False, True))
    Some(RemoteError(frame_id, code, message)) ->
      case frame_id == id {
        False -> correlation_error(frame_id, id)
        True ->
          Error(
            "remote RPC stream error "
            <> code
            <> case message {
              None -> ""
              Some(value) -> ": " <> value
            },
          )
      }
    Some(Call(_)) ->
      Error("a call frame cannot arrive inside its response stream")
  }
}

fn terminal(
  frame_id: String,
  expected_id: String,
  context: StreamContext,
) -> Result(StreamStep(a), String) {
  case frame_id == expected_id {
    True -> Ok(Finished(context))
    False -> correlation_error(frame_id, expected_id)
  }
}

fn correlation_error(frame_id: String, expected_id: String) -> Result(a, String) {
  Error(
    "frame for correlation id "
    <> frame_id
    <> " arrived on the stream for "
    <> expected_id,
  )
}

pub fn cancel(client: StreamClient(a)) -> Result(StreamContext, String) {
  let StreamClient(session, id, key, carrier) = client
  let Session(_, do_cancel) = session
  use _ <- result.try(do_cancel())
  Ok(StreamContext(id, key, carrier, False, True))
}
