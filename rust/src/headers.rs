//! Hardened docs headers. Pattern from t2v-v2t.rs in k8s-cluster.

use serde_json::{json, Value};

use crate::schema::{validate_headers, SchemaError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Html,
    Json,
}

#[must_use]
pub fn hardening_headers(kind: BodyKind) -> Vec<(&'static str, &'static str)> {
    let mut h = vec![
        ("Cache-Control", "no-store"),
        ("Pragma", "no-cache"),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        (
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=()",
        ),
    ];
    match kind {
        BodyKind::Html => {
            h.push(("Content-Type", "text/html; charset=utf-8"));
            h.push(("X-Frame-Options", "DENY"));
            h.push((
                "Content-Security-Policy",
                "default-src 'none'; style-src 'unsafe-inline'; img-src 'none'; frame-ancestors 'none'; base-uri 'none'; object-src 'none'; form-action 'none'; connect-src 'none'; script-src 'none'",
            ));
        }
        BodyKind::Json => {
            h.push(("Content-Type", "application/json; charset=utf-8"));
        }
    }
    h
}

pub fn headers_as_json(kind: BodyKind) -> Result<Value, SchemaError> {
    let mut obj = serde_json::Map::new();
    for (k, v) in hardening_headers(kind) {
        obj.insert(k.to_string(), json!(v));
    }
    let value = Value::Object(obj);
    validate_headers(&value)?;
    Ok(value)
}

#[must_use]
pub fn method_not_allowed_headers() -> Vec<(&'static str, &'static str)> {
    let mut h = hardening_headers(BodyKind::Json);
    h.push(("Allow", "GET, HEAD"));
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_and_json_headers_match_schema() {
        headers_as_json(BodyKind::Html).unwrap();
        headers_as_json(BodyKind::Json).unwrap();
    }

    #[test]
    fn no_cdn_and_no_store() {
        let html = hardening_headers(BodyKind::Html);
        let joined = html
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("no-store"));
        assert!(joined.contains("frame-ancestors 'none'"));
        assert!(!joined.contains("unpkg"));
        assert!(!joined.contains("cdn"));
        assert!(!joined.contains("scalar"));
    }
}
