//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "pmap-api-server"

pub type RouteKey {
  Healthz
  CreateMatter
  GetMatter
  WalkMatter
  GetDocuments
  GetFacts
  Avenues
  Geography
  CheckFieldSanity
  AskCounsel
  CheckFieldSanityRest
}

pub fn all() -> List(RouteKey) {
  [Healthz, CreateMatter, GetMatter, WalkMatter, GetDocuments, GetFacts, Avenues, Geography, CheckFieldSanity, AskCounsel, CheckFieldSanityRest]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    CreateMatter -> "create_matter"
    GetMatter -> "get_matter"
    WalkMatter -> "walk_matter"
    GetDocuments -> "get_documents"
    GetFacts -> "get_facts"
    Avenues -> "avenues"
    Geography -> "geography"
    CheckFieldSanity -> "CheckFieldSanity"
    AskCounsel -> "AskCounsel"
    CheckFieldSanityRest -> "check_field_sanity_rest"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "create_matter" -> Ok(CreateMatter)
    "get_matter" -> Ok(GetMatter)
    "walk_matter" -> Ok(WalkMatter)
    "get_documents" -> Ok(GetDocuments)
    "get_facts" -> Ok(GetFacts)
    "avenues" -> Ok(Avenues)
    "geography" -> Ok(Geography)
    "CheckFieldSanity" -> Ok(CheckFieldSanity)
    "AskCounsel" -> Ok(AskCounsel)
    "check_field_sanity_rest" -> Ok(CheckFieldSanityRest)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    CreateMatter -> "/v1/matters"
    GetMatter -> "/v1/matters/{id}"
    WalkMatter -> "/v1/matters/{id}/walk"
    GetDocuments -> "/v1/matters/{id}/documents"
    GetFacts -> "/v1/matters/{id}/facts"
    Avenues -> "/v1/avenues"
    Geography -> "/v1/geography"
    CheckFieldSanity -> "/pmap.v1.Interview/CheckFieldSanity"
    AskCounsel -> "/pmap.v1.Interview/AskCounsel"
    CheckFieldSanityRest -> "/v1/fields/sanity"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    CreateMatter -> ["POST"]
    GetMatter -> ["GET"]
    WalkMatter -> ["POST"]
    GetDocuments -> ["GET"]
    GetFacts -> ["GET"]
    Avenues -> ["GET"]
    Geography -> ["GET"]
    CheckFieldSanity -> ["POST"]
    AskCounsel -> ["POST"]
    CheckFieldSanityRest -> ["POST"]
  }
}
