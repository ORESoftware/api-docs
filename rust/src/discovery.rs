//! Stable, host-agnostic discovery document for MCP and other API-docs clients.
//!
//! All routes are relative on purpose. A caller supplies the trusted service
//! origin; forwarded headers and request host data are never reflected into the
//! manifest.

use serde::Serialize;

use crate::catalog::Catalog;
use crate::paths::{
    CATALOG_ROUTE, CONNECT_ROUTE, DISCOVERY_ROUTE, DOCS_ALIAS_ROUTES, HTML_ROUTE,
    OPENAPI_ROUTE, OPENRPC_ROUTE,
};
use crate::project::contract_sha256;

pub const DISCOVERY_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocsProjectionRoutes {
    pub openapi: &'static str,
    pub openrpc: &'static str,
    pub connect: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsDiscoveryManifest {
    pub schema_version: &'static str,
    pub service: String,
    pub contract_sha256: String,
    pub route_count: usize,
    pub discovery: &'static str,
    pub html: &'static str,
    pub catalog: &'static str,
    pub projections: DocsProjectionRoutes,
    pub aliases: Vec<&'static str>,
}

impl DocsDiscoveryManifest {
    #[must_use]
    pub fn from_catalog(catalog: &Catalog) -> Self {
        Self {
            schema_version: DISCOVERY_SCHEMA_VERSION,
            service: catalog.map.service.clone(),
            contract_sha256: contract_sha256(&catalog.map),
            route_count: catalog.map.map.len(),
            discovery: DISCOVERY_ROUTE,
            html: HTML_ROUTE,
            catalog: CATALOG_ROUTE,
            projections: DocsProjectionRoutes {
                openapi: OPENAPI_ROUTE,
                openrpc: OPENRPC_ROUTE,
                connect: CONNECT_ROUTE,
            },
            aliases: DOCS_ALIAS_ROUTES.to_vec(),
        }
    }

    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::RouteMap;
    use serde_json::Value;

    fn manifest() -> DocsDiscoveryManifest {
        let map = RouteMap::from_json_str(include_str!("../../examples/pmap-api.route-map.json"))
            .expect("example route map");
        let catalog = Catalog::from_map(map).expect("example catalog");
        DocsDiscoveryManifest::from_catalog(&catalog)
    }

    #[test]
    fn manifest_is_relative_digest_bound_and_schema_valid() {
        let manifest = manifest();
        assert_eq!(manifest.discovery, "/api-docs/manifest.json");
        assert_eq!(manifest.catalog, "/api/docs.json");
        assert_eq!(manifest.html, "/docs/api");
        assert_eq!(manifest.contract_sha256.len(), 64);
        assert!(manifest
            .contract_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        assert!(manifest.route_count > 0);

        for path in manifest
            .aliases
            .iter()
            .copied()
            .chain([
                manifest.discovery,
                manifest.html,
                manifest.catalog,
                manifest.projections.openapi,
                manifest.projections.openrpc,
                manifest.projections.connect,
            ])
        {
            assert!(path.starts_with('/'), "route must be relative: {path}");
            assert!(!path.starts_with("//"), "network-path reference: {path}");
            assert!(!path.contains(".."), "traversal-like route: {path}");
            assert!(!path.contains('?'), "query not permitted: {path}");
            assert!(!path.contains('#'), "fragment not permitted: {path}");
        }

        let instance = serde_json::to_value(&manifest).expect("manifest JSON");
        let schema: Value = serde_json::from_str(include_str!(
            "../../json-schema/docs-discovery.schema.json"
        ))
        .expect("discovery JSON Schema");
        jsonschema::validator_for(&schema)
            .expect("valid discovery schema")
            .validate(&instance)
            .expect("manifest conforms to JSON Schema authority");
    }

    #[test]
    fn typespec_and_json_schema_route_literals_match_runtime() {
        let typespec = include_str!("../../idl/typespec/docs-discovery.tsp");
        let schema: Value = serde_json::from_str(include_str!(
            "../../json-schema/docs-discovery.schema.json"
        ))
        .expect("discovery JSON Schema");
        let properties = schema["properties"]
            .as_object()
            .expect("top-level properties");

        for (name, path) in [
            ("discovery", DISCOVERY_ROUTE),
            ("html", HTML_ROUTE),
            ("catalog", CATALOG_ROUTE),
        ] {
            assert_eq!(properties[name]["const"], path);
            assert!(
                typespec.contains(&format!("{name}: \"{path}\";")),
                "TypeSpec authority is missing {name}={path}"
            );
        }

        let projections = schema["properties"]["projections"]["properties"]
            .as_object()
            .expect("projection properties");
        for (name, path) in [
            ("openapi", OPENAPI_ROUTE),
            ("openrpc", OPENRPC_ROUTE),
            ("connect", CONNECT_ROUTE),
        ] {
            assert_eq!(projections[name]["const"], path);
            assert!(
                typespec.contains(&format!("{name}: \"{path}\";")),
                "TypeSpec authority is missing {name}={path}"
            );
        }

        let aliases = schema["properties"]["aliases"]["items"]["enum"]
            .as_array()
            .expect("alias enum")
            .iter()
            .map(|value| value.as_str().expect("alias string"))
            .collect::<Vec<_>>();
        assert_eq!(aliases, DOCS_ALIAS_ROUTES);
    }
}
