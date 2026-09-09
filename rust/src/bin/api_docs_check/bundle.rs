use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ores_api_docs::{contract_sha256, project, RouteMap};
use serde_json::{json, Value};

use super::common::{read_json, read_text, sha256_json, write_json, write_text, CheckResult, TempDir};
use super::routes::{
    compact_json, default_maps, gen_dart, gen_gleam, gen_rust, gen_typescript, go_ident,
    insert_before, mechanism_manifest, pretty_json, source_ordered_map, OrderedRouteDoc,
};

fn semantic_contract(map: &RouteMap) -> Value {
    let operations = map
        .map
        .iter()
        .map(|(key, entry)| {
            let mut operation = serde_json::Map::new();
            operation.insert("key".to_owned(), json!(key));
            operation.insert("path".to_owned(), json!(entry.path));
            operation.insert("methods".to_owned(), json!(entry.methods));
            operation.insert("transports".to_owned(), json!(entry.transports));
            operation.insert(
                "delivery".to_owned(),
                json!(entry.delivery.as_deref().unwrap_or("direct")),
            );
            if let Some(value) = &entry.tcp_framing {
                operation.insert("tcpFraming".to_owned(), json!(value));
            }
            if let Some(value) = &entry.summary {
                operation.insert("summary".to_owned(), json!(value));
            }
            if let Some(value) = &entry.binding {
                operation.insert(
                    "binding".to_owned(),
                    serde_json::to_value(value).expect("binding is JSON serializable"),
                );
            }
            if let Some(value) = &entry.path_params {
                operation.insert("pathParams".to_owned(), value.clone());
            }
            if let Some(value) = &entry.query_schema {
                operation.insert("querySchema".to_owned(), value.clone());
            }
            if let Some(value) = &entry.header_schema {
                operation.insert("headerSchema".to_owned(), value.clone());
            }
            if let Some(value) = &entry.request_schema {
                operation.insert("requestSchema".to_owned(), value.clone());
            }
            if let Some(value) = &entry.response_schema {
                operation.insert("responseSchema".to_owned(), value.clone());
            }
            if let Some(value) = &entry.error_schema {
                operation.insert("errorSchema".to_owned(), value.clone());
            }
            if let Some(value) = &entry.alias_of {
                operation.insert("aliasOf".to_owned(), json!(value));
            }
            if let Some(value) = &entry.opto_sync {
                operation.insert(
                    "optoSync".to_owned(),
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

fn contract_document(map_path: &Path, map: &RouteMap) -> CheckResult<Value> {
    let semantic = semantic_contract(map);
    let digest = sha256_json(&semantic)?;
    let library_digest = contract_sha256(map);
    if digest != library_digest {
        return Err(format!(
            "{}: checker digest {digest} != ores-api-docs digest {library_digest}",
            map_path.display()
        ));
    }
    let mut object = semantic
        .as_object()
        .expect("semantic contract is an object")
        .clone();
    object.insert(
        "source".to_owned(),
        Value::String(
            map_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("invalid route-map path: {}", map_path.display()))?
                .to_owned(),
        ),
    );
    object.insert("contractSha256".to_owned(), Value::String(digest));
    Ok(Value::Object(object))
}

fn language_manifest_json(map: &RouteMap) -> CheckResult<String> {
    compact_json(&mechanism_manifest(map))
}

fn gen_typescript_bound(
    contract: &Value,
    ordered: &OrderedRouteDoc,
    map: &RouteMap,
) -> CheckResult<String> {
    let base = gen_typescript(&ordered.service, &ordered.map, map)?;
    let manifest = pretty_json(&mechanism_manifest(map))?;
    let declaration = format!(
        "export const RPC_CONTRACT_SHA256 = \"{}\" as const;\nexport const RPC_MECHANISMS = {manifest} as const;\n\n",
        contract["contractSha256"].as_str().unwrap_or_default()
    );
    insert_before(&base, "export const SERVICE", &declaration, "TypeScript")
}

fn gen_rust_bound(contract: &Value, ordered: &OrderedRouteDoc, map: &RouteMap) -> CheckResult<String> {
    let base = gen_rust(&ordered.service, &ordered.map, map)?;
    let manifest = language_manifest_json(map)?;
    let declaration = format!(
        "/// Digest of the normalized RPC contract and docs bundle.\npub const RPC_CONTRACT_SHA256: &str = \"{}\";\n/// Transport, framing, delivery, alias, and queue metadata for every route.\npub const RPC_MECHANISMS_JSON: &str = r###\"{manifest}\"###;\n\n",
        contract["contractSha256"].as_str().unwrap_or_default()
    );
    insert_before(&base, "pub const SERVICE", &declaration, "Rust")
}

fn gen_dart_bound(contract: &Value, ordered: &OrderedRouteDoc, map: &RouteMap) -> CheckResult<String> {
    let base = gen_dart(&ordered.service, &ordered.map, map)?;
    let manifest = language_manifest_json(map)?;
    let declaration = format!(
        "/// Digest of the normalized RPC contract and docs bundle.\nconst String rpcContractSha256 = '{}';\n/// Full route mechanism metadata, bound by [rpcContractSha256].\nconst String rpcMechanismsJson = r'''{manifest}''';\n\n",
        contract["contractSha256"].as_str().unwrap_or_default()
    );
    insert_before(&base, "const String kService", &declaration, "Dart")
}

fn json_string_literal(value: &str) -> CheckResult<String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn gen_gleam_bound(contract: &Value, ordered: &OrderedRouteDoc, map: &RouteMap) -> CheckResult<String> {
    let base = gen_gleam(&ordered.service, &ordered.map, map)?;
    let manifest = language_manifest_json(map)?;
    let manifest_literal = json_string_literal(&manifest)?;
    let declaration = format!(
        "/// Digest of the normalized RPC contract and docs bundle.\npub const rpc_contract_sha256: String = \"{}\"\n/// Full route mechanism metadata, bound by rpc_contract_sha256.\npub const rpc_mechanisms_json: String = {manifest_literal}\n\n",
        contract["contractSha256"].as_str().unwrap_or_default()
    );
    insert_before(&base, "pub const service", &declaration, "Gleam")
}

fn gen_go(contract: &Value) -> CheckResult<String> {
    let operations = contract["operations"]
        .as_array()
        .ok_or_else(|| "contract operations must be an array".to_owned())?;
    let manifest = operations
        .iter()
        .filter_map(|operation| {
            let key = operation.get("key")?.as_str()?;
            let mut item = serde_json::Map::new();
            for field in [
                "key",
                "path",
                "methods",
                "transports",
                "tcpFraming",
                "delivery",
                "aliasOf",
                "optoSync",
            ] {
                if let Some(value) = operation.get(field) {
                    item.insert(field.to_owned(), value.clone());
                }
            }
            Some((key.to_owned(), Value::Object(item)))
        })
        .collect::<serde_json::Map<_, _>>();
    let mechanisms = compact_json(&Value::Object(manifest))?;
    let mechanisms_literal = json_string_literal(&mechanisms)?;
    let mut identifiers = BTreeMap::new();
    let mut route_names = Vec::new();
    for operation in operations {
        let key = operation["key"].as_str().unwrap_or_default();
        let identifier = go_ident(key)?;
        if let Some(previous) = identifiers.insert(identifier.clone(), key.to_owned()) {
            return Err(format!(
                "Go identifier collision: {previous:?} and {key:?} both become {identifier}"
            ));
        }
        route_names.push(format!("Route{identifier}"));
    }
    let mut lines = vec![
        "// Code generated by api-docs-check; DO NOT EDIT.".to_owned(),
        "package rpccontract".to_owned(),
        String::new(),
        format!(
            "const Service = {}",
            json_string_literal(contract["service"].as_str().unwrap_or_default())?
        ),
        format!(
            "const ContractSHA256 = {}",
            json_string_literal(contract["contractSha256"].as_str().unwrap_or_default())?
        ),
        String::new(),
        "// RPCMechanismsJSON is the exact machine-readable route mechanism contract.".to_owned(),
        format!("const RPCMechanismsJSON = {mechanisms_literal}"),
        String::new(),
        "type RouteKey string".to_owned(),
        String::new(),
        "const (".to_owned(),
    ];
    for (operation, route_name) in operations.iter().zip(&route_names) {
        lines.push(format!(
            "\t{route_name} RouteKey = {}",
            json_string_literal(operation["key"].as_str().unwrap_or_default())?
        ));
    }
    lines.extend([
        ")".to_owned(),
        String::new(),
        "type Route struct {".to_owned(),
        "\tKey RouteKey".to_owned(),
        "\tPath string".to_owned(),
        "\tMethods []string".to_owned(),
        "\tTransports []string".to_owned(),
        "\tTCPFraming string".to_owned(),
        "\tDelivery string".to_owned(),
        "\tAliasOf string".to_owned(),
        "\tOptoSyncTable string".to_owned(),
        "\tOptoSyncOperation string".to_owned(),
        "}".to_owned(),
        String::new(),
        "var Routes = map[RouteKey]Route{".to_owned(),
    ]);
    for operation in operations {
        let key = operation["key"].as_str().unwrap_or_default();
        let route_name = format!("Route{}", go_ident(key)?);
        let methods = operation["methods"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(json_string_literal)
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        let transports = operation["transports"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(json_string_literal)
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        let opto = operation.get("optoSync").and_then(Value::as_object);
        let string_field = |name: &str| {
            operation
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        lines.extend([
            format!("\t{route_name}: {{"),
            format!("\t\tKey: {route_name},"),
            format!("\t\tPath: {},", json_string_literal(string_field("path"))?),
            format!("\t\tMethods: []string{{{methods}}},"),
            format!("\t\tTransports: []string{{{transports}}},"),
            format!(
                "\t\tTCPFraming: {},",
                json_string_literal(string_field("tcpFraming"))?
            ),
            format!("\t\tDelivery: {},", json_string_literal(string_field("delivery"))?),
            format!("\t\tAliasOf: {},", json_string_literal(string_field("aliasOf"))?),
            format!(
                "\t\tOptoSyncTable: {},",
                json_string_literal(
                    opto.and_then(|value| value.get("table"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                )?
            ),
            format!(
                "\t\tOptoSyncOperation: {},",
                json_string_literal(
                    opto.and_then(|value| value.get("operation"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                )?
            ),
            "\t},".to_owned(),
        ]);
    }
    lines.extend(["}".to_owned(), String::new()]);
    Ok(lines.join("\n"))
}

fn normalize_source(text: String) -> String {
    format!("{}\n", text.trim_end_matches('\n'))
}

fn map_stem(path: &Path) -> CheckResult<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid map path: {}", path.display()))?;
    Ok(name
        .strip_suffix(".route-map.json")
        .unwrap_or(name)
        .replace('-', "_"))
}

fn write_language(path: &Path, text: String) -> CheckResult<()> {
    write_text(path, &normalize_source(text))
}

pub fn generate_one(map_path: &Path, out_root: &Path) -> CheckResult<(PathBuf, RouteMap)> {
    let (ordered, map) = source_ordered_map(map_path)?;
    if ordered.schema_version != "1.0.0" {
        return Err(format!("{}: expected v1 route map", map_path.display()));
    }
    let contract = contract_document(map_path, &map)?;
    let target = out_root.join(map_stem(map_path)?);
    write_json(&target.join("contract.json"), &contract)?;
    let openapi = project::openapi(&map).map_err(|error| error.to_string())?;
    let openrpc = project::openrpc(&map).map_err(|error| error.to_string())?;
    let connect = project::connect(&map).map_err(|error| error.to_string())?;
    let hyper = project::hyper_schema(&map).map_err(|error| error.to_string())?;
    write_json(&target.join("docs/openapi.json"), &openapi)?;
    write_json(&target.join("docs/openrpc.json"), &openrpc)?;
    write_json(&target.join("docs/connect.json"), &connect)?;
    write_json(&target.join("docs/hyper-schema.json"), &hyper)?;
    write_language(
        &target.join("typescript/routes.ts"),
        gen_typescript_bound(&contract, &ordered, &map)?,
    )?;
    write_language(
        &target.join("rust/routes.rs"),
        gen_rust_bound(&contract, &ordered, &map)?,
    )?;
    write_language(
        &target.join("dart/routes.dart"),
        gen_dart_bound(&contract, &ordered, &map)?,
    )?;
    write_language(
        &target.join("gleam/routes.gleam"),
        gen_gleam_bound(&contract, &ordered, &map)?,
    )?;
    write_language(&target.join("go/routes.go"), gen_go(&contract)?)?;
    write_text(
        &target.join("go/go.mod"),
        "module example.invalid/ores/rpccontract\n\ngo 1.23\n",
    )?;
    Ok((target, map))
}

pub fn verify_bundle(target: &Path, map: &RouteMap, map_path: &Path) -> CheckResult<()> {
    let contract = read_json(&target.join("contract.json"))?;
    let expected_contract = contract_document(map_path, map)?;
    if contract != expected_contract {
        return Err(format!("{}: normalized contract drift", target.display()));
    }
    let expected_docs = [
        ("openapi", project::openapi(map).map_err(|error| error.to_string())?),
        ("openrpc", project::openrpc(map).map_err(|error| error.to_string())?),
        ("connect", project::connect(map).map_err(|error| error.to_string())?),
        (
            "hyper-schema",
            project::hyper_schema(map).map_err(|error| error.to_string())?,
        ),
    ];
    for (name, expected) in expected_docs {
        let actual = read_json(&target.join(format!("docs/{name}.json")))?;
        if actual != expected {
            return Err(format!(
                "{}: {name} projection differs from normalized contract",
                target.display()
            ));
        }
    }
    let (ordered, _) = source_ordered_map(map_path)?;
    let expected_languages = [
        (
            "typescript/routes.ts",
            normalize_source(gen_typescript_bound(&contract, &ordered, map)?),
        ),
        ("rust/routes.rs", normalize_source(gen_rust_bound(&contract, &ordered, map)?)),
        ("dart/routes.dart", normalize_source(gen_dart_bound(&contract, &ordered, map)?)),
        (
            "gleam/routes.gleam",
            normalize_source(gen_gleam_bound(&contract, &ordered, map)?),
        ),
        ("go/routes.go", normalize_source(gen_go(&contract)?)),
    ];
    for (relative, expected) in expected_languages {
        let actual = read_text(&target.join(relative))?;
        if actual != expected {
            return Err(format!(
                "{}: {relative} RPC mechanisms differ from normalized contract",
                target.display()
            ));
        }
    }
    Ok(())
}

pub fn verify_ridl_emitters(root: &Path) -> CheckResult<()> {
    let expected = [
        "dart",
        "gleam",
        "go",
        "kotlin",
        "python",
        "rust",
        "swift",
        "typescript",
    ];
    let directory = root.join("ridl/emit");
    let mut actual = fs::read_dir(&directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("py"))
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_owned();
            (!matches!(stem.as_str(), "__init__" | "base" | "json_schema")).then_some(stem)
        })
        .collect::<Vec<_>>();
    actual.sort();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("RIDL emitter set {actual:?} != {expected:?}"))
    }
}

pub fn run_bundle(
    root: &Path,
    maps: &[PathBuf],
    out: Option<&Path>,
    check: bool,
) -> CheckResult<()> {
    let maps = if maps.is_empty() {
        default_maps(root)?
            .into_iter()
            .filter(|path| {
                read_json(path)
                    .ok()
                    .and_then(|document| {
                        document
                            .get("schema_version")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .as_deref()
                    == Some("1.0.0")
            })
            .collect::<Vec<_>>()
    } else {
        maps.iter()
            .map(|path| if path.is_absolute() { path.clone() } else { root.join(path) })
            .collect()
    };
    if maps.is_empty() {
        return Err("no v1 route maps".to_owned());
    }
    verify_ridl_emitters(root)?;
    let temp = if out.is_none() {
        Some(TempDir::new("rpc-contract-bundle")?)
    } else {
        None
    };
    let out_root = out.unwrap_or_else(|| temp.as_ref().expect("temp exists").path());
    fs::create_dir_all(out_root).map_err(|error| format!("{}: {error}", out_root.display()))?;
    let mut index = Vec::new();
    for map_path in &maps {
        let (target, map) = generate_one(map_path, out_root)?;
        if check {
            verify_bundle(&target, &map, map_path)?;
        }
        let contract = read_json(&target.join("contract.json"))?;
        index.push(json!({
            "source": contract["source"],
            "service": contract["service"],
            "contractSha256": contract["contractSha256"],
            "operationCount": contract["operations"].as_array().map_or(0, Vec::len),
        }));
    }
    index.sort_by_key(|value| value["source"].as_str().unwrap_or_default().to_owned());
    let contracts = Value::Array(index);
    let catalog = sha256_json(&contracts)?;
    let index_document = json!({
        "formatVersion": 1,
        "contracts": contracts,
        "catalogSha256": catalog,
    });
    write_json(&out_root.join("index.json"), &index_document)?;
    if check {
        let reread = read_json(&out_root.join("index.json"))?;
        let digest = sha256_json(&reread["contracts"])?;
        if reread["catalogSha256"] != digest {
            return Err("catalog digest mismatch".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::common::repo_root;

    #[test]
    fn all_v1_maps_generate_and_verify() {
        let root = repo_root();
        let maps = default_maps(&root)
            .unwrap()
            .into_iter()
            .filter(|path| read_json(path).unwrap()["schema_version"] == "1.0.0")
            .collect::<Vec<_>>();
        assert!(maps.len() >= 8);
        let temp = TempDir::new("bundle-test").unwrap();
        for path in maps {
            let (target, map) = generate_one(&path, temp.path()).unwrap();
            verify_bundle(&target, &map, &path).unwrap();
        }
        verify_ridl_emitters(&root).unwrap();
    }

    #[test]
    fn document_drift_is_a_veto() {
        let root = repo_root();
        let source = root.join("examples/rpc-transports.route-map.json");
        let temp = TempDir::new("bundle-drift").unwrap();
        let (target, map) = generate_one(&source, temp.path()).unwrap();
        let openapi_path = target.join("docs/openapi.json");
        let mut openapi = read_json(&openapi_path).unwrap();
        openapi["paths"]["/v1/items/{id}"]["get"]["operationId"] = json!("drift");
        write_json(&openapi_path, &openapi).unwrap();
        assert!(verify_bundle(&target, &map, &source).unwrap_err().contains("openapi"));
    }

    #[test]
    fn go_identifier_collision_is_rejected() {
        let contract = json!({
            "service": "x",
            "contractSha256": "a".repeat(64),
            "operations": [
                {"key":"foo_bar","path":"/a","methods":["GET"],"transports":["http"],"delivery":"direct"},
                {"key":"foo__bar","path":"/b","methods":["GET"],"transports":["http"],"delivery":"direct"}
            ]
        });
        assert!(gen_go(&contract).unwrap_err().contains("collision"));
    }
}
