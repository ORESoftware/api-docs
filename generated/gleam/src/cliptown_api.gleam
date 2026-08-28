//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = "cliptown-api-server"

pub type RouteKey {
  Healthz
  Readyz
  ListClips
  CreateClip
  PutClip
  DeleteClip
  SyncPush
  SyncPull
  AppVaultSyncPush
  AppVaultSyncPull
  Search
  RagCandidates
  RegisterDevice
  DeleteDevice
  GetSecuritySettings
  PutSecuritySettings
}

pub fn all() -> List(RouteKey) {
  [Healthz, Readyz, ListClips, CreateClip, PutClip, DeleteClip, SyncPush, SyncPull, AppVaultSyncPush, AppVaultSyncPull, Search, RagCandidates, RegisterDevice, DeleteDevice, GetSecuritySettings, PutSecuritySettings]
}

pub fn to_string(key: RouteKey) -> String {
  case key {
    Healthz -> "healthz"
    Readyz -> "readyz"
    ListClips -> "list_clips"
    CreateClip -> "create_clip"
    PutClip -> "put_clip"
    DeleteClip -> "delete_clip"
    SyncPush -> "sync_push"
    SyncPull -> "sync_pull"
    AppVaultSyncPush -> "app_vault_sync_push"
    AppVaultSyncPull -> "app_vault_sync_pull"
    Search -> "search"
    RagCandidates -> "rag_candidates"
    RegisterDevice -> "register_device"
    DeleteDevice -> "delete_device"
    GetSecuritySettings -> "get_security_settings"
    PutSecuritySettings -> "put_security_settings"
  }
}

pub fn parse(key: String) -> Result(RouteKey, Nil) {
  case key {
    "healthz" -> Ok(Healthz)
    "readyz" -> Ok(Readyz)
    "list_clips" -> Ok(ListClips)
    "create_clip" -> Ok(CreateClip)
    "put_clip" -> Ok(PutClip)
    "delete_clip" -> Ok(DeleteClip)
    "sync_push" -> Ok(SyncPush)
    "sync_pull" -> Ok(SyncPull)
    "app_vault_sync_push" -> Ok(AppVaultSyncPush)
    "app_vault_sync_pull" -> Ok(AppVaultSyncPull)
    "search" -> Ok(Search)
    "rag_candidates" -> Ok(RagCandidates)
    "register_device" -> Ok(RegisterDevice)
    "delete_device" -> Ok(DeleteDevice)
    "get_security_settings" -> Ok(GetSecuritySettings)
    "put_security_settings" -> Ok(PutSecuritySettings)
    _ -> Error(Nil)
  }
}

pub fn path(key: RouteKey) -> String {
  case key {
    Healthz -> "/healthz"
    Readyz -> "/readyz"
    ListClips -> "/v1/clips"
    CreateClip -> "/v1/clips"
    PutClip -> "/v1/clips/{clipId}"
    DeleteClip -> "/v1/clips/{clipId}"
    SyncPush -> "/v1/sync/push"
    SyncPull -> "/v1/sync/pull"
    AppVaultSyncPush -> "/v1/app-vault/{appId}/sync/push"
    AppVaultSyncPull -> "/v1/app-vault/{appId}/sync/pull"
    Search -> "/v1/search"
    RagCandidates -> "/v1/rag/candidates"
    RegisterDevice -> "/v1/devices"
    DeleteDevice -> "/v1/devices/{deviceId}"
    GetSecuritySettings -> "/v1/settings/security"
    PutSecuritySettings -> "/v1/settings/security"
  }
}

pub fn methods(key: RouteKey) -> List(String) {
  case key {
    Healthz -> ["GET"]
    Readyz -> ["GET"]
    ListClips -> ["GET"]
    CreateClip -> ["POST"]
    PutClip -> ["PUT"]
    DeleteClip -> ["DELETE"]
    SyncPush -> ["POST"]
    SyncPull -> ["POST"]
    AppVaultSyncPush -> ["POST"]
    AppVaultSyncPull -> ["POST"]
    Search -> ["POST"]
    RagCandidates -> ["POST"]
    RegisterDevice -> ["POST"]
    DeleteDevice -> ["DELETE"]
    GetSecuritySettings -> ["GET"]
    PutSecuritySettings -> ["PUT"]
  }
}
