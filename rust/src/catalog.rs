//! Served catalog: map + projections + cluster inventory.

use serde::Serialize;
use serde_json::{json, Value};

use crate::map::{RouteEntry, RouteMap};
use crate::paths::STANDARD_DOCS_ROUTES;
use crate::project::{connect, hyper_schema, openapi, openrpc};
use crate::schema::{validate_catalog, SchemaError};
use crate::{GENERATED_BY, SCHEMA_VERSION};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRoute {
    pub path: String,
    pub methods: Vec<String>,
    pub keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub map: RouteMap,
    pub openapi: Value,
    pub openrpc: Value,
    pub connect: Value,
    pub hyper_schema: Value,
    pub routes: Vec<InventoryRoute>,
    pub language: Option<String>,
}

impl Catalog {
    pub fn from_map(map: RouteMap) -> Result<Self, SchemaError> {
        Self::from_map_with_language(map, Some("rust"))
    }

    pub fn from_map_with_language(
        map: RouteMap,
        language: Option<&str>,
    ) -> Result<Self, SchemaError> {
        let openapi = openapi(&map)?;
        let openrpc = openrpc(&map)?;
        let connect = connect(&map)?;
        let hyper_schema = hyper_schema(&map)?;
        let routes = inventory(&map);
        let catalog = Self {
            map,
            openapi,
            openrpc,
            connect,
            hyper_schema,
            routes,
            language: language.map(str::to_owned),
        };
        validate_catalog(&catalog.to_value()?)?;
        Ok(catalog)
    }

    pub fn to_value(&self) -> Result<Value, SchemaError> {
        let mut map_obj = serde_json::Map::new();
        for (key, entry) in &self.map.map {
            map_obj.insert(key.clone(), entry_json(entry));
        }
        let mut doc = json!({
            "ok": true,
            "generatedBy": GENERATED_BY,
            "schema_version": SCHEMA_VERSION,
            "service": self.map.service,
            "standards": [
                "openapi-3.1",
                "json-schema-2020-12",
                "connect-protocol-json-unary",
                "openrpc-1.3",
                "json-hyper-schema-links",
                "ores-api-docs-catalog",
                "rfc6570-uri-templates",
                "ores-rpc-call-1",
                "ores-rpc-receipt-1"
            ],
            "standardDocsRoutes": STANDARD_DOCS_ROUTES,
            "map": map_obj,
            "openapi": self.openapi,
            "openrpc": self.openrpc,
            "connect": self.connect,
            "hyperSchema": self.hyper_schema,
            "routes": self.routes,
            "routeCount": self.map.map.len(),
        });
        if let Some(title) = &self.map.title {
            doc["title"] = json!(title);
        }
        if let Some(version) = &self.map.version {
            doc["version"] = json!(version);
        }
        if let Some(language) = &self.language {
            doc["language"] = json!(language);
        }
        Ok(doc)
    }

    pub fn to_pretty_json(&self) -> Result<String, SchemaError> {
        serde_json::to_string_pretty(&self.to_value()?).map_err(|e| SchemaError::Instance {
            name: "catalog",
            detail: e.to_string(),
        })
    }
}

fn entry_json(entry: &RouteEntry) -> Value {
    let mut obj = json!({
        "path": entry.path,
        "methods": entry.methods,
    });
    if let Some(summary) = &entry.summary {
        obj["summary"] = json!(summary);
    }
    if let Some(binding) = &entry.binding {
        obj["binding"] = serde_json::to_value(binding).unwrap_or(json!({}));
    }
    if let Some(path_params) = &entry.path_params {
        obj["path_params"] = path_params.clone();
    }
    if let Some(query) = &entry.query_schema {
        obj["query_schema"] = query.clone();
    }
    if let Some(req) = &entry.request_schema {
        obj["request_schema"] = req.clone();
    }
    if let Some(res) = &entry.response_schema {
        obj["response_schema"] = res.clone();
    }
    if let Some(err) = &entry.error_schema {
        obj["error_schema"] = err.clone();
    }
    if let Some(alias) = &entry.alias_of {
        obj["alias_of"] = json!(alias);
    }
    if !entry.transports.is_empty() {
        obj["transports"] = json!(entry.transports);
    }
    if let Some(framing) = &entry.tcp_framing {
        obj["tcp_framing"] = json!(framing);
    }
    obj
}

fn inventory(map: &RouteMap) -> Vec<InventoryRoute> {
    let mut by_path: Vec<InventoryRoute> = Vec::new();
    for (key, entry) in &map.map {
        if let Some(existing) = by_path.iter_mut().find(|r| r.path == entry.path) {
            for m in &entry.methods {
                if !existing.methods.contains(m) {
                    existing.methods.push(m.clone());
                }
            }
            if !existing.keys.contains(key) {
                existing.keys.push(key.clone());
            }
            if let Some(file) = entry.binding.as_ref().and_then(|b| b.file.clone()) {
                if !existing.source_files.contains(&file) {
                    existing.source_files.push(file);
                }
            }
            continue;
        }
        let route_type = if entry
            .methods
            .iter()
            .any(|m| m == "POST" && key.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        {
            Some("connect-json-unary".into())
        } else {
            Some("http".into())
        };
        by_path.push(InventoryRoute {
            path: entry.path.clone(),
            methods: entry.methods.clone(),
            keys: vec![key.clone()],
            route_type,
            purpose: entry.summary.clone(),
            source_files: entry
                .binding
                .as_ref()
                .and_then(|b| b.file.clone())
                .into_iter()
                .collect(),
        });
    }
    by_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_catalog_validates() {
        let map = RouteMap::from_json_str(include_str!("../../examples/pmap-api.route-map.json"))
            .unwrap();
        let catalog = Catalog::from_map(map).unwrap();
        let v = catalog.to_value().unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["generatedBy"], "ores-api-docs");
        assert!(v["standards"].as_array().unwrap().len() >= 3);
        assert!(v["map"]["CheckFieldSanity"]["binding"]["param_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "CheckFieldSanityRequest"));
    }
}
