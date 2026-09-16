//! Request-header admission for RPC/business handlers.
//!
//! The raw HTTP header map belongs to transport, auth, tracing, proxy, CORS,
//! and framing middleware. This module builds a second, deliberately narrower
//! application view from a route's declared `header_schema` properties. Unknown
//! headers are not copied into that view.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::RouteEntry;

/// Request headers owned by transport/security middleware rather than business
/// RPC contracts. Keep this list aligned with RIDL v2's
/// `RUNTIME_OWNED_REQUEST_HEADERS`.
pub const RUNTIME_OWNED_REQUEST_HEADERS: &[&str] = &[
    "authorization",
    "baggage",
    "connection",
    "content-encoding",
    "content-length",
    "content-type",
    "cookie",
    "forwarded",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "set-cookie",
    "te",
    "traceparent",
    "tracestate",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
];

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HeaderAdmissionError {
    #[error("header_schema must declare an object properties map")]
    MissingProperties,
    #[error("declared header {0:?} is not a canonical lower-case HTTP field name")]
    InvalidDeclaredName(String),
    #[error("declared header {0:?} belongs to transport/security middleware")]
    RuntimeOwned(String),
    #[error("required application header {0:?} is missing")]
    MissingRequired(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeaderAdmission {
    accepted: BTreeSet<String>,
    required: BTreeSet<String>,
}

impl HeaderAdmission {
    /// Compile the application-header policy from the route contract.
    ///
    /// A route without `header_schema` accepts no business-visible headers.
    pub fn from_route(route: &RouteEntry) -> Result<Self, HeaderAdmissionError> {
        let Some(schema) = route.header_schema.as_ref() else {
            return Ok(Self::default());
        };
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .ok_or(HeaderAdmissionError::MissingProperties)?;

        let mut accepted = BTreeSet::new();
        for name in properties.keys() {
            if !is_canonical_application_header_name(name) {
                return Err(HeaderAdmissionError::InvalidDeclaredName(name.clone()));
            }
            if is_runtime_owned_request_header(name) {
                return Err(HeaderAdmissionError::RuntimeOwned(name.clone()));
            }
            accepted.insert(name.clone());
        }

        let mut required = BTreeSet::new();
        if let Some(values) = schema.get("required").and_then(serde_json::Value::as_array) {
            for value in values {
                if let Some(name) = value.as_str() {
                    if accepted.contains(name) {
                        required.insert(name.to_owned());
                    }
                }
            }
        }

        Ok(Self { accepted, required })
    }

    #[must_use]
    pub fn accepted_names(&self) -> &BTreeSet<String> {
        &self.accepted
    }

    #[must_use]
    pub fn required_names(&self) -> &BTreeSet<String> {
        &self.required
    }

    #[must_use]
    pub fn accepts(&self, name: &str) -> bool {
        self.accepted.contains(name)
    }

    #[cfg(feature = "axum")]
    pub fn project(&self, raw: &http::HeaderMap) -> Result<http::HeaderMap, HeaderAdmissionError> {
        use http::header::HeaderName;

        for name in &self.required {
            if !raw.contains_key(name.as_str()) {
                return Err(HeaderAdmissionError::MissingRequired(name.clone()));
            }
        }

        let mut projected = http::HeaderMap::new();
        for name in &self.accepted {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| HeaderAdmissionError::InvalidDeclaredName(name.clone()))?;
            for value in raw.get_all(&header_name).iter() {
                projected.append(header_name.clone(), value.clone());
            }
        }
        Ok(projected)
    }
}

#[must_use]
pub fn is_runtime_owned_request_header(name: &str) -> bool {
    RUNTIME_OWNED_REQUEST_HEADERS.contains(&name)
        || name.starts_with("x-forwarded-")
        || name.starts_with("grpc-")
}

#[must_use]
pub fn is_canonical_application_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    fn route(header_schema: Option<Value>) -> RouteEntry {
        RouteEntry {
            path: "/v1/items".into(),
            methods: vec!["POST".into()],
            summary: None,
            rpc_key: None,
            authorization: None,
            idempotency: None,
            data_classification: None,
            binding: None,
            path_params: None,
            query_schema: None,
            header_schema,
            request_schema: None,
            response_schema: None,
            error_schema: None,
            alias_of: None,
            transports: vec!["http".into()],
            tcp_framing: None,
            delivery: None,
            opto_sync: None,
        }
    }

    fn schema_with_header(name: &str) -> Value {
        let mut properties = Map::new();
        properties.insert(name.to_owned(), json!({"type": "string"}));
        json!({"type": "object", "properties": properties})
    }

    #[test]
    fn no_schema_means_empty_application_view() {
        let policy = HeaderAdmission::from_route(&route(None)).unwrap();
        assert!(policy.accepted_names().is_empty());
    }

    #[test]
    fn extracts_declared_and_required_names() {
        let policy = HeaderAdmission::from_route(&route(Some(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["idempotency-key"],
            "properties": {
                "idempotency-key": {"type": "string"},
                "x-client-version": {"type": "string"}
            }
        }))))
        .unwrap();
        assert!(policy.accepts("idempotency-key"));
        assert!(policy.accepts("x-client-version"));
        assert_eq!(
            policy.required_names().iter().cloned().collect::<Vec<_>>(),
            vec!["idempotency-key"]
        );
    }

    #[test]
    fn rejects_runtime_owned_or_noncanonical_declarations() {
        for name in [
            "authorization",
            "content-type",
            "traceparent",
            "x-forwarded-for",
            "grpc-timeout",
            "X-Client-Version",
        ] {
            let policy = HeaderAdmission::from_route(&route(Some(schema_with_header(name))));
            assert!(policy.is_err(), "{name} should not be a business header");
        }
    }

    #[cfg(feature = "axum")]
    #[test]
    fn projects_only_declared_headers_and_preserves_runtime_raw_map() {
        use http::{HeaderMap, HeaderValue};

        let policy = HeaderAdmission::from_route(&route(Some(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["idempotency-key"],
            "properties": {
                "idempotency-key": {"type": "string"},
                "x-client-version": {"type": "string"}
            }
        }))))
        .unwrap();
        let mut raw = HeaderMap::new();
        raw.insert("authorization", HeaderValue::from_static("Bearer secret"));
        raw.insert("content-type", HeaderValue::from_static("application/json"));
        raw.insert("idempotency-key", HeaderValue::from_static("abc"));
        raw.insert("x-client-version", HeaderValue::from_static("2"));
        raw.insert("x-unexpected", HeaderValue::from_static("drop-me"));

        let projected = policy.project(&raw).unwrap();
        assert_eq!(projected.len(), 2);
        assert_eq!(projected["idempotency-key"], "abc");
        assert_eq!(projected["x-client-version"], "2");
        assert!(!projected.contains_key("authorization"));
        assert!(!projected.contains_key("content-type"));
        assert!(!projected.contains_key("x-unexpected"));

        assert!(raw.contains_key("authorization"));
        assert!(raw.contains_key("content-type"));
    }

    #[cfg(feature = "axum")]
    #[test]
    fn missing_required_declared_header_fails_closed() {
        let policy = HeaderAdmission::from_route(&route(Some(json!({
            "type": "object",
            "required": ["idempotency-key"],
            "properties": {"idempotency-key": {"type": "string"}}
        }))))
        .unwrap();
        let err = policy.project(&http::HeaderMap::new()).unwrap_err();
        assert_eq!(
            err,
            HeaderAdmissionError::MissingRequired("idempotency-key".into())
        );
    }
}
