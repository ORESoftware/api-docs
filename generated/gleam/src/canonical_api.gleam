//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "canonical-api-server"

pub type RouteKey {
  Healthz
  ListQuotes
  CreateQuote
  GetQuote
  RetryQuote
  QuoteEvents
  ListReadinessFrameworks
  GetReadinessFramework
  ListReadinessAssessments
  CreateReadinessAssessment
  GetReadinessAssessment
  SyncChanges
  SyncMutations
}

pub fn all() -> List(RouteKey) {
  [Healthz, ListQuotes, CreateQuote, GetQuote, RetryQuote, QuoteEvents, ListReadinessFrameworks, GetReadinessFramework, ListReadinessAssessments, CreateReadinessAssessment, GetReadinessAssessment, SyncChanges, SyncMutations]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    ListQuotes -> "list_quotes"
    CreateQuote -> "create_quote"
    GetQuote -> "get_quote"
    RetryQuote -> "retry_quote"
    QuoteEvents -> "quote_events"
    ListReadinessFrameworks -> "list_readiness_frameworks"
    GetReadinessFramework -> "get_readiness_framework"
    ListReadinessAssessments -> "list_readiness_assessments"
    CreateReadinessAssessment -> "create_readiness_assessment"
    GetReadinessAssessment -> "get_readiness_assessment"
    SyncChanges -> "sync_changes"
    SyncMutations -> "sync_mutations"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "list_quotes" -> Ok(ListQuotes)
    "create_quote" -> Ok(CreateQuote)
    "get_quote" -> Ok(GetQuote)
    "retry_quote" -> Ok(RetryQuote)
    "quote_events" -> Ok(QuoteEvents)
    "list_readiness_frameworks" -> Ok(ListReadinessFrameworks)
    "get_readiness_framework" -> Ok(GetReadinessFramework)
    "list_readiness_assessments" -> Ok(ListReadinessAssessments)
    "create_readiness_assessment" -> Ok(CreateReadinessAssessment)
    "get_readiness_assessment" -> Ok(GetReadinessAssessment)
    "sync_changes" -> Ok(SyncChanges)
    "sync_mutations" -> Ok(SyncMutations)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    ListQuotes -> "/api/v1/quotes"
    CreateQuote -> "/api/v1/quotes"
    GetQuote -> "/api/v1/quotes/{quoteId}"
    RetryQuote -> "/api/v1/quotes/{quoteId}/retry"
    QuoteEvents -> "/api/v1/quotes/{quoteId}/events"
    ListReadinessFrameworks -> "/api/v1/readiness/frameworks"
    GetReadinessFramework -> "/api/v1/readiness/frameworks/{frameworkId}"
    ListReadinessAssessments -> "/api/v1/readiness/assessments"
    CreateReadinessAssessment -> "/api/v1/readiness/assessments"
    GetReadinessAssessment -> "/api/v1/readiness/assessments/{assessmentId}"
    SyncChanges -> "/api/v1/sync/changes"
    SyncMutations -> "/api/v1/sync/mutations"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    ListQuotes -> ["GET"]
    CreateQuote -> ["POST"]
    GetQuote -> ["GET"]
    RetryQuote -> ["POST"]
    QuoteEvents -> ["GET"]
    ListReadinessFrameworks -> ["GET"]
    GetReadinessFramework -> ["GET"]
    ListReadinessAssessments -> ["GET"]
    CreateReadinessAssessment -> ["POST"]
    GetReadinessAssessment -> ["GET"]
    SyncChanges -> ["GET"]
    SyncMutations -> ["POST"]
  }
}
