//! Normalized typed RPC operation IR.
//!
//! `route.rs` remains the implementation authority while TypeSpec and authored
//! JSON Schema remain peer authorities for the wire shapes. This module joins
//! those two facts into one deterministic object consumed by SDK generators.
//! It deliberately describes semantic HTTP request/response metadata separately
//! from the `/v1/rpc` transport so clients cannot choose a different HTTP verb
//! or REST path for a generated operation.

use serde::Serialize;
use serde_json::Value;

use crate::{contract_sha256, RouteEntry, RouteMap};

pub const RPC_V1_HTTP_PATH: &str = "/v1/rpc";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcPayloadCodec {
    Json,
    Protobuf,
    Messagepack,
}

impl RpcPayloadCodec {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Protobuf => "protobuf",
            Self::Messagepack => "messagepack",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcClientAudience {
    Browser,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcOperationScope {
    Regular,
    Admin,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RpcOperationSource {
    pub route_file: String,
    pub handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RpcHttpProjection {
    pub method: String,
    pub path: String,
    pub rpc_transport_path: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RpcCodecSet {
    pub allowed: Vec<RpcPayloadCodec>,
    pub default: RpcPayloadCodec,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RpcRequestShape {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_schema: Option<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RpcResponseShape {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailer_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_schema: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RpcOperationContract {
    pub schema_version: u32,
    pub operation_key: String,
    pub namespace: Vec<String>,
    pub source: RpcOperationSource,
    pub http: RpcHttpProjection,
    pub scope: RpcOperationScope,
    pub audiences: Vec<RpcClientAudience>,
    pub codecs: RpcCodecSet,
    pub request: RpcRequestShape,
    pub response: RpcResponseShape,
    pub contract_sha256: String,
}

/// Convert one normalized route-map operation into the codegen IR.
///
/// New sliced SDKs require a stable dotted `rpc_key`; legacy map keys can keep
/// using the older generic client surface until they declare one. This avoids
/// silently inventing a wire identity during migration.
pub fn rpc_operation_contract(
    map: &RouteMap,
    route_key: &str,
    scope: RpcOperationScope,
    repository: Option<&str>,
    commit_sha: Option<&str>,
) -> Result<RpcOperationContract, String> {
    let entry = map
        .lookup(route_key)
        .ok_or_else(|| format!("unknown route-map operation {route_key:?}"))?;
    let operation_key = entry.rpc_key.clone().ok_or_else(|| {
        format!("{route_key}: typed namespace SDK generation requires stable dotted rpc_key")
    })?;
    let mut segments = operation_key
        .split('.')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return Err(format!(
            "{route_key}: rpc_key {operation_key:?} must contain a namespace and operation name"
        ));
    }
    segments.pop();

    if entry.methods.len() != 1 {
        return Err(format!(
            "{route_key}: typed RPC operation must project exactly one HTTP method, found {:?}",
            entry.methods
        ));
    }
    let method = entry.methods[0].clone();
    let handler = method.to_ascii_lowercase();
    let route_file = route_source(map, route_key, entry).ok_or_else(|| {
        format!(
            "{route_key}: typed RPC generation needs route.rs source identity in files or binding.file"
        )
    })?;
    if !route_file.ends_with("route.rs") {
        return Err(format!(
            "{route_key}: RPC implementation authority must be route.rs, got {route_file:?}"
        ));
    }

    let audiences = audiences_for(entry, scope);
    Ok(RpcOperationContract {
        schema_version: 1,
        operation_key,
        namespace: segments,
        source: RpcOperationSource {
            route_file,
            handler,
            repository: repository.map(str::to_owned),
            commit_sha: commit_sha.map(str::to_owned),
        },
        http: RpcHttpProjection {
            method,
            path: entry.path.clone(),
            rpc_transport_path: RPC_V1_HTTP_PATH,
        },
        scope,
        audiences,
        // JSON is the compatibility baseline. Route-level attributes will
        // narrow/extend this set when codec metadata lands in the route map.
        codecs: RpcCodecSet {
            allowed: vec![RpcPayloadCodec::Json],
            default: RpcPayloadCodec::Json,
        },
        request: RpcRequestShape {
            path_schema: entry.path_params.clone(),
            query_schema: entry.query_schema.clone(),
            header_schema: entry.header_schema.clone(),
            body_schema: entry.request_schema.clone(),
        },
        response: RpcResponseShape {
            header_schema: None,
            trailer_schema: None,
            body_schema: entry.response_schema.clone(),
            error_schema: entry.error_schema.clone(),
        },
        contract_sha256: contract_sha256(map),
    })
}

/// Generate only operations that have opted into stable dotted `rpc_key`s.
/// This makes migration incremental and prevents one legacy route from blocking
/// typed SDK generation for an otherwise modern namespace.
#[must_use]
pub fn rpc_operation_contracts(
    map: &RouteMap,
    scope: RpcOperationScope,
    repository: Option<&str>,
    commit_sha: Option<&str>,
) -> Vec<Result<RpcOperationContract, String>> {
    map.map
        .keys()
        .filter(|key| {
            map.lookup(key)
                .and_then(|entry| entry.rpc_key.as_ref())
                .is_some()
        })
        .map(|key| rpc_operation_contract(map, key, scope, repository, commit_sha))
        .collect()
}

fn route_source(map: &RouteMap, key: &str, entry: &RouteEntry) -> Option<String> {
    entry
        .binding
        .as_ref()
        .and_then(|binding| binding.file.clone())
        .or_else(|| map.files.get(key).cloned())
}

fn audiences_for(entry: &RouteEntry, scope: RpcOperationScope) -> Vec<RpcClientAudience> {
    if scope == RpcOperationScope::Admin {
        return vec![RpcClientAudience::Server];
    }
    match entry
        .authorization
        .as_ref()
        .map(|policy| policy.mode.as_str())
    {
        Some("service" | "admin") => vec![RpcClientAudience::Server],
        _ => vec![RpcClientAudience::Browser, RpcClientAudience::Server],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ir_keeps_http_projection_and_rpc_identity_separate() {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"fiducia-api-server",
              "map":{
                "find_user_by_id":{
                  "path":"/v1/users/{user_id}",
                  "methods":["GET"],
                  "rpc_key":"fiducia_cloud.users.find_user_by_id",
                  "path_params":{
                    "type":"object",
                    "properties":{"user_id":{"type":"string"}},
                    "required":["user_id"]
                  },
                  "header_schema":{
                    "type":"object",
                    "properties":{"if-none-match":{"type":"string"}}
                  },
                  "response_schema":{"type":"object"},
                  "binding":{
                    "annotation":"ores_rpc",
                    "file":"src/routes/v1/users/[user_id]/route.rs"
                  }
                }
              }
            }"#,
        )
        .expect("map");
        let op = rpc_operation_contract(
            &map,
            "find_user_by_id",
            RpcOperationScope::Regular,
            Some("fiducia-cloud/fiducia-api-server.rs"),
            Some("0123456789012345678901234567890123456789"),
        )
        .expect("operation IR");
        assert_eq!(op.operation_key, "fiducia_cloud.users.find_user_by_id");
        assert_eq!(op.namespace, vec!["fiducia_cloud", "users"]);
        assert_eq!(op.http.method, "GET");
        assert_eq!(op.http.path, "/v1/users/{user_id}");
        assert_eq!(op.http.rpc_transport_path, "/v1/rpc");
        assert!(op.request.header_schema.is_some());
        assert_eq!(op.codecs.default, RpcPayloadCodec::Json);
        assert_eq!(
            op.audiences,
            vec![RpcClientAudience::Browser, RpcClientAudience::Server]
        );
    }

    #[test]
    fn admin_ir_is_server_only() {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"fiducia-admin-api-server",
              "map":{
                "disable_user":{
                  "path":"/v1/users/{user_id}/disable",
                  "methods":["POST"],
                  "rpc_key":"fiducia_cloud.admin.users.disable_user",
                  "binding":{
                    "annotation":"ores_rpc",
                    "file":"src/routes/v1/users/[user_id]/disable/route.rs"
                  }
                }
              }
            }"#,
        )
        .expect("map");
        let op = rpc_operation_contract(&map, "disable_user", RpcOperationScope::Admin, None, None)
            .expect("operation IR");
        assert_eq!(op.audiences, vec![RpcClientAudience::Server]);
    }
}
