//! Transport-neutral RPC call and receipt frames.
//!
//! HTTP, TCP, and WebSocket all carry the same JSON. This module encodes and
//! validates frames. It does not open sockets, speak Axum, or import
//! opto-sync / ores-otel.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::{validate_rpc_call, validate_rpc_receipt, SchemaError};

pub const CALL_VERSION: u32 = 1;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Http,
    Tcp,
    Websocket,
}

impl Transport {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Tcp => "tcp",
            Self::Websocket => "websocket",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "http" => Some(Self::Http),
            "tcp" => Some(Self::Tcp),
            "websocket" => Some(Self::Websocket),
            _ => None,
        }
    }
}

/// One JSON object: HTTP body mapping, WebSocket text frame, or TCP NDJSON line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcCall {
    pub v: u32,
    pub op: CallOp,
    pub id: String,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub path: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub query: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    #[serde(rename = "traceId", skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(rename = "spanId", skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallOp {
    Call,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcReceipt {
    pub v: u32,
    pub op: ReceiptOp,
    pub id: String,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(rename = "traceId", skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(rename = "spanId", skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptOp {
    Receipt,
}

impl RpcCall {
    pub fn new(id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            v: CALL_VERSION,
            op: CallOp::Call,
            id: id.into(),
            key: key.into(),
            transport: None,
            path: Value::Null,
            query: Value::Null,
            body: None,
            trace_id: None,
            span_id: None,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        validate_rpc_call(&serde_json::to_value(self).map_err(|e| SchemaError::Instance {
            name: "rpc-call",
            detail: e.to_string(),
        })?)
    }

    /// One object per line for TCP. Does not include a trailing extra newline beyond `\n`.
    pub fn to_ndjson(&self) -> Result<String, SchemaError> {
        self.validate()?;
        let mut line = serde_json::to_string(self).map_err(|e| SchemaError::Instance {
            name: "rpc-call",
            detail: e.to_string(),
        })?;
        line.push('\n');
        Ok(line)
    }

    pub fn from_ndjson(line: &str) -> Result<Self, SchemaError> {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let value: Value = serde_json::from_str(trimmed).map_err(|e| SchemaError::Instance {
            name: "rpc-call",
            detail: e.to_string(),
        })?;
        validate_rpc_call(&value)?;
        serde_json::from_value(value).map_err(|e| SchemaError::Instance {
            name: "rpc-call",
            detail: e.to_string(),
        })
    }
}

impl RpcReceipt {
    pub fn ok(id: impl Into<String>, key: impl Into<String>, body: Option<Value>) -> Self {
        Self {
            v: CALL_VERSION,
            op: ReceiptOp::Receipt,
            id: id.into(),
            key: key.into(),
            transport: None,
            ok: true,
            status: Some(200),
            body,
            error: None,
            trace_id: None,
            span_id: None,
        }
    }

    pub fn error(
        id: impl Into<String>,
        key: impl Into<String>,
        status: u16,
        error: Value,
    ) -> Self {
        Self {
            v: CALL_VERSION,
            op: ReceiptOp::Receipt,
            id: id.into(),
            key: key.into(),
            transport: None,
            ok: false,
            status: Some(status),
            body: None,
            error: Some(error),
            trace_id: None,
            span_id: None,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        validate_rpc_receipt(&serde_json::to_value(self).map_err(|e| SchemaError::Instance {
            name: "rpc-receipt",
            detail: e.to_string(),
        })?)
    }

    pub fn to_ndjson(&self) -> Result<String, SchemaError> {
        self.validate()?;
        let mut line = serde_json::to_string(self).map_err(|e| SchemaError::Instance {
            name: "rpc-receipt",
            detail: e.to_string(),
        })?;
        line.push('\n');
        Ok(line)
    }

    pub fn from_ndjson(line: &str) -> Result<Self, SchemaError> {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let value: Value = serde_json::from_str(trimmed).map_err(|e| SchemaError::Instance {
            name: "rpc-receipt",
            detail: e.to_string(),
        })?;
        validate_rpc_receipt(&value)?;
        serde_json::from_value(value).map_err(|e| SchemaError::Instance {
            name: "rpc-receipt",
            detail: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndjson_round_trip_call() {
        let mut call = RpcCall::new("c1", "get_reservation");
        call.transport = Some(Transport::Tcp);
        call.path = serde_json::json!({ "id": "a" });
        let line = call.to_ndjson().unwrap();
        assert!(line.ends_with('\n'));
        let back = RpcCall::from_ndjson(&line).unwrap();
        assert_eq!(back.key, "get_reservation");
        assert_eq!(back.transport, Some(Transport::Tcp));
    }

    #[test]
    fn receipt_error_validates() {
        let rec = RpcReceipt::error(
            "c1",
            "get_reservation",
            404,
            serde_json::json!({ "code": "not_found" }),
        );
        rec.validate().unwrap();
        assert!(!rec.ok);
    }
}
