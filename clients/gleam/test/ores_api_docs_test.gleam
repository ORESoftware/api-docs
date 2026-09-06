import gleam/dict
import gleeunit
import gleeunit/should
import ores_api_docs.{RouteEntry, RouteMap}

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
