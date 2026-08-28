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
    Nats,
}

impl Transport {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Tcp => "tcp",
            Self::Websocket => "websocket",
            Self::Nats => "nats",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "http" => Some(Self::Http),
            "tcp" => Some(Self::Tcp),
            "websocket" => Some(Self::Websocket),
            "nats" => Some(Self::Nats),
            _ => None,
        }
    }
}

/// 4-byte big-endian length prefix used when `tcp_framing` is `length-prefixed`.
pub const LENGTH_PREFIX_BYTES: usize = 4;
/// Refuse a declared length above this *before* allocating.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub fn encode_length_prefixed(payload: &[u8]) -> Result<Vec<u8>, SchemaError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(SchemaError::Instance {
            name: "rpc-call",
            detail: format!(
                "declared frame length {} is over the {MAX_FRAME_BYTES} limit",
                payload.len()
            ),
        });
    }
    let mut out = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Split a TCP read buffer into complete length-prefixed payloads and a leftover tail.
pub fn split_length_prefixed(buf: &[u8]) -> Result<(Vec<&[u8]>, &[u8]), SchemaError> {
    let mut frames = Vec::new();
    let mut offset = 0;
    while buf.len() - offset >= LENGTH_PREFIX_BYTES {
        let len = u32::from_be_bytes(buf[offset..offset + LENGTH_PREFIX_BYTES].try_into().unwrap())
            as usize;
        if len > MAX_FRAME_BYTES {
            return Err(SchemaError::Instance {
                name: "rpc-call",
                detail: format!("declared frame length {len} is over the {MAX_FRAME_BYTES} limit"),
            });
        }
        let start = offset + LENGTH_PREFIX_BYTES;
        if buf.len() - start < len {
            break;
        }
        frames.push(&buf[start..start + len]);
        offset = start + len;
    }
    Ok((frames, &buf[offset..]))
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

    pub fn to_length_prefixed(&self) -> Result<Vec<u8>, SchemaError> {
        self.validate()?;
        let payload = serde_json::to_vec(self).map_err(|e| SchemaError::Instance {
            name: "rpc-call",
            detail: e.to_string(),
        })?;
        encode_length_prefixed(&payload)
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

    pub fn to_length_prefixed(&self) -> Result<Vec<u8>, SchemaError> {
        self.validate()?;
        let payload = serde_json::to_vec(self).map_err(|e| SchemaError::Instance {
            name: "rpc-receipt",
            detail: e.to_string(),
        })?;
        encode_length_prefixed(&payload)
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

    #[test]
    fn golden_fixture_and_omitted_transport_are_valid() {
        let golden = include_str!("../../tests/generated-contract/valid/rpc-call.json");
        let call = RpcCall::from_ndjson(golden).unwrap();
        assert_eq!(call.key, "get_item");
        assert_eq!(call.transport, Some(Transport::Tcp));
        assert_eq!(call.id, "tcp-get-item");

        let omitted = RpcCall::new("c2", "get_item");
        omitted.validate().unwrap();
        assert!(omitted.transport.is_none());
        let line = omitted.to_ndjson().unwrap();
        assert!(!line.contains("transport"));
        let back = RpcCall::from_ndjson(line.trim_end()).unwrap();
        assert_eq!(back.id, "c2");
    }

    #[test]
    fn ndjson_accepts_crlf_and_receipt_round_trips() {
        let mut call = RpcCall::new("c-crlf", "tcp_ping");
        call.transport = Some(Transport::Tcp);
        let mut line = call.to_ndjson().unwrap();
        line.pop();
        line.push_str("\r\n");
        let back = RpcCall::from_ndjson(&line).unwrap();
        assert_eq!(back.key, "tcp_ping");

        let rec = RpcReceipt::ok("c-crlf", "tcp_ping", Some(serde_json::json!({"pong": true})));
        let rec_line = rec.to_ndjson().unwrap();
        let rec_back = RpcReceipt::from_ndjson(&rec_line).unwrap();
        assert!(rec_back.ok);
        assert_eq!(rec_back.id, call.id);
    }

    #[test]
    fn schema_rejects_illegal_call_shapes() {
        for bad in [
            serde_json::json!({"v": 2, "op": "call", "id": "c", "key": "get_item"}),
            serde_json::json!({"v": 1, "op": "invoke", "id": "c", "key": "get_item"}),
            serde_json::json!({"v": 1, "op": "call", "id": "", "key": "get_item"}),
            serde_json::json!({"v": 1, "op": "call", "id": "c", "key": "get-item"}),
            serde_json::json!({"v": 1, "op": "call", "id": "c", "key": "get_item", "transport": "grpc"}),
            serde_json::json!({"v": 1, "op": "call", "id": "c", "key": "get_item", "extra": true}),
        ] {
            assert!(
                validate_rpc_call(&bad).is_err(),
                "should reject {bad}"
            );
        }
        assert!(validate_rpc_receipt(&serde_json::json!({
            "v": 1, "op": "receipt", "id": "c", "key": "get_item"
        }))
        .is_err());
    }

    #[test]
    fn length_prefixed_round_trip_and_refuses_huge_length() {
        let mut call = RpcCall::new("c-lp", "get_item");
        call.transport = Some(Transport::Tcp);
        let framed = call.to_length_prefixed().unwrap();
        assert_eq!(
            u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize,
            framed.len() - 4
        );
        let (parts, rest) = split_length_prefixed(&framed).unwrap();
        assert!(rest.is_empty());
        assert_eq!(parts.len(), 1);
        let back: RpcCall = serde_json::from_slice(parts[0]).unwrap();
        assert_eq!(back.id, "c-lp");

        let partial = call.to_length_prefixed().unwrap();
        let tail = &partial[..3];
        let combined = [framed.as_slice(), tail].concat();
        let (parts, rest) = split_length_prefixed(&combined).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(rest.len(), 3);

        let huge = u32::MAX.to_be_bytes();
        assert!(split_length_prefixed(&huge).is_err());

        call.transport = Some(Transport::Nats);
        call.validate().unwrap();
    }
}
