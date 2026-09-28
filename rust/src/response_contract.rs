//! Deterministic response representation/media metadata derived from the
//! authored response schema.
//!
//! TypeSpec and Draft 2020-12 JSON Schema remain the wire-shape authorities.
//! `api-docs` therefore reads response representation metadata from the same
//! schema instead of asking handler macros to repeat it as another string.

use serde::Serialize;
use serde_json::Value;

use crate::OperationResponseRepresentation;

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
        Self {
            representation: OperationResponseRepresentation::Structured,
            content_type: None,
        }
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
    validate_media_pair(representation, content_type.as_deref())?;

    Ok(ResponseContractMetadata {
        representation,
        content_type,
    })
}

fn parse_representation(value: &str) -> Result<OperationResponseRepresentation, String> {
    match value {
        "structured" => Ok(OperationResponseRepresentation::Structured),
        "html" => Ok(OperationResponseRepresentation::Html),
        "text" => Ok(OperationResponseRepresentation::Text),
        "binary" => Ok(OperationResponseRepresentation::Binary),
        other => Err(format!(
            "unsupported {RESPONSE_REPRESENTATION_EXTENSION} value {other:?}; expected structured, html, text, or binary"
        )),
    }
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
            if media_type_essence(content_type).starts_with("text/") {
                return Err(format!(
                    "binary response representation cannot use text contentMediaType {content_type:?}"
                ));
            }
        }
    }
    Ok(())
}

fn media_type_essence(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
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
}
