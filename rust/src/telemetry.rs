//! Telemetry attribute bag for an RPC call or receipt.
//!
//! Shaped so ores-otel can put it in log-context `fields` (or any OTEL
//! attribute map). This crate does **not** depend on ores-otel; ores-otel
//! does **not** depend on this crate. No payloads, tokens, or PII.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};

use crate::call::Transport;
use crate::schema::{validate_telemetry_attributes, SchemaError};

pub const RPC_SYSTEM: &str = "ores-api-docs";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TelemetryAttributes {
    #[serde(rename = "rpc.system")]
    pub rpc_system: &'static str,
    #[serde(rename = "rpc.service")]
    pub rpc_service: String,
    #[serde(rename = "rpc.method")]
    pub rpc_method: String,
    #[serde(rename = "rpc.transport")]
    pub rpc_transport: Transport,
    #[serde(rename = "rpc.ok", skip_serializing_if = "Option::is_none")]
    pub rpc_ok: Option<bool>,
    #[serde(rename = "http.status_code", skip_serializing_if = "Option::is_none")]
    pub http_status_code: Option<u16>,
}

impl TelemetryAttributes {
    #[must_use]
    pub fn start(service: impl Into<String>, method: impl Into<String>, transport: Transport) -> Self {
        Self {
            rpc_system: RPC_SYSTEM,
            rpc_service: service.into(),
            rpc_method: method.into(),
            rpc_transport: transport,
            rpc_ok: None,
            http_status_code: None,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        validate_telemetry_attributes(&self.to_fields()?)
    }

    /// Flattened object for ores-otel `fields` / OTEL attributes.
    pub fn to_fields(&self) -> Result<Value, SchemaError> {
        serde_json::to_value(self).map_err(|e| SchemaError::Instance {
            name: "telemetry-attributes",
            detail: e.to_string(),
        })
    }

    #[must_use]
    pub fn to_string_map(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert("rpc.system".into(), self.rpc_system.into());
        out.insert("rpc.service".into(), self.rpc_service.clone());
        out.insert("rpc.method".into(), self.rpc_method.clone());
        out.insert("rpc.transport".into(), self.rpc_transport.as_str().into());
        if let Some(ok) = self.rpc_ok {
            out.insert("rpc.ok".into(), ok.to_string());
        }
        if let Some(status) = self.http_status_code {
            out.insert("http.status_code".into(), status.to_string());
        }
        out
    }
}

/// Optional W3C ids copied from a call frame. Same names as ores-otel
/// log-context (`traceId`, `spanId`); unused here except as a pass-through.
#[must_use]
pub fn trace_context_fields(trace_id: Option<&str>, span_id: Option<&str>) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(id) = trace_id {
        obj.insert("traceId".into(), json!(id));
    }
    if let Some(id) = span_id {
        obj.insert("spanId".into(), json!(id));
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_match_schema() {
        let mut attrs = TelemetryAttributes::start("hhm-api-server", "get_reservation", Transport::Http);
        attrs.rpc_ok = Some(true);
        attrs.http_status_code = Some(200);
        attrs.validate().unwrap();
        let fields = attrs.to_fields().unwrap();
        assert_eq!(fields["rpc.system"], "ores-api-docs");
        assert_eq!(fields["rpc.transport"], "http");
        let obj = fields.as_object().unwrap();
        assert!(obj.len() <= 256);
        for key in obj.keys() {
            assert!(!key.is_empty() && key.len() <= 256, "{key}");
        }
    }

    #[test]
    fn otel_shaped_log_context_uses_same_field_names_without_a_crate_edge() {
        // ores-otel log-context: { fields, traceId, spanId }. We copy names only.
        let mut attrs = TelemetryAttributes::start("example-rpc", "get_item", Transport::Tcp);
        attrs.rpc_ok = Some(true);
        attrs.http_status_code = Some(200);
        let log = json!({
            "fields": attrs.to_fields().unwrap(),
            "traceId": "4bf92f3577b34da6a3ce929d0e0e4736",
            "spanId": "00f067aa0ba902b7",
        });
        let fields = log["fields"].as_object().unwrap();
        assert_eq!(fields.get("rpc.system").unwrap(), "ores-api-docs");
        assert!(!fields.contains_key("body"));
        assert!(!fields.contains_key("authorization"));
        assert_eq!(log["traceId"].as_str().unwrap().len(), 32);
        assert_eq!(log["spanId"].as_str().unwrap().len(), 16);
    }
}
