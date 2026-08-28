//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "gha-indie-worker"

pub type RouteKey {
  Healthz
  Readyz
  ListBuilds
  SubmitBuild
  GetBuild
  GetBuildLogs
  GetBuildArtifacts
  GithubWebhook
  RegistryWebhook
  SyncSecrets
  SyncSecretsStatus
}

pub fn all() -> List(RouteKey) {
  [Healthz, Readyz, ListBuilds, SubmitBuild, GetBuild, GetBuildLogs, GetBuildArtifacts, GithubWebhook, RegistryWebhook, SyncSecrets, SyncSecretsStatus]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    Readyz -> "readyz"
    ListBuilds -> "list_builds"
    SubmitBuild -> "submit_build"
    GetBuild -> "get_build"
    GetBuildLogs -> "get_build_logs"
    GetBuildArtifacts -> "get_build_artifacts"
    GithubWebhook -> "github_webhook"
    RegistryWebhook -> "registry_webhook"
    SyncSecrets -> "sync_secrets"
    SyncSecretsStatus -> "sync_secrets_status"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "readyz" -> Ok(Readyz)
    "list_builds" -> Ok(ListBuilds)
    "submit_build" -> Ok(SubmitBuild)
    "get_build" -> Ok(GetBuild)
    "get_build_logs" -> Ok(GetBuildLogs)
    "get_build_artifacts" -> Ok(GetBuildArtifacts)
    "github_webhook" -> Ok(GithubWebhook)
    "registry_webhook" -> Ok(RegistryWebhook)
    "sync_secrets" -> Ok(SyncSecrets)
    "sync_secrets_status" -> Ok(SyncSecretsStatus)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    Readyz -> "/readyz"
    ListBuilds -> "/builds"
    SubmitBuild -> "/builds"
    GetBuild -> "/builds/{job_id}"
    GetBuildLogs -> "/builds/{job_id}/logs"
    GetBuildArtifacts -> "/builds/{job_id}/artifacts"
    GithubWebhook -> "/webhooks/github"
    RegistryWebhook -> "/webhooks/registry"
    SyncSecrets -> "/secrets/sync"
    SyncSecretsStatus -> "/secrets/sync/status"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    Readyz -> ["GET"]
    ListBuilds -> ["GET"]
    SubmitBuild -> ["POST"]
    GetBuild -> ["GET"]
    GetBuildLogs -> ["GET"]
    GetBuildArtifacts -> ["GET"]
    GithubWebhook -> ["POST"]
    RegistryWebhook -> ["POST"]
    SyncSecrets -> ["POST"]
    SyncSecretsStatus -> ["GET"]
  }
}

pub fn transports(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["http"]
    Readyz -> ["http"]
    ListBuilds -> ["http"]
    SubmitBuild -> ["http"]
    GetBuild -> ["http"]
    GetBuildLogs -> ["http"]
    GetBuildArtifacts -> ["http"]
    GithubWebhook -> ["http"]
    RegistryWebhook -> ["http"]
    SyncSecrets -> ["http"]
    SyncSecretsStatus -> ["http"]
  }
}
