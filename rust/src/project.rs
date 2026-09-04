//! Project one route map into multiple adjacent standards.

use serde_json::{json, Map, Value};

use crate::infer::is_connect_method_key;
use crate::map::{RouteEntry, RouteMap};
use crate::schema::{
    validate_connect, validate_hyper_schema, validate_openapi, validate_openrpc, SchemaError,
};
use crate::template::path_template_vars;

/// Stable digest of the normalized semantic RPC contract.
///
/// The same canonical object is emitted by `scripts/rpc-contract-bundle.py`.
/// Formatting and JSON object-key order do not affect this identifier.
#[must_use]
pub fn contract_sha256(map: &RouteMap) -> String {
    let semantic = contract_semantic_value(map);
    let bytes = serde_json::to_vec(&semantic).expect("semantic contract is JSON serializable");
    sha256_hex(&bytes)
}

fn sha256_hex(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = sha256(input);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

// Dependency-free SHA-256 for a public contract identifier. This is not used
// for passwords, signatures, MACs, encryption, or secret derivation. Keeping
// it here avoids adding a lockfile-changing crypto dependency merely to bind
// generated docs and language surfaces to the same normalized RPC contract.
fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
        0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let padded_len = (input.len() + 9).div_ceil(64) * 64;
    let mut padded = Vec::with_capacity(padded_len);
    padded.extend_from_slice(input);
    padded.push(0x80);
    padded.resize(padded_len - 8, 0);
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut schedule = [0_u32; 64];
        for (index, word) in schedule[..16].iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes(
                block[offset..offset + 4]
                    .try_into()
                    .expect("SHA-256 word is four bytes"),
            );
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let big1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(big1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let big0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = big0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut out = [0_u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn contract_semantic_value(map: &RouteMap) -> Value {
    let operations = map
        .map
        .iter()
        .map(|(key, entry)| {
            let mut operation = Map::new();
            operation.insert("key".into(), json!(key));
            operation.insert("path".into(), json!(entry.path));
            operation.insert("methods".into(), json!(entry.methods));
            operation.insert("transports".into(), json!(entry.transports));
            operation.insert(
                "delivery".into(),
                json!(entry.delivery.as_deref().unwrap_or("direct")),
            );
            if let Some(value) = &entry.tcp_framing {
                operation.insert("tcpFraming".into(), json!(value));
            }
            if let Some(value) = &entry.summary {
                operation.insert("summary".into(), json!(value));
            }
            if let Some(value) = &entry.binding {
                operation.insert(
                    "binding".into(),
                    serde_json::to_value(value).expect("binding is JSON serializable"),
                );
            }
            if let Some(value) = &entry.path_params {
                operation.insert("pathParams".into(), value.clone());
            }
            if let Some(value) = &entry.query_schema {
                operation.insert("querySchema".into(), value.clone());
            }
            if let Some(value) = &entry.header_schema {
                operation.insert("headerSchema".into(), value.clone());
            }
            if let Some(value) = &entry.request_schema {
                operation.insert("requestSchema".into(), value.clone());
            }
            if let Some(value) = &entry.response_schema {
                operation.insert("responseSchema".into(), value.clone());
            }
            if let Some(value) = &entry.error_schema {
                operation.insert("errorSchema".into(), value.clone());
            }
            if let Some(value) = &entry.alias_of {
                operation.insert("aliasOf".into(), json!(value));
            }
            if let Some(value) = &entry.opto_sync {
                operation.insert(
                    "optoSync".into(),
                    serde_json::to_value(value).expect("opto-sync metadata is JSON serializable"),
                );
            }
            Value::Object(operation)
        })
        .collect::<Vec<_>>();
    json!({
        "formatVersion": 1,
        "routeMapSchemaVersion": map.schema_version,
        "service": map.service,
        "title": map.title.as_deref().unwrap_or(&map.service),
        "version": map.version.as_deref().unwrap_or("0.1.0"),
        "description": map.description.as_deref().unwrap_or(""),
        "operations": operations,
    })
}

fn validate_projection_contract(map: &RouteMap) -> Result<(), SchemaError> {
    for (key, entry) in &map.map {
        for (label, schema) in [
            ("path_params", &entry.path_params),
            ("query_schema", &entry.query_schema),
            ("header_schema", &entry.header_schema),
            ("request_schema", &entry.request_schema),
            ("response_schema", &entry.response_schema),
            ("error_schema", &entry.error_schema),
        ] {
            if let Some(schema) = schema {
                jsonschema::validator_for(schema).map_err(|error| SchemaError::Instance {
                    name: "rpc-contract",
                    detail: format!("{key}.{label} is not valid JSON Schema 2020-12: {error}"),
                })?;
            }
        }
        if let Some(schema) = &entry.path_params {
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .ok_or_else(|| SchemaError::Instance {
                    name: "rpc-contract",
                    detail: format!("{key}.path_params must declare properties"),
                })?;
            let required = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<std::collections::BTreeSet<_>>()
                })
                .unwrap_or_default();
            let declared = properties
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            if required != declared {
                return Err(SchemaError::Instance {
                    name: "rpc-contract",
                    detail: format!(
                        "{key}.path_params must require every path variable; required={required:?} properties={declared:?}"
                    ),
                });
            }
            let template = path_template_vars(&entry.path).map_err(|error| SchemaError::Instance {
                name: "rpc-contract",
                detail: format!("{key}.path is invalid: {error}"),
            })?;
            let template = template
                .iter()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            if template != declared {
                return Err(SchemaError::Instance {
                    name: "rpc-contract",
                    detail: format!(
                        "{key}.path_params properties={declared:?} do not match template={template:?}"
                    ),
                });
            }
        }
        if is_connect_method_key(key) {
            let parts = entry
                .path
                .trim_start_matches('/')
                .split('/')
                .collect::<Vec<_>>();
            if parts.len() != 2 || parts[1] != key {
                return Err(SchemaError::Instance {
                    name: "rpc-contract",
                    detail: format!(
                        "{key}: Connect JSON unary path must be /service/{key}, got {}",
                        entry.path
                    ),
                });
            }
        }
    }

    for start in map.map.keys() {
        let mut seen = std::collections::BTreeSet::new();
        let mut current = start.as_str();
        while let Some(next) = map
            .map
            .get(current)
            .and_then(|entry| entry.alias_of.as_deref())
        {
            if !seen.insert(current) {
                return Err(SchemaError::Instance {
                    name: "rpc-contract",
                    detail: format!("{start}: alias cycle"),
                });
            }
            current = next;
        }
    }
    Ok(())
}

fn rpc_extension(key: &str, entry: &RouteEntry, digest: &str) -> Value {
    let mut extension = json!({
        "contractSha256": digest,
        "key": key,
        "transports": entry.transports,
        "delivery": entry.delivery.as_deref().unwrap_or("direct"),
    });
    if let Some(value) = &entry.tcp_framing {
        extension["tcpFraming"] = json!(value);
    }
    if let Some(value) = &entry.alias_of {
        extension["aliasOf"] = json!(value);
    }
    if let Some(value) = &entry.opto_sync {
        extension["optoSync"] =
            serde_json::to_value(value).expect("opto-sync metadata is JSON serializable");
    }
    extension
}

/// OpenAPI 3.1: paths → methods, operationId is the map key.
pub fn openapi(map: &RouteMap) -> Result<Value, SchemaError> {
    validate_projection_contract(map)?;
    let digest = contract_sha256(map);
    let mut paths = Map::new();
    for (key, entry) in &map.map {
        let item = paths
            .entry(entry.path.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let item_obj = item
            .as_object_mut()
            .expect("path item was just constructed as object");
        for method in &entry.methods {
            let mut op = json!({
                "operationId": key,
                "responses": {
                    "200": { "description": "ok" }
                },
                "x-ores-rpc": rpc_extension(key, entry, &digest),
            });
            if let Some(summary) = &entry.summary {
                op["summary"] = json!(summary);
            }
            let params = parameter_list(entry);
            if !params.is_empty() {
                op["parameters"] = Value::Array(params);
            }
            if let Some(body) = &entry.request_schema {
                op["requestBody"] = json!({
                    "required": true,
                    "content": {
                        "application/json": { "schema": body }
                    }
                });
            }
            if let Some(response) = &entry.response_schema {
                op["responses"]["200"]["content"] = json!({
                    "application/json": { "schema": response }
                });
            }
            item_obj.insert(method.to_ascii_lowercase(), op);
        }
    }
    let doc = json!({
        "openapi": "3.1.0",
        "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
        "info": {
            "title": map.title.as_deref().unwrap_or(&map.service),
            "version": map.version.as_deref().unwrap_or("0.1.0"),
            "description": map.description.as_deref().unwrap_or(""),
        },
        "paths": paths,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": map.schema_version,
    });
    validate_openapi(&doc)?;
    Ok(doc)
}

/// Connect protocol, JSON codec, unary only. PascalCase keys are methods.
pub fn connect(map: &RouteMap) -> Result<Value, SchemaError> {
    validate_projection_contract(map)?;
    let digest = contract_sha256(map);
    let mut services: Map<String, Value> = Map::new();
    for (key, entry) in &map.map {
        if !is_connect_method_key(key) {
            continue;
        }
        let parts: Vec<&str> = entry.path.trim_start_matches('/').split('/').collect();
        if parts.len() != 2 {
            continue;
        }
        let service = parts[0];
        let method_name = parts[1];
        let svc = services.entry(service.to_string()).or_insert_with(|| {
            json!({
                "methods": {}
            })
        });
        let mut method = json!({
            "path": entry.path,
            "httpMethod": "POST",
            "idempotency": "unknown",
            "x-ores-rpc": rpc_extension(key, entry, &digest),
        });
        if let Some(request) = &entry.request_schema {
            method["request"] = request.clone();
        }
        if let Some(headers) = &entry.header_schema {
            method["requestHeaders"] = headers.clone();
        }
        if let Some(response) = &entry.response_schema {
            method["response"] = response.clone();
        }
        svc["methods"][method_name] = method;
    }
    let doc = json!({
        "protocol": "connect",
        "codec": "json",
        "contentType": "application/json",
        "streaming": false,
        "services": services,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": map.schema_version,
    });
    validate_connect(&doc)?;
    Ok(doc)
}

/// OpenRPC 1.3 / JSON-RPC-adjacent method discovery.
pub fn openrpc(map: &RouteMap) -> Result<Value, SchemaError> {
    validate_projection_contract(map)?;
    let digest = contract_sha256(map);
    let mut methods = Vec::new();
    for (key, entry) in &map.map {
        let mut method = json!({
            "name": key,
            "paramStructure": "by-name",
            "x-http-path": entry.path,
            "x-http-methods": entry.methods,
            "x-ores-rpc": rpc_extension(key, entry, &digest),
        });
        if let Some(summary) = &entry.summary {
            method["summary"] = json!(summary);
        }
        let mut params = rpc_params(entry);
        if let Some(body) = &entry.request_schema {
            params.push(json!({
                "name": "body",
                "required": true,
                "schema": body,
                "x-ores-location": "body"
            }));
        }
        if !params.is_empty() {
            method["params"] = Value::Array(params);
        }
        if let Some(result) = &entry.response_schema {
            method["result"] = json!({
                "name": "result",
                "schema": result
            });
        }
        methods.push(method);
    }
    let doc = json!({
        "openrpc": "1.3.2",
        "info": {
            "title": map.title.as_deref().unwrap_or(&map.service),
            "version": map.version.as_deref().unwrap_or("0.1.0"),
        },
        "methods": methods,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": map.schema_version,
    });
    validate_openrpc(&doc)?;
    Ok(doc)
}

/// JSON Hyper-Schema-ish typed link objects.
pub fn hyper_schema(map: &RouteMap) -> Result<Value, SchemaError> {
    validate_projection_contract(map)?;
    let digest = contract_sha256(map);
    let mut links = Vec::new();
    for (key, entry) in &map.map {
        for method in &entry.methods {
            let mut link = json!({
                "rel": key,
                "href": entry.path,
                "method": method,
                "x-ores-rpc": rpc_extension(key, entry, &digest),
            });
            if let Some(request) = &entry.request_schema {
                link["submissionSchema"] = request.clone();
            }
            if let Some(response) = &entry.response_schema {
                link["targetSchema"] = response.clone();
            }
            if let Some(path) = &entry.path_params {
                link["hrefSchema"] = path.clone();
            }
            if let Some(query) = &entry.query_schema {
                link["querySchema"] = query.clone();
            }
            if let Some(headers) = &entry.header_schema {
                link["headerSchema"] = headers.clone();
            }
            links.push(link);
        }
    }
    let doc = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "links": links,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": map.schema_version,
    });
    validate_hyper_schema(&doc)?;
    Ok(doc)
}

fn parameter_list(entry: &RouteEntry) -> Vec<Value> {
    let mut params = Vec::new();
    append_schema_params(&mut params, entry.path_params.as_ref(), "path");
    append_schema_params(&mut params, entry.query_schema.as_ref(), "query");
    append_schema_params(&mut params, entry.header_schema.as_ref(), "header");
    params
}

fn append_schema_params(out: &mut Vec<Value>, schema: Option<&Value>, location: &str) {
    let Some(schema) = schema else { return };
    let Some(props) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    for (name, sub) in props {
        out.push(json!({
            "name": name,
            "in": location,
            "required": location == "path" || required.contains(name.as_str()),
            "schema": sub,
        }));
    }
}

fn rpc_params(entry: &RouteEntry) -> Vec<Value> {
    let mut params = Vec::new();
    for (schema, location) in [
        (&entry.path_params, "path"),
        (&entry.query_schema, "query"),
        (&entry.header_schema, "header"),
    ] {
        let Some(schema) = schema else { continue };
        let Some(props) = schema.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        for (name, sub) in props {
            params.push(json!({
                "name": name,
                "required": location == "path" || required.contains(name.as_str()),
                "schema": sub,
                "x-ores-location": location,
            }));
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> RouteMap {
        RouteMap::from_json_str(include_str!("../../examples/pmap-api.route-map.json")).unwrap()
    }

    #[test]
    fn sha256_implementation_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn projections_validate() {
        let map = example();
        assert_eq!(openapi(&map).unwrap()["openapi"], "3.1.0");
        assert_eq!(openrpc(&map).unwrap()["openrpc"], "1.3.2");
        assert_eq!(connect(&map).unwrap()["protocol"], "connect");
        assert!(hyper_schema(&map).unwrap()["links"].as_array().unwrap().len() > 5);
    }

    #[test]
    fn digest_is_shared_by_every_docs_projection() {
        let map = RouteMap::from_json_str(include_str!(
            "../../examples/rpc-transports.route-map.json"
        ))
        .unwrap();
        let digest = contract_sha256(&map);
        assert_eq!(
            digest,
            "883a04ee34e51e74e89f3f688beac79516962fa60e7c25fd85e2fe66b2ef83af"
        );
        for doc in [
            openapi(&map).unwrap(),
            openrpc(&map).unwrap(),
            connect(&map).unwrap(),
            hyper_schema(&map).unwrap(),
        ] {
            assert_eq!(doc["x-ores-rpc-contract-sha256"], digest);
            assert_eq!(doc["x-ores-rpc-schema-version"], "1.0.0");
        }
    }

    #[test]
    fn projections_reject_invalid_nested_schema_and_connect_drift() {
        let invalid_schema = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"x",
              "map":{"get_x":{"path":"/x","methods":["GET"],"response_schema":{"type":7}}}
            }"#,
        )
        .unwrap();
        assert!(openapi(&invalid_schema).is_err());

        let bad_connect = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"x",
              "map":{"CreateThing":{"path":"/x.v1.Svc/Wrong","methods":["POST"]}}
            }"#,
        )
        .unwrap();
        assert!(connect(&bad_connect).is_err());
    }

    #[test]
    fn hyper_schema_keeps_every_declared_method() {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"x",
              "map":{"thing":{"path":"/thing","methods":["GET","HEAD"]}}
            }"#,
        )
        .unwrap();
        let links = hyper_schema(&map).unwrap()["links"].as_array().unwrap().clone();
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|link| link["method"] == "GET"));
        assert!(links.iter().any(|link| link["method"] == "HEAD"));
    }

    #[test]
    fn transport_and_delivery_metadata_reach_docs() {
        let map = RouteMap::from_json_str(include_str!(
            "../../examples/rpc-transports.route-map.json"
        ))
        .unwrap();
        let openapi = openapi(&map).unwrap();
        let extension = &openapi["paths"]["/v1/items/{id}"]["get"]["x-ores-rpc"];
        assert_eq!(
            extension["transports"],
            json!(["http", "tcp", "websocket"])
        );
        assert_eq!(extension["tcpFraming"], "ndjson");
        assert_eq!(extension["delivery"], "direct");
    }
}
