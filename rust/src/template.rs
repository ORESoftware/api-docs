//! RFC 6570 level-1 path templates (`/v1/matters/{id}`).

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TemplateError {
    #[error("{0}")]
    Semantic(String),
}

/// `{name}` placeholders. Names are `[A-Za-z_][A-Za-z0-9_]*`.
pub fn path_template_vars(path: &str) -> Result<Vec<String>, TemplateError> {
    let mut vars = Vec::new();
    let mut seen = BTreeSet::new();
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let close = path[i + 1..]
                .find('}')
                .map(|rel| i + 1 + rel)
                .ok_or_else(|| TemplateError::Semantic("unclosed { in path".into()))?;
            let name = &path[i + 1..close];
            if name.is_empty()
                || !name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(TemplateError::Semantic(format!(
                    "invalid path placeholder {{{name}}}"
                )));
            }
            if !seen.insert(name.to_string()) {
                return Err(TemplateError::Semantic(format!(
                    "duplicate path placeholder {{{name}}}"
                )));
            }
            vars.push(name.to_string());
            i = close + 1;
            continue;
        }
        if bytes[i] == b'}' {
            return Err(TemplateError::Semantic("unmatched } in path".into()));
        }
        i += 1;
    }
    Ok(vars)
}

/// Substitute `{name}` with URL-encoded values. Extra or missing keys fail.
///
/// Path values are intentionally stricter than query values: dot-segment
/// traversal components, backslash separators, and control characters are
/// rejected before percent-encoding. Forward slashes remain legal level-1
/// values and are encoded as `%2F`, preserving the existing api-docs contract.
pub fn expand_path(
    template: &str,
    params: &BTreeMap<String, String>,
) -> Result<String, TemplateError> {
    let vars = path_template_vars(template)?;
    let needed: BTreeSet<&str> = vars.iter().map(String::as_str).collect();
    let given: BTreeSet<&str> = params.keys().map(String::as_str).collect();
    if needed != given {
        return Err(TemplateError::Semantic(format!(
            "path params mismatch: template {needed:?} vs given {given:?}"
        )));
    }
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    let bytes = template.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let close = template[i + 1..]
                .find('}')
                .map(|rel| i + 1 + rel)
                .expect("validated");
            let name = &template[i + 1..close];
            let value = params.get(name).expect("validated");
            validate_path_value(value)?;
            out.push_str(&encode_path_segment(value));
            i = close + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok(out)
}

fn validate_path_value(value: &str) -> Result<(), TemplateError> {
    if value == "." || value == ".." {
        return Err(TemplateError::Semantic(
            "path parameter cannot be a dot-segment traversal component".into(),
        ));
    }
    if value.contains('\\') {
        return Err(TemplateError::Semantic(
            "path parameter cannot contain backslash separators".into(),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(TemplateError::Semantic(
            "path parameter cannot contain control characters".into(),
        ));
    }
    Ok(())
}

fn encode_path_segment(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Encode a query object as `k=v&k2=v2` in sorted key order. Values must be
/// strings, numbers, or booleans (arrays become repeated keys).
pub fn encode_query(query: &BTreeMap<String, QueryValue>) -> String {
    let mut parts = Vec::new();
    for (key, value) in query {
        match value {
            QueryValue::Repeat(items) => {
                for item in items {
                    parts.push(format!(
                        "{}={}",
                        encode_path_segment(key),
                        encode_path_segment(item)
                    ));
                }
            }
            QueryValue::One(item) => {
                parts.push(format!(
                    "{}={}",
                    encode_path_segment(key),
                    encode_path_segment(item)
                ));
            }
        }
    }
    parts.join("&")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryValue {
    One(String),
    Repeat(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_expands() {
        let vars = path_template_vars("/v1/matters/{id}/walk").unwrap();
        assert_eq!(vars, vec!["id"]);
        let mut params = BTreeMap::new();
        params.insert("id".into(), "a/b".into());
        assert_eq!(
            expand_path("/v1/matters/{id}/walk", &params).unwrap(),
            "/v1/matters/a%2Fb/walk"
        );
    }

    #[test]
    fn rejects_unsafe_path_values() {
        for value in [".", "..", "a\\b", "bad\nvalue", "bad\0value"] {
            let mut params = BTreeMap::new();
            params.insert("id".into(), value.into());
            assert!(expand_path("/v1/{id}", &params).is_err(), "{value:?}");
        }
    }

    #[test]
    fn rejects_mismatch() {
        let params = BTreeMap::new();
        assert!(expand_path("/v1/{id}", &params).is_err());
    }
}
