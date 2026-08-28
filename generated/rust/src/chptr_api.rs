//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "chptr-api-server";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    GetChapter,
    TransitionChapter,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::GetChapter, Self::TransitionChapter];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::GetChapter => "get_chapter",
            Self::TransitionChapter => "transition_chapter",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "get_chapter" => Some(Self::GetChapter),
            "transition_chapter" => Some(Self::TransitionChapter),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::GetChapter => "/v1/chapters/{chapterId}",
            Self::TransitionChapter => "/v1/chapters/{chapterId}/transitions",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::GetChapter => &["GET"],
            Self::TransitionChapter => &["POST"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetChapterPath {
    #[serde(rename = "chapterId")]
    pub chapter_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetChapterResponse {
    pub id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TransitionChapterPath {
    #[serde(rename = "chapterId")]
    pub chapter_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TransitionChapterRequest {
    pub to: String,
    pub revision: Option<String>,
}

