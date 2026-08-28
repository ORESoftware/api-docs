//! Embedded JSON Schema documents and validators.

use std::sync::OnceLock;

use jsonschema::Validator;
use serde_json::Value;
use thiserror::Error;

pub const ROUTE_MAP_SCHEMA_JSON: &str =
    include_str!("../../json-schema/route-map.schema.json");
pub const CATALOG_SCHEMA_JSON: &str = include_str!("../../json-schema/catalog.schema.json");
pub const OPENAPI_SCHEMA_JSON: &str =
    include_str!("../../json-schema/openapi-3.1-subset.schema.json");
pub const OPENRPC_SCHEMA_JSON: &str =
    include_str!("../../json-schema/openrpc-1.3-subset.schema.json");
pub const CONNECT_SCHEMA_JSON: &str =
    include_str!("../../json-schema/connect-json-unary.schema.json");
pub const HYPER_SCHEMA_JSON: &str =
    include_str!("../../json-schema/json-hyper-schema-links.schema.json");
pub const HEADERS_SCHEMA_JSON: &str =
    include_str!("../../json-schema/hardening-headers.schema.json");
pub const BINDING_SCHEMA_JSON: &str =
    include_str!("../../json-schema/route-binding.schema.json");
pub const LANGUAGE_SURFACE_SCHEMA_JSON: &str =
    include_str!("../../json-schema/language-surface.schema.json");
pub const OPTO_SYNC_ENVELOPE_SCHEMA_JSON: &str =
    include_str!("../../json-schema/opto-sync-envelope.schema.json");

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("invalid schema document {name}: {detail}")]
    Compile { name: &'static str, detail: String },
    #[error("{name}: {detail}")]
    Instance { name: &'static str, detail: String },
}

fn compile(name: &'static str, src: &str) -> Result<Validator, SchemaError> {
    let schema: Value = serde_json::from_str(src).map_err(|e| SchemaError::Compile {
        name,
        detail: e.to_string(),
    })?;
    jsonschema::validator_for(&schema).map_err(|e| SchemaError::Compile {
        name,
        detail: e.to_string(),
    })
}

fn validator(name: &'static str, src: &'static str) -> Result<&'static Validator, SchemaError> {
    match name {
        "route-map" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "catalog" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "openapi" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "openrpc" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "connect" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "hyper" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "headers" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "binding" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "language-surface" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        "opto-sync-envelope" => {
            static V: OnceLock<Result<Validator, String>> = OnceLock::new();
            lock_get(&V, name, src)
        }
        _ => Err(SchemaError::Compile {
            name,
            detail: "unknown schema".into(),
        }),
    }
}

fn lock_get(
    cell: &'static OnceLock<Result<Validator, String>>,
    name: &'static str,
    src: &'static str,
) -> Result<&'static Validator, SchemaError> {
    let stored = cell.get_or_init(|| compile(name, src).map_err(|e| e.to_string()));
    match stored {
        Ok(v) => Ok(v),
        Err(detail) => Err(SchemaError::Compile {
            name,
            detail: detail.clone(),
        }),
    }
}

fn check(name: &'static str, src: &'static str, instance: &Value) -> Result<(), SchemaError> {
    let v = validator(name, src)?;
    let err = v.iter_errors(instance).next();
    if let Some(e) = err {
        return Err(SchemaError::Instance {
            name,
            detail: format!("{e}"),
        });
    }
    Ok(())
}

pub fn validate_route_map(instance: &Value) -> Result<(), SchemaError> {
    check("route-map", ROUTE_MAP_SCHEMA_JSON, instance)
}

pub fn validate_catalog(instance: &Value) -> Result<(), SchemaError> {
    check("catalog", CATALOG_SCHEMA_JSON, instance)
}

pub fn validate_openapi(instance: &Value) -> Result<(), SchemaError> {
    check("openapi", OPENAPI_SCHEMA_JSON, instance)
}

pub fn validate_openrpc(instance: &Value) -> Result<(), SchemaError> {
    check("openrpc", OPENRPC_SCHEMA_JSON, instance)
}

pub fn validate_connect(instance: &Value) -> Result<(), SchemaError> {
    check("connect", CONNECT_SCHEMA_JSON, instance)
}

pub fn validate_hyper_schema(instance: &Value) -> Result<(), SchemaError> {
    check("hyper", HYPER_SCHEMA_JSON, instance)
}

pub fn validate_headers(instance: &Value) -> Result<(), SchemaError> {
    check("headers", HEADERS_SCHEMA_JSON, instance)
}

pub fn validate_binding(instance: &Value) -> Result<(), SchemaError> {
    check("binding", BINDING_SCHEMA_JSON, instance)
}

pub fn validate_language_surface(instance: &Value) -> Result<(), SchemaError> {
    check("language-surface", LANGUAGE_SURFACE_SCHEMA_JSON, instance)
}

pub fn validate_opto_sync_envelope(instance: &Value) -> Result<(), SchemaError> {
    check("opto-sync-envelope", OPTO_SYNC_ENVELOPE_SCHEMA_JSON, instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_json() {
        for src in [
            ROUTE_MAP_SCHEMA_JSON,
            CATALOG_SCHEMA_JSON,
            OPENAPI_SCHEMA_JSON,
            OPENRPC_SCHEMA_JSON,
            CONNECT_SCHEMA_JSON,
            HYPER_SCHEMA_JSON,
            HEADERS_SCHEMA_JSON,
            BINDING_SCHEMA_JSON,
            LANGUAGE_SURFACE_SCHEMA_JSON,
            OPTO_SYNC_ENVELOPE_SCHEMA_JSON,
        ] {
            let _: Value = serde_json::from_str(src).unwrap();
        }
    }

    #[test]
    fn empty_binding_is_rejected() {
        let err = validate_binding(&serde_json::json!({})).unwrap_err();
        assert!(format!("{err}").contains("binding"));
    }

    #[test]
    fn combination_binding_is_ok() {
        validate_binding(&serde_json::json!({
            "annotation": "post",
            "param_types": ["CheckFieldSanityRequest"],
            "return_type": "CheckFieldSanityResponse",
            "function_type": "UnaryFn<CheckFieldSanityRequest, CheckFieldSanityResponse>"
        }))
        .unwrap();
    }
}
