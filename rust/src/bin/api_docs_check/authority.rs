use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use super::common::{normalize_prose, read_json, read_text, write_json, CheckResult};

const GOVERNANCE_FILES: &[&str] = &[
    "AGENTS.md",
    "README.md",
    "docs/rpc-contract-coupling.md",
    "idl/README.md",
    "idl/typespec/main.tsp",
    "idl/typespec/package.json",
];

const FORBIDDEN_HIERARCHY_MARKERS: &[&str] = &[
    "P0",
    "P1",
    "P2",
    "one authority for shared RPC",
    "cannot redefine the TypeSpec authority",
    "must not redefine P0",
    "begins in TypeSpec",
    "Change the shared semantic fact in TypeSpec first",
    "authoritative TypeSpec model",
];

fn expected_authorities() -> BTreeMap<&'static str, (&'static str, BTreeSet<&'static str>, BTreeSet<&'static str>)> {
    BTreeMap::from([
        (
            "typespec",
            (
                "human_authored_contract_authority",
                BTreeSet::from(["idl/typespec"]),
                BTreeSet::from(["sql", "protobuf", "grpc", "wire_clients"]),
            ),
        ),
        (
            "json-schema-openapi",
            (
                "human_authored_contract_authority",
                BTreeSet::from(["json-schema", "examples"]),
                BTreeSet::from(["client_interfaces", "client_types", "sql", "write_clients"]),
            ),
        ),
    ])
}

fn expected_comparisons() -> BTreeMap<&'static str, (&'static str, &'static str, BTreeSet<&'static str>)> {
    BTreeMap::from([
        (
            "typespec-vs-json-schema-openapi",
            (
                "typespec",
                "json-schema-openapi",
                BTreeSet::from(["normalized_models", "sql", "client_types"]),
            ),
        ),
        (
            "diesel-vs-seaorm",
            (
                "diesel",
                "seaorm",
                BTreeSet::from(["schema", "migrations", "constraints", "relations"]),
            ),
        ),
    ])
}

fn required_governance_markers(relative: &str) -> &'static [&'static str] {
    match relative {
        "AGENTS.md" => &["peer, top-level, human-authored contract authorities", "halt and evaluate"],
        "README.md" | "docs/rpc-contract-coupling.md" => {
            &["peer top-level contract authorities", "halt and evaluate"]
        }
        "idl/README.md" => &["peer top-level authorities", "halt and evaluate"],
        _ => &[],
    }
}

fn as_object<'a>(value: &'a Value, label: &str, errors: &mut Vec<String>) -> Option<&'a serde_json::Map<String, Value>> {
    let object = value.as_object();
    if object.is_none() {
        errors.push(format!("{label} must be an object"));
    }
    object
}

fn as_array<'a>(value: Option<&'a Value>, label: &str, errors: &mut Vec<String>) -> &'a [Value] {
    match value.and_then(Value::as_array) {
        Some(array) => array,
        None => {
            errors.push(format!("{label} must be an array"));
            &[]
        }
    }
}

fn string_set(values: &[Value]) -> BTreeSet<String> {
    values.iter().filter_map(Value::as_str).map(str::to_owned).collect()
}

fn validate_root(root: &Path, raw: &Value, label: &str, errors: &mut Vec<String>) {
    let Some(relative) = raw.as_str().filter(|value| !value.is_empty()) else {
        errors.push(format!("{label} must be a non-empty relative path"));
        return;
    };
    let candidate = Path::new(relative);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        errors.push(format!("{label} must remain inside the repository"));
        return;
    }
    let resolved = root.join(candidate);
    if !resolved.exists() {
        errors.push(format!("{label} does not exist: {relative}"));
    }
}

fn index_exact<'a>(
    raw: Option<&'a Value>,
    label: &str,
    expected_ids: &BTreeSet<&str>,
    errors: &mut Vec<String>,
) -> BTreeMap<String, &'a serde_json::Map<String, Value>> {
    let mut indexed = BTreeMap::new();
    for (index, raw_item) in as_array(raw, label, errors).iter().enumerate() {
        let Some(item) = raw_item.as_object() else {
            errors.push(format!("{label}[{index}] must be an object"));
            continue;
        };
        let Some(item_id) = item.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) else {
            errors.push(format!("{label}[{index}].id is required"));
            continue;
        };
        if indexed.insert(item_id.to_owned(), item).is_some() {
            errors.push(format!("duplicate {}: {item_id}", label.trim_end_matches('s')));
        }
    }
    let actual: BTreeSet<&str> = indexed.keys().map(String::as_str).collect();
    if &actual != expected_ids {
        errors.push(format!(
            "{label} set must be exact: got={actual:?}, expected={expected_ids:?}"
        ));
    }
    indexed
}

pub fn validate_contract(document: &Value, root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(contract) = as_object(document, "contract", &mut errors) else {
        return errors;
    };
    if contract.get("schemaVersion") != Some(&json!(1)) {
        errors.push("schemaVersion must equal 1".to_owned());
    }

    let policy_value = contract.get("policy").unwrap_or(&Value::Null);
    let policy = as_object(policy_value, "policy", &mut errors);
    let expected_policy = [
        ("authoritiesArePeers", json!(true)),
        ("authorityOrder", json!([])),
        ("automaticOverwriteAllowed", json!(false)),
        ("onUnexpectedDiscrepancy", json!("halt_and_evaluate")),
        ("productionPromotionRequiresAllMaterializedGates", json!(true)),
    ];
    if let Some(policy) = policy {
        for (key, expected) in expected_policy {
            if policy.get(key) != Some(&expected) {
                errors.push(format!("policy.{key} must equal {expected}"));
            }
        }
    }

    let authorities_expected = expected_authorities();
    let authority_ids: BTreeSet<&str> = authorities_expected.keys().copied().collect();
    let authorities = index_exact(contract.get("authorities"), "authorities", &authority_ids, &mut errors);
    for (authority_id, (kind, roots_expected, outputs_expected)) in authorities_expected {
        let Some(authority) = authorities.get(authority_id) else {
            continue;
        };
        if authority.get("kind").and_then(Value::as_str) != Some(kind) {
            errors.push(format!("{authority_id}.kind must equal {kind}"));
        }
        let roots = as_array(authority.get("roots"), &format!("{authority_id}.roots"), &mut errors);
        let roots_set = string_set(roots);
        let expected: BTreeSet<String> = roots_expected.iter().map(|value| (*value).to_owned()).collect();
        if roots_set != expected || roots.len() != roots_set.len() {
            errors.push(format!("{authority_id}.roots must be exact and duplicate-free"));
        }
        for (index, relative) in roots.iter().enumerate() {
            validate_root(root, relative, &format!("{authority_id}.roots[{index}]"), &mut errors);
        }
        let outputs = as_array(
            authority.get("requiredOutputs"),
            &format!("{authority_id}.requiredOutputs"),
            &mut errors,
        );
        let outputs_set = string_set(outputs);
        let expected: BTreeSet<String> = outputs_expected.iter().map(|value| (*value).to_owned()).collect();
        if outputs_set != expected || outputs.len() != outputs_set.len() {
            errors.push(format!(
                "{authority_id}.requiredOutputs must be exact and duplicate-free"
            ));
        }
    }

    let comparisons_expected = expected_comparisons();
    let comparison_ids: BTreeSet<&str> = comparisons_expected.keys().copied().collect();
    let comparisons = index_exact(
        contract.get("comparisons"),
        "comparisons",
        &comparison_ids,
        &mut errors,
    );
    for (comparison_id, (left, right, artifacts_expected)) in comparisons_expected {
        let Some(comparison) = comparisons.get(comparison_id) else {
            continue;
        };
        for (side, expected) in [("left", left), ("right", right)] {
            if comparison.get(side).and_then(Value::as_str) != Some(expected) {
                errors.push(format!("{comparison_id}.{side} must equal {expected}"));
            }
        }
        let artifacts = as_array(
            comparison.get("artifacts"),
            &format!("{comparison_id}.artifacts"),
            &mut errors,
        );
        let actual = string_set(artifacts);
        let expected: BTreeSet<String> = artifacts_expected.iter().map(|value| (*value).to_owned()).collect();
        if actual != expected || artifacts.len() != actual.len() {
            errors.push(format!("{comparison_id}.artifacts must be exact and duplicate-free"));
        }
        if comparison.get("onMismatch").and_then(Value::as_str) != Some("halt_and_evaluate") {
            errors.push(format!("{comparison_id}.onMismatch must be halt_and_evaluate"));
        }
    }

    let materialization_value = contract.get("materialization").unwrap_or(&Value::Null);
    if let Some(materialization) = as_object(materialization_value, "materialization", &mut errors) {
        let required = BTreeSet::from([
            "rpcModelCrossCheck",
            "digestBoundDocsAndClients",
            "typespecSqlEmitter",
            "jsonSchemaOpenApiSqlEmitter",
            "dieselSeaOrmCrossCheck",
        ]);
        let actual: BTreeSet<&str> = materialization.keys().map(String::as_str).collect();
        if actual != required {
            errors.push("materialization keys must be exact".to_owned());
        }
        for (name, status) in materialization {
            if !matches!(status.as_str(), Some("implemented") | Some("not_yet_materialized")) {
                errors.push(format!("materialization.{name} has invalid status {status}"));
            }
        }
        for implemented in ["rpcModelCrossCheck", "digestBoundDocsAndClients"] {
            if materialization.get(implemented).and_then(Value::as_str) != Some("implemented") {
                errors.push(format!("materialization.{implemented} must remain implemented"));
            }
        }
    }

    for relative in GOVERNANCE_FILES {
        let path = root.join(relative);
        let Ok(text) = read_text(&path) else {
            errors.push(format!("missing governance file: {relative}"));
            continue;
        };
        let normalized = normalize_prose(&text);
        for marker in FORBIDDEN_HIERARCHY_MARKERS {
            if normalized.contains(&normalize_prose(marker)) {
                errors.push(format!("{relative} retains obsolete hierarchy marker: {marker}"));
            }
        }
        for marker in required_governance_markers(relative) {
            if !normalized.contains(&normalize_prose(marker)) {
                errors.push(format!("{relative} is missing peer-authority marker: {marker}"));
            }
        }
    }

    errors
}

pub fn run_validate(root: &Path, contract_path: Option<&Path>) -> CheckResult<()> {
    let path = contract_path
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("idl/authority-contract.json"));
    let path = if path.is_absolute() { path } else { root.join(path) };
    let document = read_json(&path).map_err(|error| format!("unable to load authority contract: {error}"))?;
    let errors = validate_contract(&document, root);
    if errors.is_empty() {
        println!("peer-authority contract and convergence policy are valid");
        return Ok(());
    }
    let mut message = String::from("peer-authority contract veto; halt and evaluate\n");
    for error in errors {
        message.push_str("  ");
        message.push_str(&error);
        message.push('\n');
    }
    Err(message.trim_end().to_owned())
}

const REQUIRED_ARTIFACT_KEYS: &[&str] = &["sql", "clientTypes"];

fn walk(left: &Value, right: &Value, pointer: &str, differences: &mut Vec<Value>) {
    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            let left_keys: BTreeSet<&str> = left_map.keys().map(String::as_str).collect();
            let right_keys: BTreeSet<&str> = right_map.keys().map(String::as_str).collect();
            for key in left_keys.difference(&right_keys) {
                differences.push(json!({
                    "path": format!("{pointer}/{key}"),
                    "kind": "missing_right",
                    "left": left_map[*key],
                }));
            }
            for key in right_keys.difference(&left_keys) {
                differences.push(json!({
                    "path": format!("{pointer}/{key}"),
                    "kind": "missing_left",
                    "right": right_map[*key],
                }));
            }
            for key in left_keys.intersection(&right_keys) {
                walk(&left_map[*key], &right_map[*key], &format!("{pointer}/{key}"), differences);
            }
        }
        (Value::Array(left_items), Value::Array(right_items)) => {
            if left_items.len() != right_items.len() {
                differences.push(json!({
                    "path": if pointer.is_empty() { "/" } else { pointer },
                    "kind": "length_mismatch",
                    "left": left_items.len(),
                    "right": right_items.len(),
                }));
            }
            for (index, (left_item, right_item)) in left_items.iter().zip(right_items).enumerate() {
                walk(left_item, right_item, &format!("{pointer}/{index}"), differences);
            }
        }
        _ if std::mem::discriminant(left) != std::mem::discriminant(right) => {
            differences.push(json!({
                "path": if pointer.is_empty() { "/" } else { pointer },
                "kind": "type_mismatch",
                "left": value_kind(left),
                "right": value_kind(right),
            }));
        }
        _ if left != right => {
            differences.push(json!({
                "path": if pointer.is_empty() { "/" } else { pointer },
                "kind": "value_mismatch",
                "left": left,
                "right": right,
            }));
        }
        _ => {}
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

pub fn compare_manifests(left: &Value, right: &Value, left_label: &str, right_label: &str) -> Value {
    let mut errors = Vec::new();
    for (label, document) in [(left_label, left), (right_label, right)] {
        let Some(object) = document.as_object() else {
            errors.push(format!("{label}: manifest must be an object"));
            continue;
        };
        if object.get("schemaVersion") != Some(&json!(1)) {
            errors.push(format!("{label}: schemaVersion must equal 1"));
        }
        if object.get("authority").and_then(Value::as_str) != Some(label) {
            errors.push(format!("{label}: authority field must equal the supplied label"));
        }
        match object.get("artifacts").and_then(Value::as_object) {
            Some(artifacts)
                if REQUIRED_ARTIFACT_KEYS
                    .iter()
                    .all(|required| artifacts.contains_key(*required)) => {}
            Some(_) => errors.push(format!(
                "{label}: artifacts must include {:?}",
                REQUIRED_ARTIFACT_KEYS
            )),
            None => errors.push(format!("{label}: artifacts must be an object")),
        }
    }

    let mut differences = Vec::new();
    if errors.is_empty() {
        walk(&left["artifacts"], &right["artifacts"], "/artifacts", &mut differences);
    }
    let ok = errors.is_empty() && differences.is_empty();
    json!({
        "schemaVersion": 1,
        "ok": ok,
        "decision": if ok { "continue" } else { "halt_and_evaluate" },
        "left": left_label,
        "right": right_label,
        "errors": errors,
        "differences": differences,
    })
}

pub fn run_compare(
    left_path: &Path,
    right_path: &Path,
    left_label: &str,
    right_label: &str,
    write_report: Option<&Path>,
) -> CheckResult<()> {
    let left = read_json(left_path)
        .map_err(|error| format!("unable to load generated authority artifacts: {error}"))?;
    let right = read_json(right_path)
        .map_err(|error| format!("unable to load generated authority artifacts: {error}"))?;
    let report = compare_manifests(&left, &right, left_label, right_label);
    if let Some(path) = write_report {
        write_json(path, &report)?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    if report["ok"] == Value::Bool(true) {
        Ok(())
    } else {
        Err(format!(
            "{left_label} vs {right_label}: discrepancy detected; halt and evaluate"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::common::{copy_tree, repo_root, TempDir};

    fn manifest(authority: &str) -> Value {
        json!({
            "schemaVersion": 1,
            "authority": authority,
            "artifacts": {
                "sql": {
                    "accounts": {
                        "columns": [
                            {"name": "id", "type": "uuid", "nullable": false},
                            {"name": "email", "type": "text", "nullable": false}
                        ],
                        "primaryKey": ["id"]
                    }
                },
                "clientTypes": {"Account": {"id": "string", "email": "string"}}
            }
        })
    }

    #[test]
    fn current_authority_contract_is_green() {
        let root = repo_root();
        let document = read_json(&root.join("idl/authority-contract.json")).unwrap();
        assert_eq!(validate_contract(&document, &root), Vec::<String>::new());
    }

    #[test]
    fn authority_order_is_forbidden() {
        let root = repo_root();
        let mut document = read_json(&root.join("idl/authority-contract.json")).unwrap();
        document["policy"]["authorityOrder"] = json!(["typespec", "json-schema-openapi"]);
        assert!(validate_contract(&document, &root)
            .iter()
            .any(|error| error.contains("authorityOrder")));
    }

    #[test]
    fn hierarchy_marker_is_a_veto() {
        let source = repo_root();
        let temp = TempDir::new("authority-marker").unwrap();
        let root = temp.path();
        for relative in GOVERNANCE_FILES {
            let from = source.join(relative);
            let to = root.join(relative);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::copy(from, to).unwrap();
        }
        copy_tree(&source.join("json-schema"), &root.join("json-schema")).unwrap();
        copy_tree(&source.join("examples"), &root.join("examples")).unwrap();
        fs::create_dir_all(root.join("idl/typespec")).unwrap();
        let agents = root.join("AGENTS.md");
        let mut text = fs::read_to_string(&agents).unwrap();
        text.push_str("\nTypeSpec is P0.\n");
        fs::write(&agents, text).unwrap();
        let document = read_json(&source.join("idl/authority-contract.json")).unwrap();
        assert!(validate_contract(&document, root)
            .iter()
            .any(|error| error.contains("obsolete hierarchy marker")));
    }

    #[test]
    fn exact_authority_artifacts_continue() {
        let report = compare_manifests(
            &manifest("typespec"),
            &manifest("json-schema-openapi"),
            "typespec",
            "json-schema-openapi",
        );
        assert_eq!(report["ok"], true);
        assert_eq!(report["decision"], "continue");
    }

    #[test]
    fn sql_discrepancy_halts() {
        let left = manifest("typespec");
        let mut right = manifest("json-schema-openapi");
        right["artifacts"]["sql"]["accounts"]["columns"][1]["nullable"] = json!(true);
        let report = compare_manifests(&left, &right, "typespec", "json-schema-openapi");
        assert_eq!(report["ok"], false);
        assert!(report["differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|difference| difference["path"] == "/artifacts/sql/accounts/columns/1/nullable"));
    }

    #[test]
    fn missing_required_artifact_halts() {
        let left = manifest("typespec");
        let mut right = manifest("json-schema-openapi");
        right["artifacts"].as_object_mut().unwrap().remove("sql");
        let report = compare_manifests(&left, &right, "typespec", "json-schema-openapi");
        assert_eq!(report["ok"], false);
        assert!(report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error.as_str().unwrap().contains("must include")));
    }
}
