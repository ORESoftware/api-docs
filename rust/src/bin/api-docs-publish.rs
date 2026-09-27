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

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("api-docs-publish: {error}");
            return ExitCode::FAILURE;
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = parse_args().map_err(|message| format!("{message}\n\n{}", usage()))?;
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

fn parse_args() -> Result<Args, String> {
    let mut route_map = None;
    let mut out_dir = None;
    let mut mode = None;
    let mut producer = None;
    let mut args = env::args().skip(1);

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--route-map" => {
                route_map = Some(PathBuf::from(next_value(&mut args, "--route-map")?));
            }
            "--out-dir" => {
                out_dir = Some(PathBuf::from(next_value(&mut args, "--out-dir")?));
            }
            "--mode" => {
                let value = next_value(&mut args, "--mode")?;
                mode = Some(parse_mode(&value)?);
            }
            "--producer" => {
                producer = Some(next_value(&mut args, "--producer")?);
            }
            "--help" | "-h" => {
                return Err(usage().to_owned());
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

    return Ok(Args {
        route_map,
        out_dir,
        mode,
        producer,
    });
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
}
