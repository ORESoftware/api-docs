//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "hhm-api-server"

pub type RouteKey {
  Healthz
  ListReservations
  CreateReservation
  GetReservation
  Websocket
}

pub fn all() -> List(RouteKey) {
  [Healthz, ListReservations, CreateReservation, GetReservation, Websocket]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    ListReservations -> "list_reservations"
    CreateReservation -> "create_reservation"
    GetReservation -> "get_reservation"
    Websocket -> "websocket"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "list_reservations" -> Ok(ListReservations)
    "create_reservation" -> Ok(CreateReservation)
    "get_reservation" -> Ok(GetReservation)
    "websocket" -> Ok(Websocket)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    ListReservations -> "/api/v1/reservations"
    CreateReservation -> "/api/v1/reservations"
    GetReservation -> "/api/v1/reservations/{id}"
    Websocket -> "/ws"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    ListReservations -> ["GET"]
    CreateReservation -> ["POST"]
    GetReservation -> ["GET"]
    Websocket -> ["GET"]
  }
}
