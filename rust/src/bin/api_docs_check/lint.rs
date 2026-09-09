use std::fs;
use std::path::{Path, PathBuf};

use super::common::{read_text, relative_path, CheckResult};

const LEGACY_CHECK_SCRIPTS: &[&str] = &[
    "scripts/generate-routes.py",
    "scripts/test_validate_authority_contract.py",
    "scripts/validate-authority-contract.py",
    "scripts/test_compare_authority_artifacts.py",
    "scripts/compare-authority-artifacts.py",
    "scripts/test_cross_check_rpc_idl.py",
    "scripts/cross-check-rpc-idl.py",
    "scripts/test_audit_rpc_idl.py",
    "scripts/audit-rpc-idl.py",
    "scripts/test_rpc_contract_bundle.py",
    "scripts/rpc-contract-bundle.py",
];

fn workflow_paths(root: &Path) -> CheckResult<Vec<PathBuf>> {
    let directory = root.join(".github/workflows");
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("yml" | "yaml")
            )
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn is_full_sha(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn lint_workflow(path: &Path, root: &Path, failures: &mut Vec<String>) -> CheckResult<()> {
    let text = read_text(path)?;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if let Some(reference) = line.strip_prefix("uses:").map(str::trim) {
            let reference = reference.split_whitespace().next().unwrap_or_default();
            if !reference.starts_with("./") {
                let Some((_, revision)) = reference.rsplit_once('@') else {
                    failures.push(format!(
                        "{}:{}: external action is missing an immutable SHA: {reference}",
                        relative_path(root, path),
                        index + 1
                    ));
                    continue;
                };
                if !is_full_sha(revision) {
                    failures.push(format!(
                        "{}:{}: mutable action reference: {reference}",
                        relative_path(root, path),
                        index + 1
                    ));
                }
            }
        }
        if line == "runs-on: ubuntu-latest" {
            failures.push(format!(
                "{}:{}: mutable runner label",
                relative_path(root, path),
                index + 1
            ));
        }
    }
    Ok(())
}

fn canonical_command() -> &'static str {
    "cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check --"
}

fn official_command_files(root: &Path) -> CheckResult<Vec<PathBuf>> {
    let mut paths = vec![
        root.join("README.md"),
        root.join("idl/README.md"),
        root.join("docs/rpc-contract-coupling.md"),
        root.join("generated/README.md"),
        root.join(".zpkg.toml"),
    ];
    paths.extend(workflow_paths(root)?);
    Ok(paths)
}

fn lint_legacy_invocations(path: &Path, root: &Path, failures: &mut Vec<String>) -> CheckResult<()> {
    let text = read_text(path)?;
    for (index, line) in text.lines().enumerate() {
        let looks_like_python = line.contains("python ")
            || line.contains("python3 ")
            || line.contains("python -")
            || line.contains("python3 -");
        if !looks_like_python {
            continue;
        }
        for legacy in LEGACY_CHECK_SCRIPTS {
            if line.contains(legacy) {
                failures.push(format!(
                    "{}:{}: legacy Python check invocation {legacy}; use `{}`",
                    relative_path(root, path),
                    index + 1,
                    canonical_command()
                ));
            }
        }
    }
    Ok(())
}

pub fn lint_repository(root: &Path) -> CheckResult<Vec<String>> {
    let mut failures = Vec::new();
    for path in workflow_paths(root)? {
        lint_workflow(&path, root, &mut failures)?;
    }
    for path in official_command_files(root)? {
        if path.is_file() {
            lint_legacy_invocations(&path, root, &mut failures)?;
        }
    }
    failures.sort();
    failures.dedup();
    Ok(failures)
}

pub fn run_lint(root: &Path) -> CheckResult<()> {
    let failures = lint_repository(root)?;
    if failures.is_empty() {
        println!("repository CLI/workflow lint ok");
        return Ok(());
    }
    let mut message = String::from("repository lint veto:\n");
    for failure in failures {
        message.push_str("  ");
        message.push_str(&failure);
        message.push('\n');
    }
    Err(message.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::common::{repo_root, TempDir};

    #[test]
    fn immutable_action_detection_is_strict() {
        assert!(is_full_sha("3d3c42e5aac5ba805825da76410c181273ba90b1"));
        assert!(!is_full_sha("v7.0.1"));
        assert!(!is_full_sha("main"));
    }

    #[test]
    fn workflow_lint_rejects_mutable_references_and_runner() {
        let temp = TempDir::new("workflow-lint").unwrap();
        let path = temp.path().join(".github/workflows/x.yml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "jobs:\n  x:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@main\n",
        )
        .unwrap();
        let mut failures = Vec::new();
        lint_workflow(&path, temp.path(), &mut failures).unwrap();
        assert!(failures.iter().any(|failure| failure.contains("mutable runner")));
        assert!(failures.iter().any(|failure| failure.contains("mutable action")));
    }

    #[test]
    fn repository_lint_has_no_legacy_check_invocations_after_migration() {
        let root = repo_root();
        let failures = lint_repository(&root).unwrap();
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
