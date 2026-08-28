//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "canonical-api-server";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    ListQuotes,
    CreateQuote,
    GetQuote,
    RetryQuote,
    QuoteEvents,
    ListReadinessFrameworks,
    GetReadinessFramework,
    ListReadinessAssessments,
    CreateReadinessAssessment,
    GetReadinessAssessment,
    SyncChanges,
    SyncMutations,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::ListQuotes, Self::CreateQuote, Self::GetQuote, Self::RetryQuote, Self::QuoteEvents, Self::ListReadinessFrameworks, Self::GetReadinessFramework, Self::ListReadinessAssessments, Self::CreateReadinessAssessment, Self::GetReadinessAssessment, Self::SyncChanges, Self::SyncMutations];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::ListQuotes => "list_quotes",
            Self::CreateQuote => "create_quote",
            Self::GetQuote => "get_quote",
            Self::RetryQuote => "retry_quote",
            Self::QuoteEvents => "quote_events",
            Self::ListReadinessFrameworks => "list_readiness_frameworks",
            Self::GetReadinessFramework => "get_readiness_framework",
            Self::ListReadinessAssessments => "list_readiness_assessments",
            Self::CreateReadinessAssessment => "create_readiness_assessment",
            Self::GetReadinessAssessment => "get_readiness_assessment",
            Self::SyncChanges => "sync_changes",
            Self::SyncMutations => "sync_mutations",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "list_quotes" => Some(Self::ListQuotes),
            "create_quote" => Some(Self::CreateQuote),
            "get_quote" => Some(Self::GetQuote),
            "retry_quote" => Some(Self::RetryQuote),
            "quote_events" => Some(Self::QuoteEvents),
            "list_readiness_frameworks" => Some(Self::ListReadinessFrameworks),
            "get_readiness_framework" => Some(Self::GetReadinessFramework),
            "list_readiness_assessments" => Some(Self::ListReadinessAssessments),
            "create_readiness_assessment" => Some(Self::CreateReadinessAssessment),
            "get_readiness_assessment" => Some(Self::GetReadinessAssessment),
            "sync_changes" => Some(Self::SyncChanges),
            "sync_mutations" => Some(Self::SyncMutations),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::ListQuotes => "/api/v1/quotes",
            Self::CreateQuote => "/api/v1/quotes",
            Self::GetQuote => "/api/v1/quotes/{quoteId}",
            Self::RetryQuote => "/api/v1/quotes/{quoteId}/retry",
            Self::QuoteEvents => "/api/v1/quotes/{quoteId}/events",
            Self::ListReadinessFrameworks => "/api/v1/readiness/frameworks",
            Self::GetReadinessFramework => "/api/v1/readiness/frameworks/{frameworkId}",
            Self::ListReadinessAssessments => "/api/v1/readiness/assessments",
            Self::CreateReadinessAssessment => "/api/v1/readiness/assessments",
            Self::GetReadinessAssessment => "/api/v1/readiness/assessments/{assessmentId}",
            Self::SyncChanges => "/api/v1/sync/changes",
            Self::SyncMutations => "/api/v1/sync/mutations",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::ListQuotes => &["GET"],
            Self::CreateQuote => &["POST"],
            Self::GetQuote => &["GET"],
            Self::RetryQuote => &["POST"],
            Self::QuoteEvents => &["GET"],
            Self::ListReadinessFrameworks => &["GET"],
            Self::GetReadinessFramework => &["GET"],
            Self::ListReadinessAssessments => &["GET"],
            Self::CreateReadinessAssessment => &["POST"],
            Self::GetReadinessAssessment => &["GET"],
            Self::SyncChanges => &["GET"],
            Self::SyncMutations => &["POST"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListQuotesQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListQuotesResponse {
    pub quotes: Vec<serde_json::Value>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateQuoteRequest {
    #[serde(rename = "organizationName")]
    pub organization_name: String,
    #[serde(rename = "contactEmail")]
    pub contact_email: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateQuoteResponse {
    #[serde(rename = "quoteId")]
    pub quote_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetQuotePath {
    #[serde(rename = "quoteId")]
    pub quote_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RetryQuotePath {
    #[serde(rename = "quoteId")]
    pub quote_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct QuoteEventsPath {
    #[serde(rename = "quoteId")]
    pub quote_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetReadinessFrameworkPath {
    #[serde(rename = "frameworkId")]
    pub framework_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetReadinessAssessmentPath {
    #[serde(rename = "assessmentId")]
    pub assessment_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncChangesQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncMutationsRequest {
    pub operations: Vec<serde_json::Value>,
}

