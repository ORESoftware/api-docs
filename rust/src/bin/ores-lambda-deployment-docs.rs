use std::env;
use std::fs;
use std::process::ExitCode;

#[path = "../lambda_deployment_docs.rs"]
mod lambda_deployment_docs;

use lambda_deployment_docs::LambdaDeploymentDocsManifest;
use ores_api_docs::{contract_sha256, RouteMap};

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ores-lambda-deployment-docs: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<String, String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if !(args.len() == 2 || args.len() == 3) {
        return Err(
            "usage: ores-lambda-deployment-docs <route-map.json> <deployment-manifest.json> [json|markdown]"
                .to_owned(),
        );
    }

    let format = args.get(2).map(String::as_str).unwrap_or("markdown");
    if !matches!(format, "json" | "markdown") {
        return Err(format!("unsupported output format {format:?}; expected json or markdown"));
    }

    let route_map_text = fs::read_to_string(&args[0])
        .map_err(|error| format!("cannot read route map {:?}: {error}", args[0]))?;
    let route_map = RouteMap::from_json_str(&route_map_text)
        .map_err(|error| format!("invalid route map {:?}: {error}", args[0]))?;

    let deployment_text = fs::read_to_string(&args[1])
        .map_err(|error| format!("cannot read deployment manifest {:?}: {error}", args[1]))?;
    let manifest = LambdaDeploymentDocsManifest::parse_json(&deployment_text)
        .map_err(|error| format!("invalid deployment manifest {:?}: {error}", args[1]))?;

    if manifest.service != route_map.service {
        return Err(format!(
            "service mismatch: route map has {:?}, deployment manifest has {:?}",
            route_map.service, manifest.service
        ));
    }

    let route_digest = contract_sha256(&route_map);
    if manifest.contract_sha256 != route_digest {
        return Err(format!(
            "contract digest mismatch: route map is {route_digest}, deployment manifest declares {}",
            manifest.contract_sha256
        ));
    }

    match format {
        "json" => manifest.to_pretty_json().map(|value| format!("{value}\n")),
        "markdown" => manifest.to_markdown(),
        _ => unreachable!("validated above"),
    }
    .map_err(|error| error.to_string())
}
