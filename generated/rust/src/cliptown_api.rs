//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "cliptown-api-server";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    Readyz,
    ListClips,
    CreateClip,
    PutClip,
    DeleteClip,
    SyncPush,
    SyncPull,
    AppVaultSyncPush,
    AppVaultSyncPull,
    Search,
    RagCandidates,
    RegisterDevice,
    DeleteDevice,
    GetSecuritySettings,
    PutSecuritySettings,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::Readyz, Self::ListClips, Self::CreateClip, Self::PutClip, Self::DeleteClip, Self::SyncPush, Self::SyncPull, Self::AppVaultSyncPush, Self::AppVaultSyncPull, Self::Search, Self::RagCandidates, Self::RegisterDevice, Self::DeleteDevice, Self::GetSecuritySettings, Self::PutSecuritySettings];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::Readyz => "readyz",
            Self::ListClips => "list_clips",
            Self::CreateClip => "create_clip",
            Self::PutClip => "put_clip",
            Self::DeleteClip => "delete_clip",
            Self::SyncPush => "sync_push",
            Self::SyncPull => "sync_pull",
            Self::AppVaultSyncPush => "app_vault_sync_push",
            Self::AppVaultSyncPull => "app_vault_sync_pull",
            Self::Search => "search",
            Self::RagCandidates => "rag_candidates",
            Self::RegisterDevice => "register_device",
            Self::DeleteDevice => "delete_device",
            Self::GetSecuritySettings => "get_security_settings",
            Self::PutSecuritySettings => "put_security_settings",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "readyz" => Some(Self::Readyz),
            "list_clips" => Some(Self::ListClips),
            "create_clip" => Some(Self::CreateClip),
            "put_clip" => Some(Self::PutClip),
            "delete_clip" => Some(Self::DeleteClip),
            "sync_push" => Some(Self::SyncPush),
            "sync_pull" => Some(Self::SyncPull),
            "app_vault_sync_push" => Some(Self::AppVaultSyncPush),
            "app_vault_sync_pull" => Some(Self::AppVaultSyncPull),
            "search" => Some(Self::Search),
            "rag_candidates" => Some(Self::RagCandidates),
            "register_device" => Some(Self::RegisterDevice),
            "delete_device" => Some(Self::DeleteDevice),
            "get_security_settings" => Some(Self::GetSecuritySettings),
            "put_security_settings" => Some(Self::PutSecuritySettings),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::Readyz => "/readyz",
            Self::ListClips => "/v1/clips",
            Self::CreateClip => "/v1/clips",
            Self::PutClip => "/v1/clips/{clipId}",
            Self::DeleteClip => "/v1/clips/{clipId}",
            Self::SyncPush => "/v1/sync/push",
            Self::SyncPull => "/v1/sync/pull",
            Self::AppVaultSyncPush => "/v1/app-vault/{appId}/sync/push",
            Self::AppVaultSyncPull => "/v1/app-vault/{appId}/sync/pull",
            Self::Search => "/v1/search",
            Self::RagCandidates => "/v1/rag/candidates",
            Self::RegisterDevice => "/v1/devices",
            Self::DeleteDevice => "/v1/devices/{deviceId}",
            Self::GetSecuritySettings => "/v1/settings/security",
            Self::PutSecuritySettings => "/v1/settings/security",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::Readyz => &["GET"],
            Self::ListClips => &["GET"],
            Self::CreateClip => &["POST"],
            Self::PutClip => &["PUT"],
            Self::DeleteClip => &["DELETE"],
            Self::SyncPush => &["POST"],
            Self::SyncPull => &["POST"],
            Self::AppVaultSyncPush => &["POST"],
            Self::AppVaultSyncPull => &["POST"],
            Self::Search => &["POST"],
            Self::RagCandidates => &["POST"],
            Self::RegisterDevice => &["POST"],
            Self::DeleteDevice => &["DELETE"],
            Self::GetSecuritySettings => &["GET"],
            Self::PutSecuritySettings => &["PUT"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListClipsQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ListClipsResponse {
    pub items: Vec<serde_json::Value>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CreateClipRequest {
    pub clip_id: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub pinned: Option<bool>,
    pub deleted: Option<bool>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PutClipPath {
    #[serde(rename = "clipId")]
    pub clip_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DeleteClipPath {
    #[serde(rename = "clipId")]
    pub clip_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncPushRequest {
    pub mutations: Vec<serde_json::Value>,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncPullRequest {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AppVaultSyncPushPath {
    #[serde(rename = "appId")]
    pub app_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AppVaultSyncPushRequest {
    pub mutations: Vec<serde_json::Value>,
    pub base: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AppVaultSyncPullPath {
    #[serde(rename = "appId")]
    pub app_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AppVaultSyncPullRequest {
    pub after: Option<serde_json::Value>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SearchRequest {
    pub privacy_mode: String,
    pub blind_terms: Option<Vec<String>>,
    pub limit: Option<i64>,
    pub pinned_only: Option<bool>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RagCandidatesRequest {
    pub privacy_mode: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RegisterDeviceRequest {
    pub device_name: String,
    pub platform: String,
    pub encryption_public_key: String,
    pub signing_public_key: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DeleteDevicePath {
    #[serde(rename = "deviceId")]
    pub device_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetSecuritySettingsResponse {
    pub reauth_interval_days: i64,
    pub reauth_max_days: i64,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PutSecuritySettingsRequest {
    pub reauth_interval_days: i64,
}

