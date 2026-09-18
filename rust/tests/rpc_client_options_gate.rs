//! Repository gate for the RPC client option contract.
//!
//! Three properties, each checked here so CI fails on the exact one that broke:
//!
//! 1. generation is deterministic — the same catalog yields the same bytes;
//! 2. the committed artifacts are not stale;
//! 3. the documented surface is exactly the surface the authored JSON Schema
//!    admits, and the authored and derived schema peers agree on every fixture.
//!
//! No network, no clock, no model: this is the whole "docs are generated
//! deterministically by CLI tooling and are correct" claim, in executable form.

use ores_api_docs::rpc_client_options::{
    conformance, emit_schema, fixtures, Catalog, AUTHORED_PLAN_SCHEMA_PATH, CATALOG_PATH,
};
use std::path::{Path, PathBuf};
use std::process::Command;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn catalog() -> Catalog {
    let source = std::fs::read_to_string(repository_root().join(CATALOG_PATH))
        .expect("catalog is readable");
    Catalog::parse(&source).expect("catalog passes its integrity checks")
}

fn run(command: &str) -> (bool, String) {
    let root = repository_root();
    let output = Command::new(env!("CARGO_BIN_EXE_ores-rpc-docs"))
        .arg(command)
        .arg(&root)
        .output()
        .expect("generator runs");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

#[test]
fn generation_is_deterministic_and_committed_artifacts_are_current() {
    let (ok, report) = run("check");
    assert!(ok, "{report}");
    assert!(report.contains("byte-for-byte"), "{report}");
}

#[test]
fn documented_surface_matches_the_authored_schema() {
    let (ok, report) = run("verify");
    assert!(ok, "{report}");
}

#[test]
fn the_fixture_corpus_exercises_both_verdicts_on_both_surfaces() {
    // A corpus of only positives would pass a schema that admits everything.
    let corpus = fixtures::corpus(&catalog());
    let positives = corpus.iter().filter(|f| f.valid).count();
    let negatives = corpus.len() - positives;
    assert!(positives >= 50, "expected a broad positive corpus, got {positives}");
    assert!(negatives >= 20, "expected a broad negative corpus, got {negatives}");

    for prefix in [
        "negative.surface.",
        "negative.exclusive.",
        "negative.bounds.",
        "negative.structural.",
    ] {
        assert!(
            corpus.iter().any(|f| f.fixture_id.starts_with(prefix)),
            "the corpus must contain {prefix} fixtures"
        );
    }
    for surface in ["unary", "stream"] {
        assert!(
            corpus
                .iter()
                .any(|f| f.fixture_id.starts_with(&format!("positive.{surface}."))),
            "the corpus must cover the {surface} surface"
        );
    }
}

#[test]
fn every_catalog_option_appears_in_the_generated_documentation() {
    let catalog = catalog();
    let markdown = std::fs::read_to_string(
        repository_root().join(ores_api_docs::rpc_client_options::MARKDOWN_PATH),
    )
    .expect("generated markdown is readable");

    for option in &catalog.options {
        assert!(
            markdown.contains(&format!("`{}`", option.option_id)),
            "{} is absent from the generated documentation",
            option.option_id
        );
        assert!(
            markdown.contains(&option.summary),
            "{} has no summary in the generated documentation",
            option.option_id
        );
    }
}

#[test]
fn the_two_surfaces_stay_disjoint_where_the_catalog_says_so() {
    let catalog = catalog();
    let unary: Vec<&str> = catalog
        .unary_options()
        .map(|o| o.option_id.as_str())
        .collect();
    let stream: Vec<&str> = catalog
        .stream_options()
        .map(|o| o.option_id.as_str())
        .collect();

    assert!(unary.contains(&"make_call"), "the unary surface must terminate in make_call");
    assert!(!stream.contains(&"make_call"), "make_call must not reach the streaming surface");
    assert!(stream.contains(&"stream"), "the streaming surface must terminate in stream");
    assert!(!unary.contains(&"stream"), "stream must not reach the unary surface");
}

#[test]
fn cross_language_chain_plans_satisfy_the_authored_schema() {
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repository_root().join(AUTHORED_PLAN_SCHEMA_PATH))
            .expect("authored plan schema is readable"),
    )
    .expect("authored plan schema is valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");

    let cases = conformance::cases();
    assert!(cases.len() >= 8, "the conformance corpus should be meaningful");
    for case in cases {
        let errors: Vec<String> = validator
            .iter_errors(&case.plan)
            .map(|error| error.to_string())
            .collect();
        assert!(errors.is_empty(), "{}: {}", case.chain_id, errors.join("; "));
    }
}

#[test]
fn the_derived_schema_is_a_valid_schema_and_covers_every_plan_field() {
    let catalog = catalog();
    let derived = emit_schema::derive(&catalog);
    jsonschema::validator_for(&derived).expect("derived schema compiles");

    let properties = derived["properties"]
        .as_object()
        .expect("derived schema has properties");
    for option in &catalog.options {
        let Some(field) = option.plan_field.as_deref() else {
            continue;
        };
        assert!(
            properties.contains_key(field),
            "{} writes plan field {field}, which the derived schema omits",
            option.option_id
        );
    }
    for identity in emit_schema::IDENTITY_FIELDS {
        assert!(properties.contains_key(identity), "missing identity field {identity}");
    }
}
