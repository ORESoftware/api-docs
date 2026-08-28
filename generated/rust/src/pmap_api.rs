//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "pmap-api-server";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    CreateMatter,
    GetMatter,
    WalkMatter,
    GetDocuments,
    GetFacts,
    Avenues,
    Geography,
    CheckFieldSanity,
    AskCounsel,
    CheckFieldSanityRest,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::CreateMatter, Self::GetMatter, Self::WalkMatter, Self::GetDocuments, Self::GetFacts, Self::Avenues, Self::Geography, Self::CheckFieldSanity, Self::AskCounsel, Self::CheckFieldSanityRest];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::CreateMatter => "create_matter",
            Self::GetMatter => "get_matter",
            Self::WalkMatter => "walk_matter",
            Self::GetDocuments => "get_documents",
            Self::GetFacts => "get_facts",
            Self::Avenues => "avenues",
            Self::Geography => "geography",
            Self::CheckFieldSanity => "CheckFieldSanity",
            Self::AskCounsel => "AskCounsel",
            Self::CheckFieldSanityRest => "check_field_sanity_rest",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "create_matter" => Some(Self::CreateMatter),
            "get_matter" => Some(Self::GetMatter),
            "walk_matter" => Some(Self::WalkMatter),
            "get_documents" => Some(Self::GetDocuments),
            "get_facts" => Some(Self::GetFacts),
            "avenues" => Some(Self::Avenues),
            "geography" => Some(Self::Geography),
            "CheckFieldSanity" => Some(Self::CheckFieldSanity),
            "AskCounsel" => Some(Self::AskCounsel),
            "check_field_sanity_rest" => Some(Self::CheckFieldSanityRest),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::CreateMatter => "/v1/matters",
            Self::GetMatter => "/v1/matters/{id}",
            Self::WalkMatter => "/v1/matters/{id}/walk",
            Self::GetDocuments => "/v1/matters/{id}/documents",
            Self::GetFacts => "/v1/matters/{id}/facts",
            Self::Avenues => "/v1/avenues",
            Self::Geography => "/v1/geography",
            Self::CheckFieldSanity => "/pmap.v1.Interview/CheckFieldSanity",
            Self::AskCounsel => "/pmap.v1.Interview/AskCounsel",
            Self::CheckFieldSanityRest => "/v1/fields/sanity",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::CreateMatter => &["POST"],
            Self::GetMatter => &["GET"],
            Self::WalkMatter => &["POST"],
            Self::GetDocuments => &["GET"],
            Self::GetFacts => &["GET"],
            Self::Avenues => &["GET"],
            Self::Geography => &["GET"],
            Self::CheckFieldSanity => &["POST"],
            Self::AskCounsel => &["POST"],
            Self::CheckFieldSanityRest => &["POST"],
        }
    }

    #[must_use]
    pub fn transports(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["http"],
            Self::CreateMatter => &["http"],
            Self::GetMatter => &["http"],
            Self::WalkMatter => &["http"],
            Self::GetDocuments => &["http"],
            Self::GetFacts => &["http"],
            Self::Avenues => &["http"],
            Self::Geography => &["http"],
            Self::CheckFieldSanity => &["http"],
            Self::AskCounsel => &["http"],
            Self::CheckFieldSanityRest => &["http"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateMatterResponse {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetMatterPath {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetMatterQuery {
    #[serde(rename = "include")]
    pub include_: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetMatterResponse {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct WalkMatterPath {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct WalkMatterRequest {
    pub choice_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetDocumentsPath {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetFactsPath {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CheckFieldSanityRequest {
    pub matter_id: Option<String>,
    pub node_id: Option<String>,
    pub fields: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CheckFieldSanityResponse {
    pub report: serde_json::Value,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AskCounselRequest {
    pub matter_id: String,
    pub question: Option<String>,
    pub scope: Option<String>,
    pub document: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AskCounselResponse {
    pub round_table: serde_json::Value,
    pub providers_configured: Vec<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CheckFieldSanityRestRequest {
    pub matter_id: Option<String>,
    pub node_id: Option<String>,
    pub fields: Vec<serde_json::Value>,
}

