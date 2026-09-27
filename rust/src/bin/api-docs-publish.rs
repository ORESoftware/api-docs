//! Materialize one deterministic API/MCP publication bundle from a route map.

#![allow(clippy::needless_return)]

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use ores_api_docs::{render_docs_publication, Catalog, PublicationMode, RouteMap};
use serde_json::json;

#[path = "../publication_fs.rs"]
mod publication_fs;

use publication_fs::materialize_publication_files;

struct Args {
    route_map: PathBuf,
    out_dir: PathBuf,
    mode: PublicationMode,
    producer: String,
}

enum ParseOutcome {
    Run(Args),
    Help,
}

fn main() -> ExitCode {
    let outcome = match parse_args_from(env::args().skip(1)) {
        Ok(outcome) => outcome,
        Err(message) => {
            eprintln!("api-docs-publish: {message}\n\n{}", usage());
            return ExitCode::FAILURE;
        }
    };

    match outcome {
        ParseOutcome::Help => {
            println!("{}", usage());
            return ExitCode::SUCCESS;
        }
        ParseOutcome::Run(args) => match run(args) {
            Ok(()) => {
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                eprintln!("api-docs-publish: {error}");
                return ExitCode::FAILURE;
            }
        },
    }
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let route_map_json = std::fs::read_to_string(&args.route_map)?;
    let route_map = RouteMap::from_json_str(&route_map_json)?;
    let catalog = Catalog::from_map_with_language(route_map, None)?;
    let bundle = render_docs_publication(&catalog, args.mode, &args.producer)?;
    materialize_publication_files(&bundle.files, &args.out_dir)?;

    let report = json!({
        "schema_version": "ores.api-docs.publish-report.v1",
        "service": bundle.manifest.service,
        "publication_mode": bundle.manifest.publication_mode.as_str(),
        "producer": bundle.manifest.producer,
        "authority_scope": bundle.manifest.authority_scope,
        "publisher_provenance_required": bundle.manifest.publisher_provenance_required,
        "contract_sha256": bundle.manifest.contract_sha256,
        "route_count": bundle.manifest.route_count,
        "artifact_count": bundle.files.len(),
        "out_dir": args.out_dir.display().to_string(),
    });
    println!("{}", serde_json::to_string(&report)?);

    return Ok(());
}

fn parse_args_from<I>(args: I) -> Result<ParseOutcome, String>
where
    I: IntoIterator<Item = String>,
{
    let mut route_map = None;
    let mut out_dir = None;
    let mut mode = None;
    let mut producer = None;
    let mut args = args.into_iter();

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--route-map" => {
                let value = PathBuf::from(next_value(&mut args, "--route-map")?);
                set_once(&mut route_map, value, "--route-map")?;
            }
            "--out-dir" => {
                let value = PathBuf::from(next_value(&mut args, "--out-dir")?);
                set_once(&mut out_dir, value, "--out-dir")?;
            }
            "--mode" => {
                let value = next_value(&mut args, "--mode")?;
                set_once(&mut mode, parse_mode(&value)?, "--mode")?;
            }
            "--producer" => {
                let value = next_value(&mut args, "--producer")?;
                set_once(&mut producer, value, "--producer")?;
            }
            "--help" | "-h" => {
                return Ok(ParseOutcome::Help);
            }
            _ => {
                return Err(format!("unknown argument: {flag}"));
            }
        }
    }

    let Some(route_map) = route_map else {
        return Err("missing --route-map".to_owned());
    };
    let Some(out_dir) = out_dir else {
        return Err("missing --out-dir".to_owned());
    };
    let Some(mode) = mode else {
        return Err("missing --mode".to_owned());
    };
    let Some(producer) = producer else {
        return Err("missing --producer".to_owned());
    };

    return Ok(ParseOutcome::Run(Args {
        route_map,
        out_dir,
        mode,
        producer,
    }));
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("duplicate argument: {flag}"));
    }
    *slot = Some(value);
    return Ok(());
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    let Some(value) = args.next() else {
        return Err(format!("missing value for {flag}"));
    };
    if value.starts_with('-') {
        return Err(format!("missing value for {flag}"));
    }

    return Ok(value);
}

fn parse_mode(value: &str) -> Result<PublicationMode, String> {
    match value {
        "publisher_external" => {
            return Ok(PublicationMode::PublisherExternal);
        }
        "consumer_project" => {
            return Ok(PublicationMode::ConsumerProject);
        }
        _ => {
            return Err(format!(
                "invalid --mode {value:?}; expected publisher_external or consumer_project"
            ));
        }
    }
}

const fn usage() -> &'static str {
    return "Usage: api-docs-publish --route-map PATH --out-dir PATH --mode publisher_external|consumer_project --producer ID";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> impl Iterator<Item = String> + '_ {
        return values.iter().map(|value| (*value).to_owned());
    }

    #[test]
    fn modes_are_explicit_and_closed() {
        assert!(matches!(
            parse_mode("publisher_external"),
            Ok(PublicationMode::PublisherExternal)
        ));
        assert!(matches!(
            parse_mode("consumer_project"),
            Ok(PublicationMode::ConsumerProject)
        ));
        assert!(parse_mode("official").is_err());
    }

    #[test]
    fn help_is_a_successful_parse_outcome() {
        assert!(matches!(
            parse_args_from(strings(&["--help"])),
            Ok(ParseOutcome::Help)
        ));
        assert!(matches!(
            parse_args_from(strings(&["-h"])),
            Ok(ParseOutcome::Help)
        ));
    }

    #[test]
    fn complete_arguments_parse_once() {
        let parsed = parse_args_from(strings(&[
            "--route-map",
            "contracts/api.route-map.json",
            "--out-dir",
            "generated/docs",
            "--mode",
            "publisher_external",
            "--producer",
            "fiducia-cloud",
        ]));
        assert!(matches!(parsed, Ok(ParseOutcome::Run(_))));
    }

    #[test]
    fn duplicate_identity_or_path_arguments_fail_closed() {
        let duplicate_route_map = parse_args_from(strings(&[
            "--route-map",
            "a.json",
            "--route-map",
            "b.json",
            "--out-dir",
            "generated/docs",
            "--mode",
            "publisher_external",
            "--producer",
            "fiducia-cloud",
        ]));
        assert!(
            matches!(duplicate_route_map, Err(message) if message == "duplicate argument: --route-map")
        );

        let duplicate_producer = parse_args_from(strings(&[
            "--route-map",
            "a.json",
            "--out-dir",
            "generated/docs",
            "--mode",
            "publisher_external",
            "--producer",
            "fiducia-cloud",
            "--producer",
            "other",
        ]));
        assert!(
            matches!(duplicate_producer, Err(message) if message == "duplicate argument: --producer")
        );
    }

    #[test]
    fn required_arguments_remain_required() {
        assert!(matches!(
            parse_args_from(strings(&["--route-map", "a.json"])),
            Err(message) if message == "missing --out-dir"
        ));
    }
}
