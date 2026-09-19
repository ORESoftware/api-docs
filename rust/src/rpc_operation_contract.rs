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

use crate::{analyze_shared_operation_route_source, contract_sha256, RouteEntry, RouteMap};

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

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "json" => Ok(Self::Json),
            "protobuf" => Ok(Self::Protobuf),
            "messagepack" => Ok(Self::Messagepack),
            other => Err(format!("unsupported RPC payload codec {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcClientAudience {
    Browser,
    Server,
}

impl RpcClientAudience {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "browser" => Ok(Self::Browser),
            "server" => Ok(Self::Server),
            other => Err(format!("unsupported RPC client audience {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcOperationScope {
    Regular,
    Admin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcStreamMode {
    #[default]
    Unary,
    ServerStream,
    ClientStream,
    Bidi,
}

impl RpcStreamMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unary => "unary",
            Self::ServerStream => "server_stream",
            Self::ClientStream => "client_stream",
            Self::Bidi => "bidi",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "unary" => Ok(Self::Unary),
            "server_stream" => Ok(Self::ServerStream),
            "client_stream" => Ok(Self::ClientStream),
            "bidi" => Ok(Self::Bidi),
            other => Err(format!("unsupported RPC stream mode {other:?}")),
        }
    }

    #[must_use]
    pub const fn is_streaming(self) -> bool {
        !matches!(self, Self::Unary)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RpcOperationSource {
    /// The `route.rs` HTTP adapter. Present exactly when [`RpcOperationContract::http`]
    /// is: an operation with no HTTP projection has no adapter file to name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_file: Option<String>,
    /// The `handlers.rs` that owns the operation. Required for a route-less
    /// operation, whose only source identity this is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handlers_file: Option<String>,
    pub handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoker: Option<String>,
    pub execution_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

/// Where an operation is ALSO reachable over plain HTTP.
///
/// A projection, never the operation's identity: an RPC operation is addressed
/// by key over [`RpcOperationContract::rpc_transport_path`] whether or not it
/// has one of these. The transport path used to live in here, which made an
/// operation without a `route.rs` unrepresentable — the IR could not say where
/// to send a call without also inventing an HTTP method and path for it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RpcHttpProjection {
    pub method: String,
    pub path: String,
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
    /// The RPC transport path every call to this operation is sent to.
    pub rpc_transport_path: &'static str,
    /// Absent for a route-less operation. The registry contract
    /// (`ores-interfaces` `rpc-operation/v1`) has always allowed that; this IR
    /// did not, so a consumer had to drop such operations or fabricate a route.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<RpcHttpProjection>,
    pub scope: RpcOperationScope,
    pub stream: RpcStreamMode,
    pub audiences: Vec<RpcClientAudience>,
    pub codecs: RpcCodecSet,
    pub request: RpcRequestShape,
    pub response: RpcResponseShape,
    pub contract_sha256: String,
}

/// Version of the serialized [`RpcOperationContract`].
///
/// 3: `http` is optional and `rpc_transport_path` moved out of it to the top
/// level; `source.route_file` is optional and `source.handlers_file` exists.
pub const RPC_OPERATION_CONTRACT_SCHEMA_VERSION: u32 = 3;

impl RpcOperationContract {
    /// Structural invariants that the field types alone cannot express.
    ///
    /// Generators call this before emitting anything, so an IR assembled by a
    /// consumer is held to the same rules as one built here.
    pub fn validate(&self) -> Result<(), String> {
        let key = &self.operation_key;
        if self.schema_version != RPC_OPERATION_CONTRACT_SCHEMA_VERSION {
            return Err(format!(
                "{key}: operation contract schema_version must be {RPC_OPERATION_CONTRACT_SCHEMA_VERSION}, got {}",
                self.schema_version
            ));
        }
        let rpc_path = self.rpc_transport_path.trim();
        if rpc_path.is_empty() || !rpc_path.starts_with('/') {
            return Err(format!(
                "{key}: a canonical absolute RPC transport path is required; route.rs HTTP projection metadata is not the RPC transport authority"
            ));
        }
        match (&self.http, &self.source.route_file) {
            (Some(http), Some(_)) => {
                if http.method.trim().is_empty() || !http.path.starts_with('/') {
                    return Err(format!(
                        "{key}: an HTTP projection needs a method and an absolute path, got {:?} {:?}",
                        http.method, http.path
                    ));
                }
            }
            (None, None) => {
                if self.source.handlers_file.is_none() {
                    return Err(format!(
                        "{key}: a route-less operation must name the handlers.rs that owns it"
                    ));
                }
            }
            (Some(_), None) => {
                return Err(format!(
                    "{key}: an HTTP projection without a route.rs adapter has no source"
                ));
            }
            (None, Some(route_file)) => {
                return Err(format!(
                    "{key}: {route_file} is named as the HTTP adapter, but the operation has no HTTP projection"
                ));
            }
        }
        Ok(())
    }

    /// Does this operation exist only over RPC?
    #[must_use]
    pub const fn is_route_less(&self) -> bool {
        self.http.is_none()
    }
}

/// Convert one normalized route-map operation into the codegen IR.
///
/// This compatibility constructor does not inspect the `route.rs` source and
/// therefore marks the server execution model as `http_projection_legacy`.
/// New filesystem routes should use [`rpc_operation_contract_with_route_source`]
/// so HTTP and RPC are statically bound to one shared typed operation.
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
        schema_version: RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
        operation_key,
        namespace: segments,
        source: RpcOperationSource {
            route_file: Some(route_file),
            handlers_file: None,
            handler,
            operation: None,
            invoker: None,
            execution_model: "http_projection_legacy".to_owned(),
            repository: repository.map(str::to_owned),
            commit_sha: commit_sha.map(str::to_owned),
        },
        rpc_transport_path: RPC_V1_HTTP_PATH,
        http: Some(RpcHttpProjection {
            method,
            path: entry.path.clone(),
        }),
        scope,
        stream: RpcStreamMode::Unary,
        audiences,
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

/// Build the preferred operation IR by inspecting the authoritative `route.rs`.
///
/// The referenced HTTP verb must carry `#[ores_route(operation = ...)]`, and
/// the referenced inner function must carry `#[ores_operation(...)]`. The
/// operation key in source must equal the route-map `rpc_key`; codec, audience,
/// and scope metadata come from the inner operation rather than the HTTP
/// adapter. This is the static invariant that keeps HTTP and `/v1/rpc` bound to
/// one typed implementation without synthesizing a second HTTP request.
pub fn rpc_operation_contract_with_route_source(
    map: &RouteMap,
    route_key: &str,
    scope: RpcOperationScope,
    repository: Option<&str>,
    commit_sha: Option<&str>,
    route_source_text: &str,
) -> Result<RpcOperationContract, String> {
    let mut contract = rpc_operation_contract(map, route_key, scope, repository, commit_sha)?;
    // A route-map entry always has an HTTP projection; the constructor above
    // set both of these.
    let route_file =
        contract.source.route_file.clone().ok_or_else(|| {
            format!("{route_key}: route-map operation lost its route.rs identity")
        })?;
    let http_method = contract
        .http
        .as_ref()
        .map(|http| http.method.clone())
        .ok_or_else(|| format!("{route_key}: route-map operation lost its HTTP projection"))?;
    let analysis = analyze_shared_operation_route_source(&route_file, route_source_text)
        .map_err(|error| error.to_string())?;
    let operation = analysis.operation_for_method(&http_method).ok_or_else(|| {
        format!(
            "{route_key}: {http_method} adapter must bind #[ores_route(operation = ...)] to a shared operation"
        )
    })?;
    if operation.key != contract.operation_key {
        return Err(format!(
            "{route_key}: route-map rpc_key {:?} disagrees with #[ores_operation] key {:?}",
            contract.operation_key, operation.key
        ));
    }

    let source_scope = match operation.scope.as_str() {
        "regular" => RpcOperationScope::Regular,
        "admin" => RpcOperationScope::Admin,
        other => {
            return Err(format!(
                "{route_key}: unsupported operation scope {other:?}"
            ))
        }
    };
    if source_scope != scope {
        return Err(format!(
            "{route_key}: requested scope {scope:?} disagrees with #[ores_operation] scope {source_scope:?}"
        ));
    }

    let allowed = operation
        .codecs
        .iter()
        .map(|codec| RpcPayloadCodec::parse(codec))
        .collect::<Result<Vec<_>, _>>()?;
    let default = RpcPayloadCodec::parse(&operation.default_codec)?;
    let audiences = operation
        .audiences
        .iter()
        .map(|audience| RpcClientAudience::parse(audience))
        .collect::<Result<Vec<_>, _>>()?;

    contract.source.operation = Some(operation.rust_name.clone());
    contract.source.invoker = Some(operation.invoke_name.clone());
    contract.source.execution_model = "shared_operation".to_owned();
    contract.stream = RpcStreamMode::parse(&operation.stream)?;
    contract.codecs = RpcCodecSet { allowed, default };
    contract.audiences = audiences;
    Ok(contract)
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

    fn sample_map() -> RouteMap {
        RouteMap::from_json_str(
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
        .expect("map")
    }

    #[test]
    fn compatibility_ir_is_explicitly_legacy_projection() {
        let map = sample_map();
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
        let http = op
            .http
            .as_ref()
            .expect("a route-map operation has an HTTP projection");
        assert_eq!(http.method, "GET");
        assert_eq!(http.path, "/v1/users/{user_id}");
        assert_eq!(op.rpc_transport_path, "/v1/rpc");
        op.validate().expect("a constructed contract is valid");
        assert_eq!(op.stream, RpcStreamMode::Unary);
        assert!(op.request.header_schema.is_some());
        assert_eq!(op.source.execution_model, "http_projection_legacy");
    }

    #[test]
    fn route_source_binds_http_and_rpc_to_same_operation() {
        let map = sample_map();
        let source = r#"
            #[ores_operation(
                key = "fiducia_cloud.users.find_user_by_id",
                codecs("json", "protobuf", "messagepack"),
                default_codec = "protobuf",
                audiences("browser", "server"),
                scope = "regular",
                stream = "unary"
            )]
            async fn find_user_by_id(ctx: OperationContext, input: FindUserInput)
                -> Result<FindUserOutput, FindUserError>
            { todo!() }

            #[ores_route(operation = find_user_by_id)]
            pub async fn get(Path(path): Path<FindUserPath>) -> HttpResult { todo!() }
        "#;
        let op = rpc_operation_contract_with_route_source(
            &map,
            "find_user_by_id",
            RpcOperationScope::Regular,
            Some("fiducia-cloud/fiducia-api-server.rs"),
            Some("0123456789012345678901234567890123456789"),
            source,
        )
        .expect("shared operation IR");
        assert_eq!(op.source.execution_model, "shared_operation");
        assert_eq!(op.source.operation.as_deref(), Some("find_user_by_id"));
        assert_eq!(
            op.source.invoker.as_deref(),
            Some("__ores_invoke_find_user_by_id")
        );
        assert_eq!(op.codecs.default, RpcPayloadCodec::Protobuf);
        assert_eq!(op.codecs.allowed.len(), 3);
        assert_eq!(op.stream, RpcStreamMode::Unary);
    }

    #[test]
    fn route_source_rpc_key_drift_fails_closed() {
        let map = sample_map();
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.wrong_operation")]
            async fn find_user_by_id(ctx: OperationContext, input: FindUserInput) -> Output {
                todo!()
            }
            #[ores_route(operation = find_user_by_id)]
            pub async fn get() -> HttpResult { todo!() }
        "#;
        let error = rpc_operation_contract_with_route_source(
            &map,
            "find_user_by_id",
            RpcOperationScope::Regular,
            None,
            None,
            source,
        )
        .expect_err("key drift must fail");
        assert!(error.contains("disagrees"));
    }

    #[test]
    fn handlers_stream_mode_reaches_normalized_ir() {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo-api-server",
              "map":{
                "watch_users_stream":{
                  "path":"/v1/users/stream",
                  "methods":["GET"],
                  "rpc_key":"demo.users.watch_users_stream",
                  "binding":{
                    "annotation":"ores_rpc",
                    "file":"src/routes/v1/users/stream/route.rs"
                  }
                }
              }
            }"#,
        )
        .expect("stream map");
        let source = r#"
            #[ores_operation(
                key = "demo.users.watch_users_stream",
                stream = "server_stream"
            )]
            async fn watch_users_stream(ctx: OperationContext, input: WatchInput) -> WatchOutput {
                todo!()
            }

            #[ores_route(operation = watch_users_stream)]
            pub async fn get() -> HttpResult { todo!() }
        "#;
        let op = rpc_operation_contract_with_route_source(
            &map,
            "watch_users_stream",
            RpcOperationScope::Regular,
            None,
            None,
            source,
        )
        .expect("stream operation IR");
        assert_eq!(op.stream, RpcStreamMode::ServerStream);
        assert!(op.stream.is_streaming());
        assert_eq!(op.stream.as_str(), "server_stream");
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
