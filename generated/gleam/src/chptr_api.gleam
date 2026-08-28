//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "chptr-api-server"

pub type RouteKey {
  Healthz
  GetChapter
  TransitionChapter
}

pub fn all() -> List(RouteKey) {
  [Healthz, GetChapter, TransitionChapter]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    GetChapter -> "get_chapter"
    TransitionChapter -> "transition_chapter"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "get_chapter" -> Ok(GetChapter)
    "transition_chapter" -> Ok(TransitionChapter)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    GetChapter -> "/v1/chapters/{chapterId}"
    TransitionChapter -> "/v1/chapters/{chapterId}/transitions"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    GetChapter -> ["GET"]
    TransitionChapter -> ["POST"]
  }
}

pub fn transports(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["http"]
    GetChapter -> ["http"]
    TransitionChapter -> ["http"]
  }
}
