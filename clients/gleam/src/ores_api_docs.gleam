/// Gleam authors route keys as **function types**, **parameter types**,
/// **return types**, or a combination. There are no annotations; the type
/// *is* the route identity. Serialize to the JSON map for other languages.
import gleam/dict.{type Dict}
import gleam/list
import gleam/string

/// `fn(req) -> res` — param type is the request, return type is the response.
pub type Unary(req, res) =
  fn(req) -> res

pub type RouteEntry {
  RouteEntry(path: String, methods: List(String))
}

pub type RouteMap {
  RouteMap(schema_version: String, service: String, map: Dict(String, RouteEntry))
}

pub fn lookup(routes: RouteMap, key: String) -> Result(RouteEntry, Nil) {
  dict.get(routes.map, key)
}

pub fn infer_methods(key: String) -> List(String) {
  case string.first(key) {
    Ok(c) -> {
      let upper = string.uppercase(c)
      let lower = string.lowercase(c)
      case upper != lower && c == upper {
        True -> ["POST"]
        False -> infer_rest(key)
      }
    }
    Error(_) -> ["GET"]
  }
}

fn infer_rest(key: String) -> List(String) {
  let lower = string.lowercase(key)
  case string.starts_with(lower, "delete") {
    True -> ["DELETE"]
    False -> case string.starts_with(lower, "put") || string.starts_with(lower, "update") || string.starts_with(lower, "replace") {
      True -> ["PUT"]
      False -> case string.starts_with(lower, "patch") {
        True -> ["PATCH"]
        False -> case contains_any(lower, ["create", "walk", "check", "ask"]) || string.starts_with(lower, "post") || string.starts_with(lower, "submit") {
          True -> ["POST"]
          False -> ["GET"]
        }
      }
    }
  }
}

fn contains_any(haystack: String, needles: List(String)) -> Bool {
  list.any(needles, fn(n) { string.contains(haystack, n) })
}
