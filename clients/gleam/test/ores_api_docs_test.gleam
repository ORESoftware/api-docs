import gleam/dict
import gleam/option
import gleeunit
import gleeunit/should
import ores_api_docs.{RouteEntry, RouteMap}
import ores_api_docs/stream_rpc as stream_rpc

pub fn main() {
  gleeunit.main()
}

pub fn pascal_case_is_connect_post_test() {
  ores_api_docs.infer_methods("CheckFieldSanity")
  |> should.equal(["POST"])
  ores_api_docs.infer_methods("healthz")
  |> should.equal(["GET"])
  ores_api_docs.infer_methods("create_matter")
  |> should.equal(["POST"])
}

pub fn lookup_by_key_test() {
  let routes =
    RouteMap(
      schema_version: "1.0.0",
      service: "pmap-api-server",
      map: dict.from_list([
        #(
          "CheckFieldSanity",
          RouteEntry(
            path: "/pmap.v1.Interview/CheckFieldSanity",
            methods: ["POST"],
            transports: ["http"],
          ),
        ),
      ]),
    )
  ores_api_docs.lookup(routes, "CheckFieldSanity")
  |> should.be_ok
}

pub fn infer_transports_http_and_websocket_test() {
  ores_api_docs.infer_transports("healthz", "/healthz")
  |> should.equal(["http"])
  ores_api_docs.infer_transports("websocket", "/ws")
  |> should.equal(["websocket"])
  ores_api_docs.infer_transports("get_item", "/v1/items/{id}")
  |> should.equal(["http"])
}

/// Return type + param type *are* the route; no annotation needed.
pub fn unary_function_type_test() {
  let handler: ores_api_docs.Unary(String, String) = fn(req) { req }
  handler("ok")
  |> should.equal("ok")
}


fn stream_transport_with(
  frame: option.Option(stream_rpc.Frame(Int)),
) -> stream_rpc.Transport(Int) {
  stream_rpc.Transport(
    stream_rpc.Websocket,
    fn(_) {
      Ok(
        stream_rpc.Session(
          fn() { Ok(frame) },
          fn() { Ok(Nil) },
        ),
      )
    },
  )
}

pub fn stream_prepare_is_deferred_until_stream_test() {
  let transport = stream_rpc.Transport(
    stream_rpc.Tcp,
    fn(_) { Error("open called") },
  )
  let builder =
    stream_rpc.prepare(
      transport,
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  stream_rpc.stream(builder)
  |> should.equal(Error("open called"))
}

pub fn stream_returns_typed_data_test() {
  let builder =
    stream_rpc.prepare(
      stream_transport_with(option.Some(stream_rpc.Data("s-1", 7))),
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  let assert Ok(client) = stream_rpc.stream(builder)
  let assert Ok(stream_rpc.Item(value, _)) = stream_rpc.next(client)
  value |> should.equal(7)
}

pub fn stream_rejects_correlation_mismatch_test() {
  let builder =
    stream_rpc.prepare(
      stream_transport_with(option.Some(stream_rpc.End("wrong"))),
      "s-1",
      "demo.events.watch_stream",
      "GET",
      "/v1/events",
    )
  let assert Ok(client) = stream_rpc.stream(builder)
  let assert Error(message) = stream_rpc.next(client)
  message
  |> should.equal("frame for correlation id wrong arrived on the stream for s-1")
}
