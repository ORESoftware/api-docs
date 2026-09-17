//! Runtime application-header admission derived from the authored route contract.
//!
//! The route's `header_schema` is the authority for application-owned request
//! headers. Transport/runtime metadata remains outside this view and is handled
//! separately by the server/runtime boundary.

use crate::map::RouteEntry;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HeaderAdmission {
    accepted: BTreeSet<String>,
}

impl HeaderAdmission {
    pub fn from_route(route: &RouteEntry) -> Result<Self, String> {
        let mut accepted = BTreeSet::new();
        if let Some(schema) = route.header_schema.as_ref() {
            collect_schema_property_names(schema, &mut accepted)?;
        }
        Ok(Self { accepted })
    }

    #[must_use]
    pub fn accepted_names(&self) -> &BTreeSet<String> {
        &self.accepted
    }

    pub fn admit<I, K, V>(&self, headers: I) -> Result<BTreeMap<String, String>, String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        let mut admitted = BTreeMap::new();
        for (name, value) in headers {
            let canonical = canonicalize_application_header_name(name.as_ref())?;
            if !self.accepted.contains(&canonical) {
                return Err(format!(
                    "application request header `{canonical}` is not declared by the route contract"
                ));
            }
            admitted.insert(canonical, value.into());
        }
        Ok(admitted)
    }
}

fn collect_schema_property_names(
    schema: &Value,
    accepted: &mut BTreeSet<String>,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return Err(format!(
            "request header schema must be dereferenced before runtime admission; found `{reference}`"
        ));
    }

    if let Some(properties) = schema.get("properties") {
        let properties = properties
            .as_object()
            .ok_or_else(|| "request header schema properties must be an object".to_owned())?;
        for name in properties.keys() {
            accepted.insert(canonicalize_application_header_name(name)?);
        }
    }

    for combinator in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(combinator) {
            let branches = branches.as_array().ok_or_else(|| {
                format!("request header schema {combinator} must be an array")
            })?;
            for branch in branches {
                collect_schema_property_names(branch, accepted)?;
            }
        }
    }
    Ok(())
}

pub fn canonicalize_application_header_name(name: &str) -> Result<String, String> {
    let canonical = name.trim().to_ascii_lowercase();
    if !is_canonical_application_header_name(&canonical) {
        return Err(format!(
            "invalid application header name `{name}`; use a canonical lowercase HTTP token"
        ));
    }
    if is_runtime_owned_header_name(&canonical) {
        return Err(format!(
            "application header `{canonical}` is runtime-owned and must not be declared by an operation"
        ));
    }
    Ok(canonical)
}

#[must_use]
pub fn is_runtime_owned_header_name(name: &str) -> bool {
    matches!(
        name,
        "authorization"
            | "cookie"
            | "set-cookie"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "transfer-encoding"
            | "traceparent"
            | "tracestate"
            | "baggage"
    ) || name.starts_with("x-forwarded-")
        || name.starts_with("x-ores-runtime-")
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
            response_header_schema: None,
            response_trailer_schema: None,
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
    fn declared_headers_are_canonicalized_and_admitted() {
        let policy = HeaderAdmission::from_route(&route(Some(schema_with_header("x-request-id"))))
            .unwrap();
        let admitted = policy
            .admit([("X-Request-Id", "abc")])
            .expect("declared request header");
        assert_eq!(admitted.get("x-request-id").map(String::as_str), Some("abc"));
    }

    #[test]
    fn undeclared_header_fails_closed() {
        let policy = HeaderAdmission::from_route(&route(Some(schema_with_header("x-request-id"))))
            .unwrap();
        let error = policy
            .admit([("x-other", "abc")])
            .expect_err("unknown request header must fail");
        assert!(error.contains("not declared"));
    }

    #[test]
    fn runtime_owned_header_cannot_be_declared() {
        let error = HeaderAdmission::from_route(&route(Some(schema_with_header("authorization"))))
            .expect_err("authorization remains runtime-owned");
        assert!(error.contains("runtime-owned"));
    }

    #[test]
    fn composed_header_schemas_accumulate_declared_names() {
        let schema = json!({
            "allOf": [
                {"type": "object", "properties": {"x-a": {"type": "string"}}},
                {"type": "object", "properties": {"x-b": {"type": "string"}}}
            ]
        });
        let policy = HeaderAdmission::from_route(&route(Some(schema))).unwrap();
        assert_eq!(
            policy.accepted_names().iter().cloned().collect::<Vec<_>>(),
            vec!["x-a".to_owned(), "x-b".to_owned()]
        );
    }
}
