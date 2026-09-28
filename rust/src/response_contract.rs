//! Deterministic response representation/media metadata derived from the
//! authored response schema.
//!
//! TypeSpec and Draft 2020-12 JSON Schema remain the wire-shape authorities.
//! `api-docs` therefore reads response representation metadata from the same
//! schema instead of asking handler macros to repeat it as another string.
//!
//! HTTP/Lambda projection framing is intentionally a separate axis from RPC
//! semantic stream cardinality. A server-stream RPC is not synonymous with
//! HTTP chunking, SSE, NDJSON, or any other HTTP body framing.

use serde::Serialize;
use serde_json::Value;

use crate::{OperationResponseRepresentation, RpcStreamMode};

pub const RESPONSE_REPRESENTATION_EXTENSION: &str = "x-ores-response-representation";
pub const CONTENT_MEDIA_TYPE_KEY: &str = "contentMediaType";
pub const BINARY_FORMAT: &str = "binary";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResponseContractMetadata {
    pub representation: OperationResponseRepresentation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl Default for ResponseContractMetadata {
    fn default() -> Self {
        return Self {
            representation: OperationResponseRepresentation::Structured,
            content_type: None,
        };
    }
}

/// HTTP/Lambda HTTP-projection body framing.
///
/// This is deliberately independent from [`RpcStreamMode`]. `RpcStreamMode`
/// describes semantic RPC cardinality; this enum describes how an HTTP-like
/// projection serializes a response body on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpResponseFraming {
    /// One complete response body.
    #[default]
    Single,
    /// `text/event-stream` Server-Sent Events framing.
    Sse,
    /// One JSON value per line.
    Ndjson,
    /// RFC 7464 JSON text sequences.
    JsonSeq,
    /// Explicit length-prefix framing for binary/structured records.
    LengthDelimited,
    /// Opaque raw byte chunks.
    RawChunks,
}

impl HttpResponseFraming {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        return match self {
            Self::Single => "single",
            Self::Sse => "sse",
            Self::Ndjson => "ndjson",
            Self::JsonSeq => "json_seq",
            Self::LengthDelimited => "length_delimited",
            Self::RawChunks => "raw_chunks",
        };
    }

    #[must_use]
    pub const fn is_streaming(self) -> bool {
        return !matches!(self, Self::Single);
    }
}

/// Resolve one response schema into the semantic representation consumed by
/// generated `OperationSpec`s, HTTP/OpenAPI projections, lambda adapters, and
/// typed client generation.
///
/// Explicit `x-ores-response-representation` is authoritative. In its absence,
/// standards-shaped metadata is used where unambiguous:
///
/// - `contentMediaType = text/html...` => HTML
/// - another `text/*` media type => text
/// - `format = binary` => binary
/// - otherwise => structured
///
/// HTML, text, and binary representations require an explicit media type so a
/// direct HTTP/lambda adapter never guesses how to write the body.
pub fn response_contract_from_schema(
    schema: Option<&Value>,
) -> Result<ResponseContractMetadata, String> {
    let Some(schema) = schema else {
        return Ok(ResponseContractMetadata::default());
    };
    let object = schema
        .as_object()
        .ok_or_else(|| "response schema must be a JSON Schema object".to_owned())?;

    let content_type = object
        .get(CONTENT_MEDIA_TYPE_KEY)
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    format!("{CONTENT_MEDIA_TYPE_KEY} must be a non-empty media-type string")
                })
        })
        .transpose()?;

    if let Some(content_type) = content_type.as_deref() {
        validate_media_type(content_type)?;
    }

    let explicit = object
        .get(RESPONSE_REPRESENTATION_EXTENSION)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{RESPONSE_REPRESENTATION_EXTENSION} must be a string"))
                .and_then(parse_representation)
        })
        .transpose()?;

    let inferred = if content_type
        .as_deref()
        .is_some_and(|value| media_type_essence(value) == "text/html")
    {
        OperationResponseRepresentation::Html
    } else if content_type
        .as_deref()
        .is_some_and(|value| media_type_essence(value).starts_with("text/"))
    {
        OperationResponseRepresentation::Text
    } else if object.get("format").and_then(Value::as_str) == Some(BINARY_FORMAT) {
        OperationResponseRepresentation::Binary
    } else {
        OperationResponseRepresentation::Structured
    };

    let representation = explicit.unwrap_or(inferred);
    validate_schema_representation_pair(object, representation)?;
    validate_media_pair(representation, content_type.as_deref())?;

    return Ok(ResponseContractMetadata {
        representation,
        content_type,
    });
}

/// Validate the fifth response-contract axis: HTTP/Lambda HTTP-projection
/// framing.
///
/// The semantic operation remains transport-neutral. Callers should obtain
/// `stream` from `OperationSpec::STREAM`, response representation/media from
/// the normalized response schema, and framing from the HTTP projection
/// (`#[ores_route]` / normalized route IR). This function rejects combinations
/// that would otherwise silently buffer a stream, invent stream semantics for
/// a unary operation, or use a framing incompatible with the declared media.
pub fn validate_http_response_framing(
    stream: RpcStreamMode,
    response: &ResponseContractMetadata,
    framing: HttpResponseFraming,
) -> Result<(), String> {
    match stream {
        RpcStreamMode::Unary => {
            if framing != HttpResponseFraming::Single {
                return Err(format!(
                    "unary operation cannot use streaming HTTP/Lambda framing {:?}",
                    framing.as_str()
                ));
            }
        }
        RpcStreamMode::ServerStream => {
            if framing == HttpResponseFraming::Single {
                return Err(
                    "server_stream operation requires explicit streaming HTTP/Lambda framing; refusing to buffer into one body"
                        .to_owned(),
                );
            }
        }
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => {
            return Err(format!(
                "{} does not have a canonical HTTP/Lambda response projection ABI; refuse projection instead of guessing",
                stream.as_str()
            ));
        }
    }

    let content_type = response.content_type.as_deref();
    match framing {
        HttpResponseFraming::Single => {
            return Ok(());
        }
        HttpResponseFraming::Sse => {
            require_media_type(content_type, "text/event-stream", "SSE")?;
            if matches!(
                response.representation,
                OperationResponseRepresentation::Html | OperationResponseRepresentation::Binary
            ) {
                return Err(
                    "SSE framing requires structured or text response representation".to_owned(),
                );
            }
        }
        HttpResponseFraming::Ndjson => {
            require_structured(response, "NDJSON")?;
            require_one_of_media_types(
                content_type,
                &["application/x-ndjson", "application/ndjson", "application/jsonl"],
                "NDJSON",
            )?;
        }
        HttpResponseFraming::JsonSeq => {
            require_structured(response, "JSON-seq")?;
            require_media_type(content_type, "application/json-seq", "JSON-seq")?;
        }
        HttpResponseFraming::LengthDelimited => {
            if matches!(
                response.representation,
                OperationResponseRepresentation::Html | OperationResponseRepresentation::Text
            ) {
                return Err(
                    "length-delimited framing cannot be used for HTML/text response representation"
                        .to_owned(),
                );
            }
            let content_type = content_type.ok_or_else(|| {
                "length-delimited framing requires an explicit response content type".to_owned()
            })?;
            if media_type_essence(content_type).starts_with("text/") {
                return Err(format!(
                    "length-delimited framing cannot use text media type {content_type:?}"
                ));
            }
        }
        HttpResponseFraming::RawChunks => {
            if response.representation != OperationResponseRepresentation::Binary {
                return Err("raw chunk framing requires binary response representation".to_owned());
            }
            let content_type = content_type.ok_or_else(|| {
                "raw chunk framing requires an explicit binary response content type".to_owned()
            })?;
            if media_type_essence(content_type).starts_with("text/") {
                return Err(format!(
                    "raw chunk framing cannot use text media type {content_type:?}"
                ));
            }
        }
    }

    return Ok(());
}

fn parse_representation(value: &str) -> Result<OperationResponseRepresentation, String> {
    return match value {
        "structured" => Ok(OperationResponseRepresentation::Structured),
        "html" => Ok(OperationResponseRepresentation::Html),
        "text" => Ok(OperationResponseRepresentation::Text),
        "binary" => Ok(OperationResponseRepresentation::Binary),
        other => Err(format!(
            "unsupported {RESPONSE_REPRESENTATION_EXTENSION} value {other:?}; expected structured, html, text, or binary"
        )),
    };
}

fn validate_schema_representation_pair(
    object: &serde_json::Map<String, Value>,
    representation: OperationResponseRepresentation,
) -> Result<(), String> {
    if representation == OperationResponseRepresentation::Structured {
        return Ok(());
    }

    let Some(schema_type) = object.get("type") else {
        return Ok(());
    };
    let supports_string = match schema_type {
        Value::String(value) => value == "string",
        Value::Array(values) => values.iter().any(|value| value.as_str() == Some("string")),
        _ => false,
    };
    if !supports_string {
        return Err(format!(
            "{} response representation requires a JSON Schema string body when type is declared",
            representation.as_str()
        ));
    }

    return Ok(());
}

fn validate_media_pair(
    representation: OperationResponseRepresentation,
    content_type: Option<&str>,
) -> Result<(), String> {
    match representation {
        OperationResponseRepresentation::Structured => {
            if content_type.is_some_and(|value| {
                let essence = media_type_essence(value);
                essence == "text/html" || essence.starts_with("text/")
            }) {
                return Err(
                    "structured response representation conflicts with text/HTML contentMediaType"
                        .to_owned(),
                );
            }
        }
        OperationResponseRepresentation::Html => {
            let content_type = content_type.ok_or_else(|| {
                "html response representation requires contentMediaType".to_owned()
            })?;
            if media_type_essence(content_type) != "text/html" {
                return Err(format!(
                    "html response representation requires text/html contentMediaType, got {content_type:?}"
                ));
            }
        }
        OperationResponseRepresentation::Text => {
            let content_type = content_type.ok_or_else(|| {
                "text response representation requires contentMediaType".to_owned()
            })?;
            let essence = media_type_essence(content_type);
            if !essence.starts_with("text/") || essence == "text/html" {
                return Err(format!(
                    "text response representation requires non-HTML text/* contentMediaType, got {content_type:?}"
                ));
            }
        }
        OperationResponseRepresentation::Binary => {
            let content_type = content_type.ok_or_else(|| {
                "binary response representation requires contentMediaType".to_owned()
            })?;
            let essence = media_type_essence(content_type);
            if essence.starts_with("text/") || is_json_media_type(&essence) {
                return Err(format!(
                    "binary response representation cannot use text/JSON contentMediaType {content_type:?}"
                ));
            }
        }
    }
    return Ok(());
}

fn require_structured(response: &ResponseContractMetadata, framing: &str) -> Result<(), String> {
    if response.representation != OperationResponseRepresentation::Structured {
        return Err(format!(
            "{framing} framing requires structured response representation"
        ));
    }
    return Ok(());
}

fn require_media_type(
    content_type: Option<&str>,
    expected: &str,
    framing: &str,
) -> Result<(), String> {
    let content_type = content_type
        .ok_or_else(|| format!("{framing} framing requires content type {expected}"))?;
    if media_type_essence(content_type) != expected {
        return Err(format!(
            "{framing} framing requires content type {expected}, got {content_type:?}"
        ));
    }
    return Ok(());
}

fn require_one_of_media_types(
    content_type: Option<&str>,
    expected: &[&str],
    framing: &str,
) -> Result<(), String> {
    let content_type = content_type.ok_or_else(|| {
        format!(
            "{framing} framing requires one of these content types: {}",
            expected.join(", ")
        )
    })?;
    let essence = media_type_essence(content_type);
    if !expected.iter().any(|candidate| essence == *candidate) {
        return Err(format!(
            "{framing} framing requires one of these content types: {}; got {content_type:?}",
            expected.join(", ")
        ));
    }
    return Ok(());
}

fn validate_media_type(value: &str) -> Result<(), String> {
    let essence = media_type_essence(value);
    let mut pieces = essence.split('/');
    let Some(type_name) = pieces.next() else {
        return Err(format!("invalid media type {value:?}"));
    };
    let Some(subtype) = pieces.next() else {
        return Err(format!("invalid media type {value:?}: missing subtype"));
    };
    if pieces.next().is_some()
        || type_name.is_empty()
        || subtype.is_empty()
        || !type_name.chars().all(is_media_token_char)
        || !subtype.chars().all(is_media_token_char)
    {
        return Err(format!("invalid media type {value:?}"));
    }
    return Ok(());
}

fn is_media_token_char(value: char) -> bool {
    return value.is_ascii_alphanumeric()
        || matches!(
            value,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        );
}

fn is_json_media_type(essence: &str) -> bool {
    return essence == "application/json" || essence.ends_with("+json");
}

fn media_type_essence(value: &str) -> String {
    return value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ordinary_object_schema_defaults_to_structured() {
        let schema = json!({"type":"object","properties":{"id":{"type":"string"}}});
        assert_eq!(
            response_contract_from_schema(Some(&schema)).expect("contract"),
            ResponseContractMetadata::default()
        );
    }

    #[test]
    fn html_is_inferred_from_standard_media_metadata() {
        let schema = json!({
            "type":"string",
            "contentMediaType":"text/html; charset=utf-8"
        });
        let contract = response_contract_from_schema(Some(&schema)).expect("contract");
        assert_eq!(
            contract.representation,
            OperationResponseRepresentation::Html
        );
        assert_eq!(
            contract.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
    }

    #[test]
    fn binary_is_explicit_and_media_typed() {
        let schema = json!({
            "type":"string",
            "format":"binary",
            "contentMediaType":"application/octet-stream"
        });
        let contract = response_contract_from_schema(Some(&schema)).expect("contract");
        assert_eq!(
            contract.representation,
            OperationResponseRepresentation::Binary
        );
        assert_eq!(
            contract.content_type.as_deref(),
            Some("application/octet-stream")
        );
    }

    #[test]
    fn extension_can_make_representation_exact() {
        let schema = json!({
            "type":"string",
            "x-ores-response-representation":"text",
            "contentMediaType":"text/event-stream"
        });
        let contract = response_contract_from_schema(Some(&schema)).expect("contract");
        assert_eq!(
            contract.representation,
            OperationResponseRepresentation::Text
        );
    }

    #[test]
    fn contradictory_representation_fails_closed() {
        let schema = json!({
            "type":"string",
            "x-ores-response-representation":"structured",
            "contentMediaType":"text/html"
        });
        assert!(response_contract_from_schema(Some(&schema)).is_err());
    }

    #[test]
    fn binary_requires_explicit_media_type() {
        let schema = json!({"type":"string","format":"binary"});
        assert!(response_contract_from_schema(Some(&schema)).is_err());
    }

    #[test]
    fn non_structured_representation_rejects_non_string_schema_type() {
        let schema = json!({
            "type":"object",
            "x-ores-response-representation":"html",
            "contentMediaType":"text/html"
        });
        assert!(response_contract_from_schema(Some(&schema)).is_err());
    }

    #[test]
    fn binary_rejects_json_media_type() {
        let schema = json!({
            "type":"string",
            "format":"binary",
            "contentMediaType":"application/problem+json"
        });
        assert!(response_contract_from_schema(Some(&schema)).is_err());
    }

    #[test]
    fn malformed_media_type_fails_closed() {
        let schema = json!({
            "type":"string",
            "x-ores-response-representation":"text",
            "contentMediaType":"text"
        });
        assert!(response_contract_from_schema(Some(&schema)).is_err());
    }

    #[test]
    fn unary_cannot_claim_streaming_http_framing() {
        let response = ResponseContractMetadata {
            representation: OperationResponseRepresentation::Text,
            content_type: Some("text/event-stream".to_owned()),
        };
        assert!(validate_http_response_framing(
            RpcStreamMode::Unary,
            &response,
            HttpResponseFraming::Sse
        )
        .is_err());
    }

    #[test]
    fn server_stream_cannot_buffer_into_single_body() {
        let response = ResponseContractMetadata {
            representation: OperationResponseRepresentation::Structured,
            content_type: Some("application/json".to_owned()),
        };
        assert!(validate_http_response_framing(
            RpcStreamMode::ServerStream,
            &response,
            HttpResponseFraming::Single
        )
        .is_err());
    }

    #[test]
    fn structured_server_stream_can_use_ndjson() {
        let response = ResponseContractMetadata {
            representation: OperationResponseRepresentation::Structured,
            content_type: Some("application/x-ndjson; charset=utf-8".to_owned()),
        };
        validate_http_response_framing(
            RpcStreamMode::ServerStream,
            &response,
            HttpResponseFraming::Ndjson,
        )
        .expect("NDJSON stream should be admitted");
    }

    #[test]
    fn raw_chunks_require_binary_representation() {
        let structured = ResponseContractMetadata {
            representation: OperationResponseRepresentation::Structured,
            content_type: Some("application/octet-stream".to_owned()),
        };
        assert!(validate_http_response_framing(
            RpcStreamMode::ServerStream,
            &structured,
            HttpResponseFraming::RawChunks
        )
        .is_err());

        let binary = ResponseContractMetadata {
            representation: OperationResponseRepresentation::Binary,
            content_type: Some("application/octet-stream".to_owned()),
        };
        validate_http_response_framing(
            RpcStreamMode::ServerStream,
            &binary,
            HttpResponseFraming::RawChunks,
        )
        .expect("binary raw chunks should be admitted");
    }

    #[test]
    fn bidi_http_projection_fails_closed_until_canonical_abi_exists() {
        let response = ResponseContractMetadata::default();
        assert!(validate_http_response_framing(
            RpcStreamMode::Bidi,
            &response,
            HttpResponseFraming::RawChunks
        )
        .is_err());
    }
}
