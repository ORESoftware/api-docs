//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = "gha-indie-worker";

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {
    Healthz,
    Readyz,
    ListBuilds,
    SubmitBuild,
    GetBuild,
    GetBuildLogs,
    GetBuildArtifacts,
    GithubWebhook,
    RegistryWebhook,
    SyncSecrets,
    SyncSecretsStatus,
}

impl RouteKey {
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::Readyz, Self::ListBuilds, Self::SubmitBuild, Self::GetBuild, Self::GetBuildLogs, Self::GetBuildArtifacts, Self::GithubWebhook, Self::RegistryWebhook, Self::SyncSecrets, Self::SyncSecretsStatus];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthz => "healthz",
            Self::Readyz => "readyz",
            Self::ListBuilds => "list_builds",
            Self::SubmitBuild => "submit_build",
            Self::GetBuild => "get_build",
            Self::GetBuildLogs => "get_build_logs",
            Self::GetBuildArtifacts => "get_build_artifacts",
            Self::GithubWebhook => "github_webhook",
            Self::RegistryWebhook => "registry_webhook",
            Self::SyncSecrets => "sync_secrets",
            Self::SyncSecretsStatus => "sync_secrets_status",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "healthz" => Some(Self::Healthz),
            "readyz" => Some(Self::Readyz),
            "list_builds" => Some(Self::ListBuilds),
            "submit_build" => Some(Self::SubmitBuild),
            "get_build" => Some(Self::GetBuild),
            "get_build_logs" => Some(Self::GetBuildLogs),
            "get_build_artifacts" => Some(Self::GetBuildArtifacts),
            "github_webhook" => Some(Self::GithubWebhook),
            "registry_webhook" => Some(Self::RegistryWebhook),
            "sync_secrets" => Some(Self::SyncSecrets),
            "sync_secrets_status" => Some(Self::SyncSecretsStatus),
            _ => None,
        }
    }

    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Healthz => "/healthz",
            Self::Readyz => "/readyz",
            Self::ListBuilds => "/builds",
            Self::SubmitBuild => "/builds",
            Self::GetBuild => "/builds/{job_id}",
            Self::GetBuildLogs => "/builds/{job_id}/logs",
            Self::GetBuildArtifacts => "/builds/{job_id}/artifacts",
            Self::GithubWebhook => "/webhooks/github",
            Self::RegistryWebhook => "/webhooks/registry",
            Self::SyncSecrets => "/secrets/sync",
            Self::SyncSecretsStatus => "/secrets/sync/status",
        }
    }

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["GET"],
            Self::Readyz => &["GET"],
            Self::ListBuilds => &["GET"],
            Self::SubmitBuild => &["POST"],
            Self::GetBuild => &["GET"],
            Self::GetBuildLogs => &["GET"],
            Self::GetBuildArtifacts => &["GET"],
            Self::GithubWebhook => &["POST"],
            Self::RegistryWebhook => &["POST"],
            Self::SyncSecrets => &["POST"],
            Self::SyncSecretsStatus => &["GET"],
        }
    }

    #[must_use]
    pub fn transports(self) -> &'static [&'static str] {
        match self {
            Self::Healthz => &["http"],
            Self::Readyz => &["http"],
            Self::ListBuilds => &["http"],
            Self::SubmitBuild => &["http"],
            Self::GetBuild => &["http"],
            Self::GetBuildLogs => &["http"],
            Self::GetBuildArtifacts => &["http"],
            Self::GithubWebhook => &["http"],
            Self::RegistryWebhook => &["http"],
            Self::SyncSecrets => &["http"],
            Self::SyncSecretsStatus => &["http"],
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SubmitBuildRequest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: Option<String>,
    #[serde(rename = "jobKind")]
    pub job_kind: Option<String>,
    #[serde(rename = "repoUrl")]
    pub repo_url: String,
    #[serde(rename = "gitRef")]
    pub git_ref: Option<String>,
    pub image: Option<String>,
    pub profile: Option<String>,
    #[serde(rename = "contextDir")]
    pub context_dir: Option<String>,
    pub dockerfile: Option<String>,
    pub push: Option<bool>,
    pub executor: Option<String>,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetBuildPath {
    pub job_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetBuildLogsPath {
    pub job_id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GetBuildArtifactsPath {
    pub job_id: String,
}

