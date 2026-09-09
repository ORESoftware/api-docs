//! Versioned public errors only. No field values, provider messages or sync state.
//! Independent TypeSpec/JSON Schema authorities live in `../contracts`.
use ores_form_validation::Code;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "ores.form-validation/v1";
pub const MAX_ISSUES: usize = 128;
pub const CODES: [&str; 18] = [
    "too_large",
    "required",
    "blank",
    "min_chars",
    "max_chars",
    "min_lines",
    "max_lines",
    "email",
    "phone_e164",
    "number",
    "integer",
    "unsafe_integer",
    "minimum",
    "maximum",
    "date",
    "date_min",
    "date_max",
    "invalid_unicode",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidMessage;
impl std::fmt::Display for InvalidMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid validation message")
    }
}
impl std::error::Error for InvalidMessage {}

fn valid_field(field: &str) -> bool {
    let bytes = field.as_bytes();
    (1..=128).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(b))
}

/// Immutable validated identifier/code pair. Construction and Serde both check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawIssue", into = "RawIssue")]
pub struct ValidationIssue {
    field: String,
    code: String,
}
impl ValidationIssue {
    pub fn new(field: &str, code: &str) -> Result<Self, InvalidMessage> {
        if !valid_field(field) || !CODES.contains(&code) {
            return Err(InvalidMessage);
        }
        Ok(Self {
            field: field.to_owned(),
            code: code.to_owned(),
        })
    }
    pub fn from_code(field: &str, code: Code) -> Result<Self, InvalidMessage> {
        Self::new(field, code.as_str())
    }
    pub fn field(&self) -> &str {
        &self.field
    }
    pub fn code(&self) -> &str {
        &self.code
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIssue {
    field: String,
    code: String,
}
impl TryFrom<RawIssue> for ValidationIssue {
    type Error = InvalidMessage;
    fn try_from(raw: RawIssue) -> Result<Self, Self::Error> {
        Self::new(&raw.field, &raw.code)
    }
}
impl From<ValidationIssue> for RawIssue {
    fn from(issue: ValidationIssue) -> Self {
        Self {
            field: issue.field,
            code: issue.code,
        }
    }
}

/// Empty issues means no local errors, never authorization or server acceptance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawMessage", into = "RawMessage")]
pub struct ValidationMessage {
    issues: Vec<ValidationIssue>,
}
impl ValidationMessage {
    pub fn new(issues: Vec<ValidationIssue>) -> Result<Self, InvalidMessage> {
        if issues.len() > MAX_ISSUES {
            return Err(InvalidMessage);
        }
        Ok(Self { issues })
    }
    pub fn from_codes(field: &str, codes: &[Code]) -> Result<Self, InvalidMessage> {
        if !valid_field(field) || codes.len() > MAX_ISSUES {
            return Err(InvalidMessage);
        }
        Self::new(
            codes
                .iter()
                .map(|&code| ValidationIssue::from_code(field, code))
                .collect::<Result<_, _>>()?,
        )
    }
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMessage {
    schema_version: String,
    issues: Vec<ValidationIssue>,
}
impl TryFrom<RawMessage> for ValidationMessage {
    type Error = InvalidMessage;
    fn try_from(raw: RawMessage) -> Result<Self, Self::Error> {
        if raw.schema_version != SCHEMA_VERSION {
            return Err(InvalidMessage);
        }
        Self::new(raw.issues)
    }
}
impl From<ValidationMessage> for RawMessage {
    fn from(message: ValidationMessage) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            issues: message.issues,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ores_form_validation::{FieldValidator, Kind, Rules};
    use serde_json::json;

    #[test]
    fn actual_validator_outputs_round_trip_without_values() {
        let validator = FieldValidator::new(Rules {
            kind: Kind::Email,
            required: true,
            ..Rules::default()
        })
        .unwrap();
        let message =
            ValidationMessage::from_codes("email", &validator.validate(Some("private-sentinel")))
                .unwrap();
        let encoded = serde_json::to_string(&message).unwrap();
        assert!(!encoded.contains("private-sentinel"));
        assert_eq!(
            serde_json::from_str::<ValidationMessage>(&encoded).unwrap(),
            message
        );
        assert_eq!(message.issues()[0].code(), "email");
    }

    #[test]
    fn rejects_unknown_fields_codes_versions_and_limits() {
        for value in [
            json!({"schema_version": SCHEMA_VERSION, "issues": [], "value": "private"}),
            json!({"schema_version": "next", "issues": []}),
            json!({"schema_version": SCHEMA_VERSION, "issues": [{"field": "email", "code": "other"}]}),
            json!({"schema_version": SCHEMA_VERSION, "issues": [{"field": "email\n", "code": "email"}]}),
            json!({"schema_version": SCHEMA_VERSION, "issues": [{"field": "email", "code": "email", "value": "private"}]}),
        ] {
            assert!(serde_json::from_value::<ValidationMessage>(value).is_err());
        }
        let issue = ValidationIssue::new("a", "email").unwrap();
        assert!(ValidationMessage::new(vec![issue.clone(); 128]).is_ok());
        assert!(ValidationMessage::new(vec![issue; 129]).is_err());
        assert!(ValidationIssue::new(&"a".repeat(128), "required").is_ok());
        assert!(ValidationIssue::new(&"a".repeat(129), "required").is_err());
        for code in CODES {
            assert!(ValidationIssue::new("field", code).is_ok());
        }
    }
}
