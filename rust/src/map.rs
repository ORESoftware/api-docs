//! Parse the authoring route map (keys → routes).

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::binding::RouteBinding;
use crate::infer::{infer_methods, is_connect_method_key};
use crate::schema::{validate_route_map, SchemaError};
use crate::template::path_template_vars;
use crate::SCHEMA_VERSION;

#[derive(Debug, Error)]
pub enum MapError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schema: {0}")]
    Schema(#[from] SchemaError),
    #[error("{0}")]
    Semantic(String),
}

/// One normalized route: path + methods + optional language binding.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RouteEntry {
    pub path: String,
    pub methods: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<RouteBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias_of: Option<String>,
    pub transports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_framing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opto_sync: Option<OptoSyncQueue>,
}

/// opto-sync queue settings declared on a route. Not an opto-sync crate type.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct OptoSyncQueue {
    pub table: String,
    pub operation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RouteMap {
    pub schema_version: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub map: BTreeMap<String, RouteEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RawMap {
    schema_version: String,
    service: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    map: BTreeMap<String, Value>,
    #[serde(default)]
    files: BTreeMap<String, String>,
}

impl RouteMap {
    pub fn from_json_str(json: &str) -> Result<Self, MapError> {
        let value: Value = serde_json::from_str(json)?;
        Self::from_value(value)
    }

    pub fn from_value(value: Value) -> Result<Self, MapError> {
        validate_route_map(&value)?;
        let raw: RawMap = serde_json::from_value(value)?;
        if raw.schema_version != SCHEMA_VERSION {
            return Err(MapError::Semantic(format!(
                "schema_version {} != {SCHEMA_VERSION}",
                raw.schema_version
            )));
        }
        let mut map = BTreeMap::new();
        for (key, val) in raw.map {
            map.insert(key.clone(), normalize_entry(&key, val)?);
        }
        let parsed = Self {
            schema_version: raw.schema_version,
            service: raw.service,
            title: raw.title,
            version: raw.version,
            description: raw.description,
            map,
            files: raw.files,
        };
        parsed.semantic_checks()?;
        Ok(parsed)
    }

    fn semantic_checks(&self) -> Result<(), MapError> {
        let mut occupied: BTreeMap<(String, String), String> = BTreeMap::new();
        for (key, entry) in &self.map {
            if is_connect_method_key(key) && entry.methods.iter().any(|m| m != "POST") {
                return Err(MapError::Semantic(format!(
                    "{key}: Connect JSON unary keys must be POST-only"
                )));
            }
            for method in &entry.methods {
                let uses_http_path = entry
                    .transports
                    .iter()
                    .any(|t| t == "http" || t == "websocket");
                if !uses_http_path {
                    continue;
                }
                let slot = (entry.path.clone(), method.clone());
                if let Some(other) = occupied.insert(slot, key.clone()) {
                    return Err(MapError::Semantic(format!(
                        "{key} and {other} both bind {method} {}",
                        entry.path
                    )));
                }
            }
            if let Some(framing) = &entry.tcp_framing {
                if !entry.transports.iter().any(|t| t == "tcp") {
                    return Err(MapError::Semantic(format!(
                        "{key}: tcp_framing set but transports does not include tcp"
                    )));
                }
                if framing != "ndjson" && framing != "length-prefixed" {
                    return Err(MapError::Semantic(format!(
                        "{key}: unknown tcp_framing {framing}"
                    )));
                }
            }
            if entry.transports.iter().all(|t| t == "nats") && entry.query_schema.is_some() {
                return Err(MapError::Semantic(format!(
                    "{key}: query parameters have no NATS encoding; add http or tcp, or move them into the request body"
                )));
            }
            check_header_schema(key, entry.header_schema.as_ref())?;
            check_delivery(key, entry)?;
            let vars = path_template_vars(&entry.path).map_err(|e| MapError::Semantic(e.to_string()))?;
            if let Some(schema) = &entry.path_params {
                let props = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        MapError::Semantic(format!(
                            "{key}: path_params must be a JSON Schema object with properties"
                        ))
                    })?;
                let declared: BTreeSet<&str> = props.keys().map(String::as_str).collect();
                let needed: BTreeSet<&str> = vars.iter().map(String::as_str).collect();
                if declared != needed {
                    return Err(MapError::Semantic(format!(
                        "{key}: path_params properties {declared:?} != template {needed:?}"
                    )));
                }
            }
            if let Some(alias) = &entry.alias_of {
                if !self.map.contains_key(alias) {
                    return Err(MapError::Semantic(format!(
                        "{key}: alias_of {alias} is not a map key"
                    )));
                }
                if alias == key {
                    return Err(MapError::Semantic(format!("{key}: alias_of cannot be self")));
                }
            }
            for (label, schema) in [
                ("query_schema", &entry.query_schema),
                ("header_schema", &entry.header_schema),
                ("request_schema", &entry.request_schema),
                ("response_schema", &entry.response_schema),
                ("error_schema", &entry.error_schema),
            ] {
                if let Some(Value::Object(obj)) = schema {
                    if obj.get("type").and_then(Value::as_str) == Some("array") {
                        return Err(MapError::Semantic(format!(
                            "{key}: {label} must describe an object, not a top-level array"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn lookup(&self, key: &str) -> Option<&RouteEntry> {
        self.map.get(key)
    }
}

fn infer_transports(key: &str, path: &str) -> Vec<String> {
    let lower = key.to_ascii_lowercase();
    if path == "/ws" || path == "/websocket" || lower.contains("websocket") {
        vec!["websocket".into()]
    } else {
        vec!["http".into()]
    }
}

fn parse_transports(key: &str, value: Option<&Value>, path: &str) -> Result<Vec<String>, MapError> {
    if let Some(Value::Array(arr)) = value {
        let mut out = Vec::new();
        for item in arr {
            let name = item.as_str().ok_or_else(|| {
                MapError::Semantic(format!("{key}: transports entries must be strings"))
            })?;
            if !matches!(name, "http" | "tcp" | "websocket" | "nats") {
                return Err(MapError::Semantic(format!(
                    "{key}: unknown transport {name}"
                )));
            }
            if out.iter().any(|t| t == name) {
                return Err(MapError::Semantic(format!(
                    "{key}: duplicate transport {name}"
                )));
            }
            out.push(name.to_string());
        }
        if out.is_empty() {
            return Err(MapError::Semantic(format!("{key}: transports must not be empty")));
        }
        return Ok(out);
    }
    Ok(infer_transports(key, path))
}

fn require_schema_object(key: &str, field: &str, value: &Value) -> Result<(), MapError> {
    if !value.is_object() {
        return Err(MapError::Semantic(format!(
            "{key}: {field} must be a JSON Schema object"
        )));
    }
    Ok(())
}

fn parse_opto_sync(key: &str, value: &Value) -> Result<OptoSyncQueue, MapError> {
    let obj = value.as_object().ok_or_else(|| {
        MapError::Semantic(format!("{key}: opto_sync must be an object"))
    })?;
    let table = obj
        .get("table")
        .and_then(Value::as_str)
        .ok_or_else(|| MapError::Semantic(format!("{key}: opto_sync.table required")))?;
    if !opto_table_ok(table) {
        return Err(MapError::Semantic(format!(
            "{key}: opto_sync.table {table:?} is not a SQL-safe identifier"
        )));
    }
    let operation = obj
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| MapError::Semantic(format!("{key}: opto_sync.operation required")))?;
    if operation != "upsert" && operation != "delete" {
        return Err(MapError::Semantic(format!(
            "{key}: opto_sync.operation must be upsert or delete"
        )));
    }
    Ok(OptoSyncQueue {
        table: table.to_string(),
        operation: operation.to_string(),
    })
}

fn opto_table_ok(table: &str) -> bool {
    let mut chars = table.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    if table.len() > 63 {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check_header_schema(key: &str, schema: Option<&Value>) -> Result<(), MapError> {
    let Some(schema) = schema else { return Ok(()) };
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| MapError::Semantic(format!("{key}: header_schema must declare properties")))?;
    const HOP_BY_HOP: &[&str] = &[
        "connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
        "te", "trailer", "transfer-encoding", "upgrade",
    ];
    for name in properties.keys() {
        let valid = !name.is_empty()
            && name.len() <= 128
            && name.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~')
            });
        if !valid {
            return Err(MapError::Semantic(format!(
                "{key}: header_schema name {name:?} must be a canonical lowercase HTTP field name"
            )));
        }
        if HOP_BY_HOP.contains(&name.as_str()) {
            return Err(MapError::Semantic(format!(
                "{key}: hop-by-hop header {name:?} is not an application contract"
            )));
        }
        if name.starts_with("grpc-") {
            return Err(MapError::Semantic(format!(
                "{key}: header {name:?} uses the reserved grpc- protocol namespace"
            )));
        }
    }
    Ok(())
}

fn check_delivery(key: &str, entry: &RouteEntry) -> Result<(), MapError> {
    let delivery = entry.delivery.as_deref().unwrap_or("direct");
    if delivery != "direct" && delivery != "opto_sync_queued" {
        return Err(MapError::Semantic(format!(
            "{key}: delivery must be direct or opto_sync_queued"
        )));
    }
    if delivery == "direct" {
        if entry.opto_sync.is_some() {
            return Err(MapError::Semantic(format!(
                "{key}: opto_sync settings require delivery: opto_sync_queued"
            )));
        }
        return Ok(());
    }
    let mutating = ["POST", "PUT", "PATCH", "DELETE"];
    if entry.methods.iter().any(|m| !mutating.contains(&m.as_str())) {
        return Err(MapError::Semantic(format!(
            "{key}: only mutating methods can be queued through opto-sync"
        )));
    }
    let Some(opto) = &entry.opto_sync else {
        return Err(MapError::Semantic(format!(
            "{key}: delivery opto_sync_queued requires an opto_sync block"
        )));
    };
    match opto.operation.as_str() {
        "upsert" => {
            let Some(schema) = &entry.request_schema else {
                return Err(MapError::Semantic(format!(
                    "{key}: a queued upsert needs a request_schema — opto-sync requires a payload"
                )));
            };
            if schema.get("type").and_then(Value::as_str) == Some("array") {
                return Err(MapError::Semantic(format!(
                    "{key}: queued upsert payload must be a JSON object"
                )));
            }
        }
        "delete" => {
            if entry.request_schema.is_some() {
                return Err(MapError::Semantic(format!(
                    "{key}: a queued delete must not carry a request body"
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

fn normalize_entry(key: &str, value: Value) -> Result<RouteEntry, MapError> {
    match value {
        Value::String(path) => {
            if !path.starts_with('/') {
                return Err(MapError::Semantic(format!("{key}: path must start with /")));
            }
            Ok(RouteEntry {
                path: path.clone(),
                methods: infer_methods(key),
                summary: None,
                binding: None,
                path_params: None,
                query_schema: None,
                header_schema: None,
                request_schema: None,
                response_schema: None,
                error_schema: None,
                alias_of: None,
                transports: infer_transports(key, &path),
                tcp_framing: None,
                delivery: None,
                opto_sync: None,
            })
        }
        Value::Object(obj) => {
            let path = obj
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| MapError::Semantic(format!("{key}: missing path")))?
                .to_string();
            let methods = obj
                .get("methods")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| infer_methods(key));
            let summary = obj.get("summary").and_then(Value::as_str).map(str::to_owned);
            let binding = obj
                .get("binding")
                .cloned()
                .map(serde_json::from_value::<RouteBinding>)
                .transpose()
                .map_err(MapError::Json)?;
            if let Some(b) = binding.as_ref() {
                if b.is_empty() {
                    return Err(MapError::Semantic(format!(
                        "{key}: binding must be annotation, param_types, return_type, function_type, or a combination"
                    )));
                }
            }
            if let Some(schema) = obj.get("path_params") {
                require_schema_object(key, "path_params", schema)?;
            }
            if let Some(schema) = obj.get("query_schema") {
                require_schema_object(key, "query_schema", schema)?;
            }
            if let Some(schema) = obj.get("header_schema") {
                require_schema_object(key, "header_schema", schema)?;
            }
            if let Some(schema) = obj.get("request_schema") {
                require_schema_object(key, "request_schema", schema)?;
            }
            if let Some(schema) = obj.get("response_schema") {
                require_schema_object(key, "response_schema", schema)?;
            }
            if let Some(schema) = obj.get("error_schema") {
                require_schema_object(key, "error_schema", schema)?;
            }
            let transports = parse_transports(key, obj.get("transports"), &path)?;
            let tcp_framing = obj
                .get("tcp_framing")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let tcp_framing = match tcp_framing {
                Some(f) => Some(f),
                None if transports.iter().any(|t| t == "tcp") => Some("ndjson".into()),
                None => None,
            };
            let delivery = obj
                .get("delivery")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let opto_sync = match obj.get("opto_sync") {
                Some(v) => Some(parse_opto_sync(key, v)?),
                None => None,
            };
            Ok(RouteEntry {
                path,
                methods,
                summary,
                binding,
                path_params: obj.get("path_params").cloned(),
                query_schema: obj.get("query_schema").cloned(),
                header_schema: obj.get("header_schema").cloned(),
                request_schema: obj.get("request_schema").cloned(),
                response_schema: obj.get("response_schema").cloned(),
                error_schema: obj.get("error_schema").cloned(),
                alias_of: obj
                    .get("alias_of")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                transports,
                tcp_framing,
                delivery,
                opto_sync,
            })
        }
        other => Err(MapError::Semantic(format!(
            "{key}: expected path string or object, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_pmap_map_parses() {
        let json = include_str!("../../examples/pmap-api.route-map.json");
        let map = RouteMap::from_json_str(json).expect("example map");
        assert_eq!(map.lookup("healthz").unwrap().path, "/healthz");
        assert_eq!(map.lookup("healthz").unwrap().methods, vec!["GET"]);
        let rpc = map.lookup("CheckFieldSanity").unwrap();
        assert_eq!(rpc.methods, vec!["POST"]);
        assert_eq!(rpc.path, "/pmap.v1.Interview/CheckFieldSanity");
        let b = rpc.binding.as_ref().unwrap();
        assert!(b.is_combination());
        assert_eq!(b.param_types, vec!["CheckFieldSanityRequest"]);
        assert_eq!(b.return_type.as_deref(), Some("CheckFieldSanityResponse"));
        assert!(b.function_type.as_ref().unwrap().contains("UnaryFn"));
        let get = map.lookup("get_matter").unwrap();
        assert!(get.path_params.is_some());
        assert!(get.query_schema.is_some());
        assert_eq!(get.transports, vec!["http"]);
    }

    #[test]
    fn websocket_and_tcp_transports() {
        let json = include_str!("../../examples/rpc-transports.route-map.json");
        let map = RouteMap::from_json_str(json).expect("transports map");
        assert_eq!(
            map.lookup("get_item").unwrap().transports,
            vec!["http", "tcp", "websocket"]
        );
        assert_eq!(map.lookup("websocket").unwrap().transports, vec!["websocket"]);
        assert_eq!(map.lookup("tcp_ping").unwrap().tcp_framing.as_deref(), Some("ndjson"));
        let call = crate::RpcCall::new("c1", "get_item");
        let env = crate::RouteMapEnvelope::wrap(&map, "1").unwrap();
        assert_eq!(env.scope, crate::OPTO_SYNC_SCOPE);
        let attrs = crate::TelemetryAttributes::start(
            map.service.clone(),
            call.key.clone(),
            crate::Transport::Tcp,
        );
        attrs.validate().unwrap();
    }

    #[test]
    fn duplicate_path_method_is_rejected() {
        let json = r#"{
          "schema_version": "1.0.0",
          "service": "x",
          "map": {
            "a": "/healthz",
            "b": "/healthz"
          }
        }"#;
        let err = RouteMap::from_json_str(json).unwrap_err();
        assert!(format!("{err}").contains("both bind"));
    }

    #[test]
    fn path_params_must_match_template() {
        let json = r#"{
          "schema_version": "1.0.0",
          "service": "x",
          "map": {
            "get_item": {
              "path": "/v1/items/{id}",
              "methods": ["GET"],
              "path_params": {
                "type": "object",
                "properties": { "nope": { "type": "string" } }
              }
            }
          }
        }"#;
        let err = RouteMap::from_json_str(json).unwrap_err();
        assert!(format!("{err}").contains("path_params"));
    }

    #[test]
    fn queued_upsert_and_nats_rules() {
        let ok = r#"{
          "schema_version": "1.0.0",
          "service": "x",
          "map": {
            "walk_matter": {
              "path": "/v1/matters/{id}/walk",
              "methods": ["POST"],
              "request_schema": { "type": "object" },
              "delivery": "opto_sync_queued",
              "opto_sync": { "table": "demo_matter_walk", "operation": "upsert" }
            },
            "nats_ping": {
              "path": "/rpc/nats-ping",
              "methods": ["POST"],
              "transports": ["nats"],
              "request_schema": { "type": "object" }
            }
          }
        }"#;
        let map = RouteMap::from_json_str(ok).expect("queued map");
        assert_eq!(
            map.lookup("walk_matter").unwrap().delivery.as_deref(),
            Some("opto_sync_queued")
        );
        assert_eq!(map.lookup("nats_ping").unwrap().transports, vec!["nats"]);

        let get_queued = r#"{
          "schema_version": "1.0.0",
          "service": "x",
          "map": {
            "get_item": {
              "path": "/v1/items/{id}",
              "methods": ["GET"],
              "delivery": "opto_sync_queued",
              "opto_sync": { "table": "items", "operation": "upsert" }
            }
          }
        }"#;
        let err = RouteMap::from_json_str(get_queued).unwrap_err();
        assert!(format!("{err}").contains("mutating"));

        let nats_query = r#"{
          "schema_version": "1.0.0",
          "service": "x",
          "map": {
            "list_items": {
              "path": "/v1/items",
              "methods": ["GET"],
              "transports": ["nats"],
              "query_schema": { "type": "object", "properties": { "q": { "type": "string" } } }
            }
          }
        }"#;
        let err = RouteMap::from_json_str(nats_query).unwrap_err();
        assert!(format!("{err}").contains("NATS"));
    }
}
