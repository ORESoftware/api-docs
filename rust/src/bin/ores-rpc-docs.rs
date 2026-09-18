//! Deterministic RPC documentation generator and conformance gate.
//!
//! No network, no clock, no model: every artifact is a pure function of
//! `contracts/rpc-client-options/v1/catalog.json`.
//!
//! ```text
//! ores-rpc-docs generate   # write Markdown, derived schema and fixture corpus
//! ores-rpc-docs check      # regenerate in memory and fail on any drift
//! ores-rpc-docs verify     # validate the corpus against both schema peers
//! ```
//!
//! `check` proves determinism (same input, byte-identical output). `verify`
//! proves correctness (the documented surface is exactly the surface the
//! authored JSON Schema admits, and the authored and derived peers agree).

use ores_api_docs::rpc_client_options::{
    canonical_json, conformance, emit_markdown, emit_rust, emit_schema, emit_typescript, fixtures,
    Catalog,
    AUTHORED_PLAN_SCHEMA_PATH, CATALOG_PATH, GENERATED_DIR, MARKDOWN_PATH, RUST_SURFACE_PATH,
    TS_RUNTIME_PATH, TS_TYPES_PATH,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "check".to_owned());
    let root = match args.next() {
        Some(path) => PathBuf::from(path),
        None => repository_root(),
    };

    let result = match command.as_str() {
        "generate" => run_generate(&root),
        "check" => run_check(&root),
        "verify" => run_verify(&root),
        other => Err(format!(
            "unknown command {other:?}; expected generate, check or verify"
        )),
    };

    match result {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("ores-rpc-docs {command}: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The artifacts this generator owns, as (repository-relative path, bytes).
fn artifacts(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let catalog_source = std::fs::read_to_string(root.join(CATALOG_PATH))
        .map_err(|error| format!("{CATALOG_PATH}: {error}"))?;
    let catalog = Catalog::parse(&catalog_source).map_err(|error| error.to_string())?;

    let mut out = BTreeMap::new();
    out.insert(
        MARKDOWN_PATH.to_owned(),
        emit_markdown::render(&catalog),
    );
    out.insert(
        format!("{GENERATED_DIR}/plan.derived.schema.json"),
        canonical_json(&emit_schema::derive(&catalog)),
    );
    out.insert(
        RUST_SURFACE_PATH.to_owned(),
        emit_rust::render(&catalog),
    );
    out.insert(
        TS_RUNTIME_PATH.to_owned(),
        emit_typescript::render_runtime(&catalog),
    );
    out.insert(
        TS_TYPES_PATH.to_owned(),
        emit_typescript::render_types(&catalog),
    );
    out.insert(
        format!("{GENERATED_DIR}/chain-conformance.json"),
        canonical_json(
            &serde_json::to_value(ConformanceFile {
                catalog_version: catalog.catalog_version.clone(),
                note: "Plans produced by the Rust builder. Every other language client replays the same steps and must produce byte-identical plans.",
                chains: conformance::cases(),
            })
            .map_err(|error| error.to_string())?,
        ),
    );
    out.insert(
        format!("{GENERATED_DIR}/plan-fixtures.json"),
        canonical_json(
            &serde_json::to_value(CorpusFile {
                catalog_version: catalog.catalog_version.clone(),
                fixtures: fixtures::corpus(&catalog),
            })
            .map_err(|error| error.to_string())?,
        ),
    );
    Ok(out)
}

#[derive(serde::Serialize)]
struct ConformanceFile {
    catalog_version: String,
    note: &'static str,
    chains: Vec<ores_api_docs::rpc_client_options::conformance::ChainCase>,
}

#[derive(serde::Serialize)]
struct CorpusFile {
    catalog_version: String,
    fixtures: Vec<fixtures::Fixture>,
}

fn run_generate(root: &Path) -> Result<String, String> {
    let artifacts = artifacts(root)?;
    for (relative, contents) in &artifacts {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("{relative}: {error}"))?;
        }
        std::fs::write(&path, contents).map_err(|error| format!("{relative}: {error}"))?;
    }
    let mut report = format!("generated {} artifacts\n", artifacts.len());
    for (relative, contents) in &artifacts {
        report.push_str(&format!("  {relative} ({} bytes)\n", contents.len()));
    }
    Ok(report.trim_end().to_owned())
}

fn run_check(root: &Path) -> Result<String, String> {
    // Determinism: generating twice from the same catalog must agree exactly.
    let first = artifacts(root)?;
    let second = artifacts(root)?;
    if first != second {
        return Err("generation is not deterministic: two runs disagreed".to_owned());
    }

    let mut drifted = Vec::new();
    for (relative, expected) in &first {
        let actual = std::fs::read_to_string(root.join(relative)).unwrap_or_default();
        if &actual != expected {
            drifted.push(relative.clone());
        }
    }
    if !drifted.is_empty() {
        return Err(format!(
            "committed artifacts are stale; re-run `ores-rpc-docs generate`:\n  {}",
            drifted.join("\n  ")
        ));
    }
    Ok(format!(
        "deterministic: {} artifacts reproduced byte-for-byte across two runs",
        first.len()
    ))
}

fn run_verify(root: &Path) -> Result<String, String> {
    let catalog_source = std::fs::read_to_string(root.join(CATALOG_PATH))
        .map_err(|error| format!("{CATALOG_PATH}: {error}"))?;
    let catalog = Catalog::parse(&catalog_source).map_err(|error| error.to_string())?;

    let authored_source = std::fs::read_to_string(root.join(AUTHORED_PLAN_SCHEMA_PATH))
        .map_err(|error| format!("{AUTHORED_PLAN_SCHEMA_PATH}: {error}"))?;
    let authored: serde_json::Value =
        serde_json::from_str(&authored_source).map_err(|error| error.to_string())?;
    let derived = emit_schema::derive(&catalog);

    let authored_validator = jsonschema::validator_for(&authored)
        .map_err(|error| format!("authored plan schema is not a valid schema: {error}"))?;
    let derived_validator = jsonschema::validator_for(&derived)
        .map_err(|error| format!("derived plan schema is not a valid schema: {error}"))?;

    let corpus = fixtures::corpus(&catalog);
    let mut authored_failures = Vec::new();
    let mut peer_disagreements = Vec::new();

    for fixture in &corpus {
        let authored_verdict = authored_validator.is_valid(&fixture.plan);
        let derived_verdict = derived_validator.is_valid(&fixture.plan);

        if authored_verdict != fixture.valid {
            let detail = authored_validator
                .iter_errors(&fixture.plan)
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            authored_failures.push(format!(
                "{}: expected {}, authored schema said {} ({})",
                fixture.fixture_id,
                verdict(fixture.valid),
                verdict(authored_verdict),
                if detail.is_empty() {
                    "no error reported".to_owned()
                } else {
                    detail
                }
            ));
        }
        if authored_verdict != derived_verdict {
            peer_disagreements.push(format!(
                "{}: authored said {}, derived said {}",
                fixture.fixture_id,
                verdict(authored_verdict),
                verdict(derived_verdict)
            ));
        }
    }

    if !authored_failures.is_empty() {
        return Err(format!(
            "{} of {} fixtures contradict the authored plan schema:\n  {}",
            authored_failures.len(),
            corpus.len(),
            authored_failures.join("\n  ")
        ));
    }
    if !peer_disagreements.is_empty() {
        return Err(format!(
            "authored and derived plan schemas disagree on {} of {} fixtures:\n  {}",
            peer_disagreements.len(),
            corpus.len(),
            peer_disagreements.join("\n  ")
        ));
    }

    let positives = corpus.iter().filter(|f| f.valid).count();
    Ok(format!(
        "verified {} fixtures ({} accepted, {} rejected); authored and derived plan schemas agree on every one",
        corpus.len(),
        positives,
        corpus.len() - positives
    ))
}

fn verdict(valid: bool) -> &'static str {
    if valid {
        "valid"
    } else {
        "invalid"
    }
}

/// Walk up from the crate directory to the repository root.
fn repository_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}
