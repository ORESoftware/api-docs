//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "example-rpc"

pub type RouteKey {
  Healthz
  GetItem
  Websocket
  TcpPing
}

pub fn all() -> List(RouteKey) {
  [Healthz, GetItem, Websocket, TcpPing]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    GetItem -> "get_item"
    Websocket -> "websocket"
    TcpPing -> "tcp_ping"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "get_item" -> Ok(GetItem)
    "websocket" -> Ok(Websocket)
    "tcp_ping" -> Ok(TcpPing)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    GetItem -> "/v1/items/{id}"
    Websocket -> "/ws"
    TcpPing -> "/rpc/ping"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    GetItem -> ["GET"]
    Websocket -> ["GET"]
    TcpPing -> ["POST"]
  }
}

pub fn transports(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["http"]
    GetItem -> ["http", "tcp", "websocket"]
    Websocket -> ["websocket"]
    TcpPing -> ["tcp"]
  }
}
