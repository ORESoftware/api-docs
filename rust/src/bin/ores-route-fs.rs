use ores_api_docs::{validate_and_sort_fs_routes, FsRoute, FsRouteKind, RouteMap};
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
struct Manifest {
    schema_version: &'static str,
    surface: &'static str,
    route_root: &'static str,
    routes: Vec<Entry>,
}

#[derive(Serialize)]
struct Entry {
    source: String,
    canonical_path: String,
    axum_paths: Vec<String>,
    dioxus_paths: Vec<String>,
    api_operations: Vec<String>,
}

fn usage() -> ! {
    eprintln!("usage: ores-route-fs <page|api> [repo-root] [route-map.json]");
    std::process::exit(2);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| usage());
    let repo_root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_owned()));
    let route_map_path = args.next().map(PathBuf::from);
    if args.next().is_some() {
        usage();
    }

    let (kind, route_root, leaf, surface) = match mode.as_str() {
        "page" => (FsRouteKind::Page, "src/pages", "page.rs", "web"),
        "api" => (FsRouteKind::ApiHandler, "src/routes", "route.rs", "api"),
        _ => usage(),
    };

    let root = repo_root.join(route_root);
    let mut sources = Vec::new();
    collect(&repo_root, &root, leaf, &mut sources)?;
    let parsed = sources
        .into_iter()
        .map(|source| match kind {
            FsRouteKind::Page => FsRoute::page(source),
            FsRouteKind::ApiHandler => FsRoute::api_handler(source),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let routes = validate_and_sort_fs_routes(parsed)?;

    let route_map = match route_map_path {
        Some(path) => Some(RouteMap::from_json_str(&fs::read_to_string(path)?)?),
        None => None,
    };

    if kind == FsRouteKind::ApiHandler && route_map.is_none() {
        return Err("api mode requires a route-map.json argument so filesystem paths cannot become a second API authority".into());
    }

    let entries = routes
        .into_iter()
        .map(|route| {
            let canonical_path = route.canonical_path();
            let api_operations = route_map
                .as_ref()
                .map(|map| {
                    map.map
                        .iter()
                        .filter(|(_, entry)| entry.path == canonical_path)
                        .map(|(key, _)| key.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if kind == FsRouteKind::ApiHandler && api_operations.is_empty() {
                return Err(format!(
                    "{} derives {} but no api-docs route-map operation owns that path",
                    route.source, canonical_path
                ));
            }
            Ok(Entry {
                source: route.source.clone(),
                canonical_path,
                axum_paths: route.axum_paths(),
                dioxus_paths: route.dioxus_paths(),
                api_operations,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let manifest = Manifest {
        schema_version: "1.0.0",
        surface,
        route_root,
        routes: entries,
    };
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

fn collect(
    repo_root: &Path,
    dir: &Path,
    leaf: &str,
    out: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(repo_root, &path, leaf, out)?;
        } else if path.file_name().and_then(|value| value.to_str()) == Some(leaf) {
            let relative = path.strip_prefix(repo_root)?;
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}
