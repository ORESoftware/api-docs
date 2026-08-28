//! Project a route map into OpenAPI 3.1, OpenRPC, Connect, and Hyper-Schema.

use serde_json::{json, Map, Value};

use crate::infer::{connect_service_method, is_connect_method_key};
use crate::map::RouteMap;
use crate::schema::{
    validate_connect, validate_hyper_schema, validate_openapi, validate_openrpc, SchemaError,
};

pub fn openapi(map: &RouteMap) -> Result<Value, SchemaError> {
    let mut paths = Map::new();
    for (key, entry) in &map.map {
        let item = paths
            .entry(entry.path.clone())
            .or_insert_with(|| json!({}));
        let obj = item.as_object_mut().expect("path item object");
        for method in &entry.methods {
            let mut op = json!({
                "operationId": key,
                "responses": {
                    "200": { "description": "ok" }
                }
            });
            if let Some(summary) = &entry.summary {
                op["summary"] = json!(summary);
            }
            let mut parameters = Vec::new();
            if let Some(path_schema) = &entry.path_params {
                if let Some(props) = path_schema.get("properties").and_then(Value::as_object) {
                    let required = path_schema
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    for (name, schema) in props {
                        parameters.push(json!({
                            "name": name,
                            "in": "path",
                            "required": required.contains(&name.as_str()) || required.is_empty(),
                            "schema": schema,
                        }));
                    }
                }
            } else if let Ok(vars) = crate::template::path_template_vars(&entry.path) {
                for name in vars {
                    parameters.push(json!({
                        "name": name,
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    }));
                }
            }
            if let Some(query) = &entry.query_schema {
                if let Some(props) = query.get("properties").and_then(Value::as_object) {
                    let required = query
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    for (name, schema) in props {
                        parameters.push(json!({
                            "name": name,
                            "in": "query",
                            "required": required.contains(&name.as_str()),
                            "schema": schema,
                        }));
                    }
                }
            }
            if !parameters.is_empty() {
                op["parameters"] = json!(parameters);
            }
            if let Some(req) = &entry.request_schema {
                op["requestBody"] = json!({
                    "required": true,
                    "content": { "application/json": { "schema": req } }
                });
            }
            if let Some(res) = &entry.response_schema {
                op["responses"]["200"]["content"] = json!({
                    "application/json": { "schema": res }
                });
            }
            obj.insert(method.to_ascii_lowercase(), op);
        }
    }
    let doc = json!({
        "openapi": "3.1.0",
        "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
        "info": {
            "title": map.title.clone().unwrap_or_else(|| map.service.clone()),
            "version": map.version.clone().unwrap_or_else(|| "0.1.0".into()),
            "description": map.description.clone().unwrap_or_default(),
        },
        "paths": paths,
    });
    validate_openapi(&doc)?;
    Ok(doc)
}

pub fn openrpc(map: &RouteMap) -> Result<Value, SchemaError> {
    let methods: Vec<Value> = map
        .map
        .iter()
        .map(|(key, entry)| {
            let mut m = json!({
                "name": key,
                "paramStructure": "by-name",
                "x-http-path": entry.path,
                "x-http-methods": entry.methods,
            });
            if let Some(summary) = &entry.summary {
                m["summary"] = json!(summary);
            }
            let mut params = Vec::new();
            if let Some(path_schema) = &entry.path_params {
                if let Some(props) = path_schema.get("properties").and_then(Value::as_object) {
                    for (name, schema) in props {
                        params.push(json!({
                            "name": name,
                            "required": true,
                            "schema": schema
                        }));
                    }
                }
            }
            if let Some(query) = &entry.query_schema {
                if let Some(props) = query.get("properties").and_then(Value::as_object) {
                    let required = query
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    for (name, schema) in props {
                        params.push(json!({
                            "name": name,
                            "required": required.contains(&name.as_str()),
                            "schema": schema
                        }));
                    }
                }
            }
            if let Some(req) = &entry.request_schema {
                params.push(json!({
                    "name": "body",
                    "required": true,
                    "schema": req
                }));
            } else if params.is_empty() {
                if let Some(binding) = &entry.binding {
                    if !binding.param_types.is_empty() {
                        for t in &binding.param_types {
                            params.push(json!({
                                "name": t,
                                "required": true,
                                "schema": { "type": "object", "title": t }
                            }));
                        }
                    }
                }
            }
            if !params.is_empty() {
                m["params"] = json!(params);
            }
            if let Some(res) = &entry.response_schema {
                m["result"] = json!({ "name": "result", "schema": res });
            } else if let Some(rt) = entry.binding.as_ref().and_then(|b| b.return_type.as_ref()) {
                m["result"] = json!({
                    "name": "result",
                    "schema": { "type": "object", "title": rt }
                });
            }
            m
        })
        .collect();
    let doc = json!({
        "openrpc": "1.3.2",
        "info": {
            "title": map.title.clone().unwrap_or_else(|| map.service.clone()),
            "version": map.version.clone().unwrap_or_else(|| "0.1.0".into()),
        },
        "methods": methods,
    });
    validate_openrpc(&doc)?;
    Ok(doc)
}

pub fn connect(map: &RouteMap) -> Result<Value, SchemaError> {
    let mut services = Map::new();
    for (key, entry) in &map.map {
        if !is_connect_method_key(key) {
            continue;
        }
        let (service, method) = connect_service_method(&entry.path)
            .unwrap_or_else(|| ("default".into(), key.clone()));
        let svc = services
            .entry(service)
            .or_insert_with(|| json!({ "methods": {} }));
        let mut method_obj = json!({
            "path": entry.path,
            "httpMethod": "POST",
            "idempotency": "unknown",
        });
        if let Some(req) = &entry.request_schema {
            method_obj["request"] = req.clone();
        }
        if let Some(res) = &entry.response_schema {
            method_obj["response"] = res.clone();
        }
        svc["methods"][method] = method_obj;
    }
    let doc = json!({
        "protocol": "connect",
        "codec": "json",
        "contentType": "application/json",
        "streaming": false,
        "services": services,
    });
    validate_connect(&doc)?;
    Ok(doc)
}

pub fn hyper_schema(map: &RouteMap) -> Result<Value, SchemaError> {
    let mut links = Vec::new();
    for (key, entry) in &map.map {
        let method = entry.methods.first().cloned().unwrap_or_else(|| "GET".into());
        let mut link = json!({
            "rel": key,
            "href": entry.path,
            "method": method,
        });
        if let Some(req) = &entry.request_schema {
            link["submissionSchema"] = req.clone();
        }
        if let Some(res) = &entry.response_schema {
            link["targetSchema"] = res.clone();
        }
        links.push(link);
    }
    let doc = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "links": links,
    });
    validate_hyper_schema(&doc)?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::RouteMap;

    #[test]
    fn example_projects_validate() {
        let map = RouteMap::from_json_str(include_str!("../../examples/pmap-api.route-map.json"))
            .unwrap();
        let oa = openapi(&map).unwrap();
        assert_eq!(oa["openapi"], "3.1.0");
        assert!(oa["paths"]["/pmap.v1.Interview/CheckFieldSanity"]["post"]["operationId"] == "CheckFieldSanity");
        let params = oa["paths"]["/v1/matters/{id}"]["get"]["parameters"].as_array().unwrap();
        assert!(params.iter().any(|p| p["in"] == "path" && p["name"] == "id"));
        assert!(params.iter().any(|p| p["in"] == "query" && p["name"] == "include"));
        let rpc = openrpc(&map).unwrap();
        assert_eq!(rpc["openrpc"], "1.3.2");
        let c = connect(&map).unwrap();
        assert_eq!(c["codec"], "json");
        assert_eq!(
            c["services"]["pmap.v1.Interview"]["methods"]["CheckFieldSanity"]["path"],
            "/pmap.v1.Interview/CheckFieldSanity"
        );
        hyper_schema(&map).unwrap();
    }
}
