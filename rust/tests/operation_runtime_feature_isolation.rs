//! The `operation-runtime` feature must stay usable without a server framework.
//!
//! Compiling is not enough to prove that: Cargo feature unification can pull a
//! dependency back in through a sibling edge while every `cfg` still looks
//! right. This test asks Cargo for the *resolved* graph of this crate with only
//! `operation-runtime` enabled and asserts on it.
//!
//! What is and is not asserted, deliberately:
//!
//! * `axum` and `axum-core` are absent from the whole graph.
//! * This crate has no *direct* edge to `axum`, `tower`, or `http-body-util`,
//!   and does have one to `http`.
//! * `tower` and `hyper` are **not** asserted absent from the transitive graph.
//!   They arrive through `jsonschema -> reqwest`, with or without any feature of
//!   this crate, and that predates this feature. Narrowing `jsonschema`'s
//!   features would change how remote `$ref`s resolve and is a separate
//!   decision; pretending the graph is cleaner than it is would make this test
//!   a lie.

use std::{collections::BTreeSet, path::Path, process::Command};

fn cargo_tree(extra: &[&str]) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .arg("tree")
        .arg("--manifest-path")
        .arg(&manifest)
        .args(["--locked", "--edges", "normal", "--prefix", "none"])
        .args(extra)
        .output()
        .expect("spawn cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 cargo tree output")
}

fn crate_names(tree: &str) -> BTreeSet<String> {
    tree.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

const RUNTIME_ONLY: &[&str] = &["--no-default-features", "--features", "operation-runtime"];

#[test]
fn operation_runtime_graph_contains_no_axum() {
    let names = crate_names(&cargo_tree(RUNTIME_ONLY));
    assert!(
        names.contains("ores-api-docs"),
        "unexpected tree: {names:?}"
    );
    for forbidden in ["axum", "axum-core", "axum-macros"] {
        assert!(
            !names.contains(forbidden),
            "`{forbidden}` must not be reachable from the operation-runtime feature"
        );
    }
}

#[test]
fn operation_runtime_direct_dependencies_are_framework_free() {
    let mut args = RUNTIME_ONLY.to_vec();
    args.extend(["--depth", "1"]);
    let mut direct = crate_names(&cargo_tree(&args));
    direct.remove("ores-api-docs");

    assert!(
        direct.contains("http"),
        "operation-runtime needs `http` for HeaderMap: {direct:?}"
    );
    for forbidden in ["axum", "tower", "http-body-util", "hyper", "tokio"] {
        assert!(
            !direct.contains(forbidden),
            "operation-runtime must not depend directly on `{forbidden}`: {direct:?}"
        );
    }
}

/// Guards the other direction: the default build must keep every server
/// dependency, or generated consumers that mount the Axum router break.
#[test]
fn default_features_still_link_the_axum_server_adapter() {
    let mut args = vec!["--depth", "1"];
    args.extend(["--features", "axum"]);
    let direct = crate_names(&cargo_tree(&args));
    for required in ["axum", "tower", "http-body-util", "http"] {
        assert!(
            direct.contains(required),
            "default build lost `{required}`: {direct:?}"
        );
    }
}

/// With no features at all the runtime is compiled out and `http` is not
/// linked directly -- the shape the client-only facade depends on.
#[test]
fn featureless_build_does_not_link_http_directly() {
    let direct = crate_names(&cargo_tree(&["--no-default-features", "--depth", "1"]));
    assert!(
        !direct.contains("http"),
        "`http` must stay optional for the client-only facade: {direct:?}"
    );
}

/// The conformance workflow is path-filtered. A runtime source file missing
/// from its filters means a PR touching only that file skips the job that tests
/// the runtime with Axum compiled out -- silently, because nothing fails.
#[test]
fn every_operation_runtime_source_triggers_the_conformance_workflow() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = std::fs::read_to_string(
        crate_dir.join("../.github/workflows/rpc-v1-runtime-conformance.yml"),
    )
    .expect("read conformance workflow");

    let mut sources = vec![
        "rust/src/typed_operation_context.rs".to_owned(),
        "rust/src/rpc_http_context.rs".to_owned(),
        "rust/src/rpc_shared_operation.rs".to_owned(),
        "rust/tests/operation_runtime_feature_isolation.rs".to_owned(),
    ];
    // Any future `operation_*.rs` module is covered without editing this test.
    for entry in std::fs::read_dir(crate_dir.join("src")).expect("read rust/src") {
        let name = entry.expect("dir entry").file_name();
        let name = name.to_string_lossy();
        if name.starts_with("operation_") && name.ends_with(".rs") {
            sources.push(format!("rust/src/{name}"));
        }
    }
    sources.sort();

    for source in &sources {
        let listed = workflow.matches(&format!("- \"{source}\"")).count();
        assert_eq!(
            listed, 2,
            "`{source}` must appear in both the pull_request and push path filters of \
             rpc-v1-runtime-conformance.yml (found {listed})"
        );
    }
}
