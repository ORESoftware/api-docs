#![forbid(unsafe_code)]
#![deny(clippy::all)]

#[path = "api_docs_check/authority.rs"]
mod authority;
#[path = "api_docs_check/bundle.rs"]
mod bundle;
#[path = "api_docs_check/common.rs"]
mod common;
#[path = "api_docs_check/idl.rs"]
mod idl;
#[path = "api_docs_check/lint.rs"]
mod lint;
#[path = "api_docs_check/routes.rs"]
mod routes;

use std::env;
use std::path::{Path, PathBuf};

use common::{repo_root, CheckResult};

const CARGO_RUN: &str =
    "cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check --";

fn usage() -> String {
    format!(
        "api-docs-check\n\n\
         Usage:\n\
           {CARGO_RUN} all\n\
           {CARGO_RUN} generate-routes [--check] [--map PATH]... [--out PATH]\n\
           {CARGO_RUN} validate-authority-contract [--root PATH] [--contract PATH]\n\
           {CARGO_RUN} compare-authority-artifacts --left PATH --right PATH --left-label NAME --right-label NAME [--write-report PATH]\n\
           {CARGO_RUN} cross-check-rpc-idl [--root PATH] [--write-report PATH]\n\
           {CARGO_RUN} audit-rpc-idl [--root PATH]\n\
           {CARGO_RUN} rpc-contract-bundle [--check] [--map PATH]... [--out PATH]\n\
           {CARGO_RUN} lint [--root PATH]\n"
    )
}

fn require_value(args: &[String], index: &mut usize, flag: &str) -> CheckResult<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn root_flag(args: &[String]) -> CheckResult<PathBuf> {
    let mut root = repo_root();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => root = PathBuf::from(require_value(args, &mut index, "--root")?),
            flag => return Err(format!("unexpected argument: {flag}\n\n{}", usage())),
        }
        index += 1;
    }
    Ok(root)
}

fn generate_routes(args: &[String]) -> CheckResult<()> {
    let root = repo_root();
    let mut maps = Vec::new();
    let mut out = root.join("generated");
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--map" => maps.push(PathBuf::from(require_value(args, &mut index, "--map")?)),
            "--out" => out = PathBuf::from(require_value(args, &mut index, "--out")?),
            "--check" => check = true,
            flag => return Err(format!("unexpected generate-routes argument: {flag}")),
        }
        index += 1;
    }
    let out = if out.is_absolute() { out } else { root.join(out) };
    routes::run_generate_routes(&root, &maps, &out, check)
}

fn validate_authority(args: &[String]) -> CheckResult<()> {
    let mut root = repo_root();
    let mut contract = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => root = PathBuf::from(require_value(args, &mut index, "--root")?),
            "--contract" => {
                contract = Some(PathBuf::from(require_value(args, &mut index, "--contract")?));
            }
            flag => return Err(format!("unexpected validate-authority-contract argument: {flag}")),
        }
        index += 1;
    }
    authority::run_validate(&root, contract.as_deref())
}

fn compare_authorities(args: &[String]) -> CheckResult<()> {
    let mut left = None;
    let mut right = None;
    let mut left_label = None;
    let mut right_label = None;
    let mut write_report = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--left" => left = Some(PathBuf::from(require_value(args, &mut index, "--left")?)),
            "--right" => right = Some(PathBuf::from(require_value(args, &mut index, "--right")?)),
            "--left-label" => left_label = Some(require_value(args, &mut index, "--left-label")?),
            "--right-label" => right_label = Some(require_value(args, &mut index, "--right-label")?),
            "--write-report" => {
                write_report = Some(PathBuf::from(require_value(args, &mut index, "--write-report")?));
            }
            flag => return Err(format!("unexpected compare-authority-artifacts argument: {flag}")),
        }
        index += 1;
    }
    authority::run_compare(
        left.as_deref().ok_or("--left is required")?,
        right.as_deref().ok_or("--right is required")?,
        left_label.as_deref().ok_or("--left-label is required")?,
        right_label.as_deref().ok_or("--right-label is required")?,
        write_report.as_deref(),
    )
}

fn cross_check(args: &[String]) -> CheckResult<()> {
    let mut root = repo_root();
    let mut write_report = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => root = PathBuf::from(require_value(args, &mut index, "--root")?),
            "--write-report" => {
                write_report = Some(PathBuf::from(require_value(args, &mut index, "--write-report")?));
            }
            flag => return Err(format!("unexpected cross-check-rpc-idl argument: {flag}")),
        }
        index += 1;
    }
    idl::run_cross_check(&root, write_report.as_deref())
}

fn bundle(args: &[String]) -> CheckResult<()> {
    let root = repo_root();
    let mut maps = Vec::new();
    let mut out = None;
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--map" => maps.push(PathBuf::from(require_value(args, &mut index, "--map")?)),
            "--out" => out = Some(PathBuf::from(require_value(args, &mut index, "--out")?)),
            "--check" => check = true,
            flag => return Err(format!("unexpected rpc-contract-bundle argument: {flag}")),
        }
        index += 1;
    }
    let out = out.map(|path| if path.is_absolute() { path } else { root.join(path) });
    bundle::run_bundle(&root, &maps, out.as_deref(), check)
}

fn all(root: &Path) -> CheckResult<()> {
    routes::run_generate_routes(root, &[], &root.join("generated"), true)?;
    authority::run_validate(root, None)?;
    idl::run_cross_check(root, None)?;
    idl::run_audit(root)?;
    bundle::run_bundle(root, &[], None, true)?;
    lint::run_lint(root)?;
    Ok(())
}

fn run() -> CheckResult<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let Some((command, rest)) = args.split_first() else {
        return Err(usage());
    };
    match command.as_str() {
        "all" if rest.is_empty() => all(&repo_root()),
        "generate-routes" => generate_routes(rest),
        "validate-authority-contract" => validate_authority(rest),
        "compare-authority-artifacts" => compare_authorities(rest),
        "cross-check-rpc-idl" => cross_check(rest),
        "audit-rpc-idl" => idl::run_audit(&root_flag(rest)?),
        "rpc-contract-bundle" => bundle(rest),
        "lint" => lint::run_lint(&root_flag(rest)?),
        "--help" | "-h" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(format!("unknown command: {command}\n\n{}", usage())),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
