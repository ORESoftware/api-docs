//! Validate durable evidence for the co-equal TypeSpec and JSON Schema/OpenAPI lanes.
//!
//! This binary deliberately validates provenance and convergence topology rather
//! than translating one authority into the other. Generated translations are
//! comparison evidence only.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const AUTHORITY_IDS: [&str; 2] = ["json-schema-openapi", "typespec"];
const ARTIFACT_IDS: [&str; 4] = [
    "clientTypes",
    "generatedCode",
    "normalizedModels",
    "sql",
];
const STOPPED: &str = "STOPPED_FOR_EVALUATION";

fn object<'a>(
    value: Option<&'a Value>,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<&'a Map<String, Value>> {
    match value.and_then(Value::as_object) {
        Some(value) => Some(value),
        None => {
            errors.push(format!("{label} must be an object"));
            None
        }
    }
}

fn array<'a>(
    value: Option<&'a Value>,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<&'a Vec<Value>> {
    match value.and_then(Value::as_array) {
        Some(value) => Some(value),
        None => {
            errors.push(format!("{label} must be an array"));
            None
        }
    }
}

fn require_bool(
    object: &Map<String, Value>,
    key: &str,
    expected: bool,
    label: &str,
    errors: &mut Vec<String>,
) {
    if object.get(key).and_then(Value::as_bool) != Some(expected) {
        errors.push(format!("{label}.{key} must equal {expected}"));
    }
}

fn require_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<&'a str> {
    match object.get(key).and_then(Value::as_str) {
        Some(value) if !value.is_empty() => Some(value),
        _ => {
            errors.push(format!("{label}.{key} must be a non-empty string"));
            None
        }
    }
}

fn string_set(values: &[Value], label: &str, errors: &mut Vec<String>) -> BTreeSet<String> {
    let mut output = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        match value.as_str() {
            Some(value) if !value.is_empty() => {
                if !output.insert(value.to_owned()) {
                    errors.push(format!("{label}[{index}] duplicates {value:?}"));
                }
            }
            _ => errors.push(format!("{label}[{index}] must be a non-empty string")),
        }
    }
    output
}

fn is_lower_hex_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_authorities(
    root: &Map<String, Value>,
    errors: &mut Vec<String>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut roots_by_authority = BTreeMap::new();
    let mut ids = BTreeSet::new();

    if let Some(authorities) = array(root.get("authorities"), "authorities", errors) {
        for (index, authority) in authorities.iter().enumerate() {
            let label = format!("authorities[{index}]");
            let Some(authority) = object(Some(authority), &label, errors) else {
                continue;
            };
            let Some(id) = require_string(authority, "id", &label, errors) else {
                continue;
            };
            if !ids.insert(id.to_owned()) {
                errors.push(format!("duplicate authority id {id:?}"));
            }
            if authority.get("kind").and_then(Value::as_str)
                != Some("human_authored_top_level_authority")
            {
                errors.push(format!(
                    "{label}.kind must equal human_authored_top_level_authority"
                ));
            }
            require_bool(authority, "humanAuthored", true, &label, errors);
            require_bool(authority, "topLevel", true, &label, errors);
            if !matches!(authority.get("generatedFromAuthority"), Some(Value::Null)) {
                errors.push(format!(
                    "{label}.generatedFromAuthority must be null; an authority cannot be generated from its peer"
                ));
            }

            let mut roots = BTreeSet::new();
            if let Some(raw_roots) = array(
                authority.get("sourceRoots"),
                &format!("{label}.sourceRoots"),
                errors,
            ) {
                roots = string_set(raw_roots, &format!("{label}.sourceRoots"), errors);
                if roots.is_empty() {
                    errors.push(format!("{label}.sourceRoots must not be empty"));
                }
                for root in &roots {
                    let path = Path::new(root);
                    if path.is_absolute()
                        || path
                            .components()
                            .any(|part| matches!(part, std::path::Component::ParentDir))
                    {
                        errors.push(format!("{label}.sourceRoots contains unsafe path {root:?}"));
                    }
                }
            }
            roots_by_authority.insert(id.to_owned(), roots);
        }
    }

    let expected: BTreeSet<String> = AUTHORITY_IDS.iter().map(|value| (*value).to_owned()).collect();
    if ids != expected {
        errors.push(format!(
            "authority id set must be exact: got={ids:?}, expected={expected:?}"
        ));
    }

    if let (Some(typespec), Some(json_schema)) = (
        roots_by_authority.get("typespec"),
        roots_by_authority.get("json-schema-openapi"),
    ) {
        let overlap: Vec<_> = typespec.intersection(json_schema).cloned().collect();
        if !overlap.is_empty() {
            errors.push(format!(
                "authority source roots must be disjoint; overlap={overlap:?}"
            ));
        }
        if !typespec.iter().any(|root| root.starts_with("idl/typespec")) {
            errors.push("typespec authority must retain an idl/typespec source root".into());
        }
        if !json_schema
            .iter()
            .any(|root| root.starts_with("json-schema"))
        {
            errors.push(
                "json-schema-openapi authority must retain a json-schema source root".into(),
            );
        }
    }

    roots_by_authority
}

fn validate_artifacts(root: &Map<String, Value>, errors: &mut Vec<String>) {
    let Some(artifacts) = object(root.get("artifacts"), "artifacts", errors) else {
        return;
    };
    let actual: BTreeSet<String> = artifacts.keys().cloned().collect();
    let expected: BTreeSet<String> = ARTIFACT_IDS.iter().map(|value| (*value).to_owned()).collect();
    if actual != expected {
        errors.push(format!(
            "artifact set must be exact: got={actual:?}, expected={expected:?}"
        ));
    }

    let expected_comparisons = BTreeMap::from([
        ("clientTypes", "language_semantics_and_fixtures"),
        (
            "generatedCode",
            "target_language_ast_and_compile_runtime_conformance",
        ),
        ("normalizedModels", "semantic_ir"),
        ("sql", "normalized_postgresql_catalog"),
    ]);
    let expected_producers: BTreeSet<String> =
        AUTHORITY_IDS.iter().map(|value| (*value).to_owned()).collect();

    for (artifact_id, comparison) in expected_comparisons {
        let label = format!("artifacts.{artifact_id}");
        let Some(artifact) = object(artifacts.get(artifact_id), &label, errors) else {
            continue;
        };
        if artifact.get("comparison").and_then(Value::as_str) != Some(comparison) {
            errors.push(format!("{label}.comparison must equal {comparison}"));
        }
        let mut producers = BTreeSet::new();
        if let Some(raw_producers) = array(
            artifact.get("producers"),
            &format!("{label}.producers"),
            errors,
        ) {
            for (index, producer) in raw_producers.iter().enumerate() {
                let producer_label = format!("{label}.producers[{index}]");
                let Some(producer) = object(Some(producer), &producer_label, errors) else {
                    continue;
                };
                if let Some(authority) =
                    require_string(producer, "authority", &producer_label, errors)
                {
                    if !producers.insert(authority.to_owned()) {
                        errors.push(format!(
                            "{producer_label}.authority duplicates {authority:?}"
                        ));
                    }
                }
                require_bool(producer, "independentSource", true, &producer_label, errors);
                require_bool(
                    producer,
                    "generatedFromOtherAuthority",
                    false,
                    &producer_label,
                    errors,
                );
                require_string(producer, "pipeline", &producer_label, errors);
            }
        }
        if producers != expected_producers {
            errors.push(format!(
                "{label} must have exactly one independent producer per authority: got={producers:?}"
            ));
        }
    }

    if let Some(sql) = artifacts.get("sql").and_then(Value::as_object) {
        if sql.get("requiredWhen").and_then(Value::as_str) != Some("persistence_model") {
            errors.push("artifacts.sql.requiredWhen must equal persistence_model".into());
        }
        require_bool(sql, "readBackRequired", true, "artifacts.sql", errors);
        require_bool(
            sql,
            "noSqlRequiresDualIndependentReceipts",
            true,
            "artifacts.sql",
            errors,
        );
    }
}

fn validate_translations(root: &Map<String, Value>, errors: &mut Vec<String>) {
    let mut directions = BTreeSet::new();
    if let Some(translations) = array(root.get("translations"), "translations", errors) {
        for (index, translation) in translations.iter().enumerate() {
            let label = format!("translations[{index}]");
            let Some(translation) = object(Some(translation), &label, errors) else {
                continue;
            };
            let from = require_string(translation, "from", &label, errors);
            let to = require_string(translation, "to", &label, errors);
            if let (Some(from), Some(to)) = (from, to) {
                directions.insert((from.to_owned(), to.to_owned()));
            }
            if translation.get("class").and_then(Value::as_str)
                != Some("derived_comparison_evidence")
            {
                errors.push(format!(
                    "{label}.class must equal derived_comparison_evidence"
                ));
            }
            require_bool(
                translation,
                "mayOverwriteAuthority",
                false,
                &label,
                errors,
            );
        }
    }
    let expected = BTreeSet::from([
        ("json-schema-openapi".to_owned(), "typespec".to_owned()),
        ("typespec".to_owned(), "json-schema-openapi".to_owned()),
    ]);
    if directions != expected {
        errors.push(format!(
            "translations must contain both comparison-only directions: got={directions:?}"
        ));
    }
}

fn validate_orm(root: &Map<String, Value>, errors: &mut Vec<String>) {
    let Some(orm) = object(root.get("ormCrossCheck"), "ormCrossCheck", errors) else {
        return;
    };
    if orm.get("dieselWitnessAuthority").and_then(Value::as_str) != Some("typespec") {
        errors.push("ormCrossCheck.dieselWitnessAuthority must equal typespec".into());
    }
    if orm.get("seaOrmWitnessAuthority").and_then(Value::as_str)
        != Some("json-schema-openapi")
    {
        errors.push(
            "ormCrossCheck.seaOrmWitnessAuthority must equal json-schema-openapi".into(),
        );
    }
    if orm.get("comparison").and_then(Value::as_str)
        != Some("structural_and_postgresql_catalog_behavior")
    {
        errors.push(
            "ormCrossCheck.comparison must equal structural_and_postgresql_catalog_behavior"
                .into(),
        );
    }
    if !matches!(orm.get("automaticWinner"), Some(Value::Null)) {
        errors.push("ormCrossCheck.automaticWinner must be null".into());
    }
    if orm.get("onMismatch").and_then(Value::as_str) != Some(STOPPED) {
        errors.push(format!("ormCrossCheck.onMismatch must equal {STOPPED}"));
    }
}

fn validate_references(root: &Map<String, Value>, errors: &mut Vec<String>) {
    let mut references = BTreeMap::<String, BTreeSet<String>>::new();
    if let Some(items) = array(
        root.get("referenceImplementations"),
        "referenceImplementations",
        errors,
    ) {
        for (index, item) in items.iter().enumerate() {
            let label = format!("referenceImplementations[{index}]");
            let Some(item) = object(Some(item), &label, errors) else {
                continue;
            };
            let Some(id) = require_string(item, "id", &label, errors) else {
                continue;
            };
            require_string(item, "repository", &label, errors);
            match require_string(item, "commit", &label, errors) {
                Some(commit) if !is_lower_hex_commit(commit) => errors.push(format!(
                    "{label}.commit must be an exact 40-character lowercase Git commit"
                )),
                _ => {}
            }
            if item.get("status").and_then(Value::as_str) != Some("merged") {
                errors.push(format!("{label}.status must equal merged"));
            }
            let artifacts = array(item.get("artifacts"), &format!("{label}.artifacts"), errors)
                .map(|items| string_set(items, &format!("{label}.artifacts"), errors))
                .unwrap_or_default();
            if let Some(paths) =
                array(item.get("evidencePaths"), &format!("{label}.evidencePaths"), errors)
            {
                let paths = string_set(paths, &format!("{label}.evidencePaths"), errors);
                if paths.is_empty() {
                    errors.push(format!("{label}.evidencePaths must not be empty"));
                }
                for path in paths {
                    let path = Path::new(&path);
                    if path.is_absolute()
                        || path
                            .components()
                            .any(|part| matches!(part, std::path::Component::ParentDir))
                    {
                        errors.push(format!("{label}.evidencePaths contains an unsafe path"));
                    }
                }
            }
            if references.insert(id.to_owned(), artifacts).is_some() {
                errors.push(format!("duplicate reference implementation id {id:?}"));
            }
        }
    }

    let expected_ids = BTreeSet::from([
        "api-docs-interface-code-parity".to_owned(),
        "ores-middleware-persistence-convergence".to_owned(),
    ]);
    let actual_ids: BTreeSet<String> = references.keys().cloned().collect();
    if actual_ids != expected_ids {
        errors.push(format!(
            "reference implementation set must be exact: got={actual_ids:?}, expected={expected_ids:?}"
        ));
    }

    let api_required = BTreeSet::from([
        "clientTypes".to_owned(),
        "generatedCode".to_owned(),
        "normalizedModels".to_owned(),
    ]);
    if references.get("api-docs-interface-code-parity") != Some(&api_required) {
        errors.push(
            "api-docs reference must prove normalized-model, client-type, and generated-code parity"
                .into(),
        );
    }
    let middleware_required = BTreeSet::from([
        "clientTypes".to_owned(),
        "generatedCode".to_owned(),
        "normalizedModels".to_owned(),
        "sql".to_owned(),
    ]);
    if references.get("ores-middleware-persistence-convergence")
        != Some(&middleware_required)
    {
        errors.push(
            "ores-middleware reference must prove both SQL lanes plus normalized-model, type, and code parity"
                .into(),
        );
    }
}

fn validate_document(document: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(root) = object(Some(document), "document", &mut errors) else {
        return errors;
    };
    if root.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        errors.push("schemaVersion must equal 1".into());
    }

    if let Some(policy) = object(root.get("policy"), "policy", &mut errors) {
        require_bool(policy, "authoritiesArePeers", true, "policy", &mut errors);
        require_bool(
            policy,
            "automaticOverwriteAllowed",
            false,
            "policy",
            &mut errors,
        );
        require_bool(
            policy,
            "independentSourceEvidenceRequired",
            true,
            "policy",
            &mut errors,
        );
        if policy
            .get("authorityOrder")
            .and_then(Value::as_array)
            .is_none_or(|order| !order.is_empty())
        {
            errors.push("policy.authorityOrder must be an empty array".into());
        }
        if policy
            .get("onUnexpectedDiscrepancy")
            .and_then(Value::as_str)
            != Some(STOPPED)
        {
            errors.push(format!(
                "policy.onUnexpectedDiscrepancy must equal {STOPPED}"
            ));
        }
    }

    validate_authorities(root, &mut errors);
    validate_artifacts(root, &mut errors);
    validate_translations(root, &mut errors);
    validate_orm(root, &mut errors);
    validate_references(root, &mut errors);
    errors
}

fn default_evidence_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust crate must remain below repository root")
        .join("idl/authority-evidence.json")
}

fn check_path(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("unable to read {}: {error}", path.display()))?;
    let document: Value = serde_json::from_str(&source)
        .map_err(|error| format!("unable to parse {}: {error}", path.display()))?;
    let errors = validate_document(&document);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "peer-authority evidence veto; {STOPPED}\n  {}",
            errors.join("\n  ")
        ))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() > 2 || args.first().is_some_and(|value| value != "--check") {
        eprintln!("usage: authority_evidence [--check [path]]");
        return ExitCode::from(64);
    }
    let path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_evidence_path);
    match check_path(&path) {
        Ok(()) => {
            println!("peer TypeSpec and JSON Schema/OpenAPI artifact evidence is valid");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../idl/authority-evidence.json"))
            .expect("repository evidence must parse")
    }

    #[test]
    fn repository_evidence_is_valid() {
        assert_eq!(validate_document(&fixture()), Vec::<String>::new());
    }

    #[test]
    fn authority_precedence_is_rejected() {
        let mut document = fixture();
        document["policy"]["authorityOrder"] = json!(["typespec", "json-schema-openapi"]);
        assert!(validate_document(&document)
            .iter()
            .any(|error| error.contains("authorityOrder")));
    }

    #[test]
    fn generated_peer_authority_is_rejected() {
        let mut document = fixture();
        document["authorities"][1]["generatedFromAuthority"] = json!("typespec");
        assert!(validate_document(&document)
            .iter()
            .any(|error| error.contains("generatedFromAuthority")));
    }

    #[test]
    fn one_sided_sql_is_rejected() {
        let mut document = fixture();
        document["artifacts"]["sql"]["producers"]
            .as_array_mut()
            .expect("SQL producers")
            .pop();
        assert!(validate_document(&document)
            .iter()
            .any(|error| error.contains("exactly one independent producer")));
    }

    #[test]
    fn implicit_no_sql_decision_is_rejected() {
        let mut document = fixture();
        document["artifacts"]["sql"]["noSqlRequiresDualIndependentReceipts"] = json!(false);
        assert!(validate_document(&document)
            .iter()
            .any(|error| error.contains("noSqlRequiresDualIndependentReceipts")));
    }

    #[test]
    fn mutable_reference_is_rejected() {
        let mut document = fixture();
        document["referenceImplementations"][0]["commit"] = json!("main");
        assert!(validate_document(&document)
            .iter()
            .any(|error| error.contains("exact 40-character")));
    }
}
