import gleam/option
import gleeunit
import gleeunit/should
import ores_ridl_stream

pub fn main() {
  gleeunit.main()
}

fn transport_with(
  frame: option.Option(ores_ridl_stream.Frame(Int)),
) -> ores_ridl_stream.Transport(Int) {
  ores_ridl_stream.Transport(
    ores_ridl_stream.Websocket,
    fn(_) {
      Ok(
        ores_ridl_stream.Session(
          fn() { Ok(frame) },
          fn() { Ok(Nil) },
        ),
      )
    },
  )
}

pub fn prepare_is_deferred_until_stream_test() {
  let transport = ores_ridl_stream.Transport(
    ores_ridl_stream.Tcp,
    fn(_) { Error("open called") },
  )
  let builder =
    ores_ridl_stream.prepare(
      transport,
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  ores_ridl_stream.stream(builder)
  |> should.equal(Error("open called"))
}

pub fn stream_returns_client_and_typed_data_test() {
  let builder =
    ores_ridl_stream.prepare(
      transport_with(option.Some(ores_ridl_stream.Data("s-1", 7))),
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  let assert Ok(client) = ores_ridl_stream.stream(builder)
  let assert Ok(ores_ridl_stream.Item(value, _)) = ores_ridl_stream.next(client)
  value |> should.equal(7)
}

pub fn stream_rejects_correlation_mismatch_test() {
  let builder =
    ores_ridl_stream.prepare(
      transport_with(option.Some(ores_ridl_stream.End("wrong"))),
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  let assert Ok(client) = ores_ridl_stream.stream(builder)
  let assert Error(message) = ores_ridl_stream.next(client)
  message
  |> should.equal("frame for correlation id wrong arrived on the stream for s-1")
}

pub fn stream_end_returns_terminal_context_test() {
  let builder =
    ores_ridl_stream.prepare(
      transport_with(option.Some(ores_ridl_stream.End("s-1"))),
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  let assert Ok(client) = ores_ridl_stream.stream(builder)
  let assert Ok(ores_ridl_stream.Finished(context)) = ores_ridl_stream.next(client)
  context
  |> should.equal(
    ores_ridl_stream.StreamContext(
      "s-1",
      "demo.events.watch_stream",
      ores_ridl_stream.Websocket,
      True,
      False,
    ),
  )
}
