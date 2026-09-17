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

/// Documentation and admission metadata for one RPC operation.
///
/// This describes the authorization decision that a runtime must enforce. It is
/// not itself an authentication result and never carries tenant/actor identity.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct AuthorizationPolicy {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default)]
    pub step_up: bool,
}

/// One normalized route: path + methods + optional language/RPC policy bindings.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RouteEntry {
    pub path: String,
    pub methods: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<AuthorizationPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_classification: Option<String>,
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
        let mut rpc_keys: BTreeMap<String, String> = BTreeMap::new();
        for (key, entry) in &self.map {
            if is_connect_method_key(key) && entry.methods.iter().any(|m| m != "POST") {
                return Err(MapError::Semantic(format!(
                    "{key}: Connect JSON unary keys must be POST-only"
                )));
            }
            if let Some(rpc_key) = &entry.rpc_key {
                if !rpc_key_ok(rpc_key) {
                    return Err(MapError::Semantic(format!(
                        "{key}: rpc_key {rpc_key:?} must be dot-separated lowercase object-key segments"
                    )));
                }
                if let Some(other) = rpc_keys.insert(rpc_key.clone(), key.clone()) {
                    return Err(MapError::Semantic(format!(
                        "{key} and {other} both declare rpc_key {rpc_key}"
                    )));
                }
            }
            check_authorization(key, entry.authorization.as_ref())?;
            check_idempotency(key, entry)?;
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
            let vars =
                path_template_vars(&entry.path).map_err(|e| MapError::Semantic(e.to_string()))?;
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
                    return Err(MapError::Semantic(format!(
                        "{key}: alias_of cannot be self"
                    )));
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

    /// Lookup by the authored route-map key only.
    #[must_use]
    pub fn lookup(&self, key: &str) -> Option<&RouteEntry> {
        self.map.get(key)
    }

    /// Lookup the canonical RPC identity.
    ///
    /// New generated clients send the stable dotted `rpc_key`. During migration
    /// legacy callers may still send the authored route-map key, so that remains
    /// an accepted alias. `semantic_checks` guarantees `rpc_key` uniqueness.
    #[must_use]
    pub fn lookup_rpc(&self, key: &str) -> Option<&RouteEntry> {
        self.map.get(key).or_else(|| {
            self.map
                .values()
                .find(|entry| entry.rpc_key.as_deref() == Some(key))
        })
    }
}

fn rpc_key_ok(key: &str) -> bool {
    if key.len() > 160 {
        return false;
    }
    let segments: Vec<&str> = key.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.into_iter().all(|segment| {
        let mut chars = segment.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    })
}

fn check_authorization(key: &str, policy: Option<&AuthorizationPolicy>) -> Result<(), MapError> {
    let Some(policy) = policy else { return Ok(()) };
    if !matches!(
        policy.mode.as_str(),
        "public" | "authenticated" | "service" | "admin"
    ) {
        return Err(MapError::Semantic(format!(
            "{key}: unknown authorization mode {}",
            policy.mode
        )));
    }
    if policy.mode == "public"
        && (!policy.roles.is_empty()
            || !policy.scopes.is_empty()
            || policy.audience.is_some()
            || policy.step_up)
    {
        return Err(MapError::Semantic(format!(
            "{key}: public authorization cannot declare roles, scopes, audience, or step_up"
        )));
    }
    if policy.mode == "service" && policy.step_up {
        return Err(MapError::Semantic(format!(
            "{key}: service authorization cannot request end-user step_up"
        )));
    }
    Ok(())
}

fn check_idempotency(key: &str, entry: &RouteEntry) -> Result<(), MapError> {
    let Some(mode) = entry.idempotency.as_deref() else { return Ok(()) };
    if !matches!(mode, "none" | "optional" | "required") {
        return Err(MapError::Semantic(format!(
            "{key}: unknown idempotency mode {mode}"
        )));
    }
    if mode == "required" && entry.methods.iter().any(|method| method == "GET" || method == "HEAD") {
        return Err(MapError::Semantic(format!(
            "{key}: idempotency=required is invalid for GET/HEAD"
        )));
    }
    Ok(())
}

fn check_header_schema(key: &str, schema: Option<&Value>) -> Result<(), MapError> {
    let Some(schema) = schema else { return Ok(()) };
    let Some(object) = schema.as_object() else {
        return Err(MapError::Semantic(format!(
            "{key}: header_schema must be a JSON Schema object"
        )));
    };
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err(MapError::Semantic(format!(
            "{key}: header_schema must have type=object"
        )));
    }
    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        return Err(MapError::Semantic(format!(
            "{key}: header_schema must define properties"
        )));
    };
    for name in properties.keys() {
        let canonical = name.to_ascii_lowercase();
        if canonical != *name {
            return Err(MapError::Semantic(format!(
                "{key}: header_schema name {name:?} must be canonical lowercase"
            )));
        }
        if name == "content-type" || name == "accept" {
            return Err(MapError::Semantic(format!(
                "{key}: {name} is runtime-owned by the selected codec and may not be an application header"
            )));
        }
    }
    Ok(())
}

fn check_delivery(key: &str, entry: &RouteEntry) -> Result<(), MapError> {
    let Some(delivery) = entry.delivery.as_deref() else { return Ok(()) };
    if !matches!(delivery, "request_response" | "at_least_once" | "exactly_once") {
        return Err(MapError::Semantic(format!(
            "{key}: unknown delivery mode {delivery}"
        )));
    }
    if delivery == "exactly_once" && entry.idempotency.as_deref() != Some("required") {
        return Err(MapError::Semantic(format!(
            "{key}: exactly_once delivery requires idempotency=required"
        )));
    }
    Ok(())
}

fn normalize_entry(key: &str, value: Value) -> Result<RouteEntry, MapError> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| MapError::Semantic(format!("{key}: route entry must be object")))?;
    let path = object
        .remove("path")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("/{key}"));
    let methods = infer_methods(key, &mut object)?;
    let summary = take_string(&mut object, "summary")?;
    let rpc_key = take_string(&mut object, "rpc_key")?;
    let authorization = take_authorization(&mut object)?;
    let idempotency = take_string(&mut object, "idempotency")?;
    let data_classification = take_string(&mut object, "data_classification")?;
    let binding = take_binding(&mut object)?;
    let path_params = object.remove("path_params");
    let query_schema = object.remove("query_schema");
    let header_schema = object.remove("header_schema");
    let request_schema = object.remove("request_schema");
    let response_schema = object.remove("response_schema");
    let error_schema = object.remove("error_schema");
    let alias_of = take_string(&mut object, "alias_of")?;
    let transports = take_string_array(&mut object, "transports")?
        .unwrap_or_else(|| vec!["http".into()]);
    let tcp_framing = take_string(&mut object, "tcp_framing")?;
    let delivery = take_string(&mut object, "delivery")?;
    let opto_sync = take_opto_sync(&mut object)?;
    if let Some(extra) = object.keys().next() {
        return Err(MapError::Semantic(format!(
            "{key}: unknown route entry field {extra:?}"
        )));
    }
    Ok(RouteEntry {
        path,
        methods,
        summary,
        rpc_key,
        authorization,
        idempotency,
        data_classification,
        binding,
        path_params,
        query_schema,
        header_schema,
        request_schema,
        response_schema,
        error_schema,
        alias_of,
        transports,
        tcp_framing,
        delivery,
        opto_sync,
    })
}

fn take_string(object: &mut serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, MapError> {
    match object.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(MapError::Semantic(format!("{key} must be a string"))),
    }
}

fn take_string_array(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, MapError> {
    let Some(value) = object.remove(key) else {
        return Ok(None);
    };
    let Value::Array(values) = value else {
        return Err(MapError::Semantic(format!("{key} must be an array")));
    };
    values
        .into_iter()
        .map(|value| match value {
            Value::String(value) => Ok(value),
            _ => Err(MapError::Semantic(format!("{key} entries must be strings"))),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn take_authorization(
    object: &mut serde_json::Map<String, Value>,
) -> Result<Option<AuthorizationPolicy>, MapError> {
    let Some(value) = object.remove("authorization") else {
        return Ok(None);
    };
    serde_json::from_value(value)
        .map(Some)
        .map_err(|error| MapError::Semantic(format!("authorization: {error}")))
}

fn take_binding(object: &mut serde_json::Map<String, Value>) -> Result<Option<RouteBinding>, MapError> {
    let Some(value) = object.remove("binding") else {
        return Ok(None);
    };
    serde_json::from_value(value)
        .map(Some)
        .map_err(|error| MapError::Semantic(format!("binding: {error}")))
}

fn take_opto_sync(
    object: &mut serde_json::Map<String, Value>,
) -> Result<Option<OptoSyncQueue>, MapError> {
    let Some(value) = object.remove("opto_sync") else {
        return Ok(None);
    };
    serde_json::from_value(value)
        .map(Some)
        .map_err(|error| MapError::Semantic(format!("opto_sync: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_rpc_accepts_stable_wire_key_and_legacy_map_key() {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo",
              "map":{
                "find_user_by_id":{
                  "path":"/v1/users/{id}",
                  "methods":["GET"],
                  "rpc_key":"demo.users.find_user"
                }
              }
            }"#,
        )
        .expect("map");
        assert_eq!(
            map.lookup_rpc("demo.users.find_user").map(|entry| entry.path.as_str()),
            Some("/v1/users/{id}")
        );
        assert_eq!(
            map.lookup_rpc("find_user_by_id").map(|entry| entry.path.as_str()),
            Some("/v1/users/{id}")
        );
    }

    #[test]
    fn duplicate_rpc_keys_fail_semantic_admission() {
        let error = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo",
              "map":{
                "a":{"path":"/a","methods":["GET"],"rpc_key":"demo.same"},
                "b":{"path":"/b","methods":["GET"],"rpc_key":"demo.same"}
              }
            }"#,
        )
        .expect_err("duplicate rpc key must fail");
        assert!(error.to_string().contains("both declare rpc_key"));
    }
}
