//! Deterministic build planning for Rust filesystem pages.
//!
//! This layer performs only static work: filesystem discovery, Rust syntax
//! analysis, route validation, content hashing, route-local CSS bundling, and
//! WASM build planning. It never executes application page functions. HTML
//! prerendering is a separate post-compile step that consumes this manifest.

use crate::{
    analyze_generator_source, analyze_page_source, page_compile_glue,
    project::sha256_hex, validate_and_sort_fs_routes, FsRoute,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const WASM_HAVE_HEADER: &str = "x-ores-wasm-have";
pub const WASM_HAVE_COOKIE: &str = "ores_wasm_have";

#[derive(Debug, Error)]
pub enum PageBuildError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("route: {0}")]
    Route(String),
    #[error("module: {0}")]
    Module(String),
    #[error("asset: {0}")]
    Asset(String),
    #[error("manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct PageBuildManifest {
    pub schema_version: &'static str,
    pub route_root: &'static str,
    pub wasm_have_header: &'static str,
    pub wasm_have_cookie: &'static str,
    pub routes: Vec<PageBuildRoute>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageBuildRoute {
    pub source: String,
    pub generator: Option<String>,
    pub canonical_path: String,
    pub axum_paths: Vec<String>,
    pub dioxus_paths: Vec<String>,
    pub renderer: String,
    pub delivery: String,
    pub render: String,
    pub revalidate_secs: Option<u64>,
    pub on_demand: Option<String>,
    pub css: Option<ContentAsset>,
    pub wasm: Option<WasmBuildPlan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentAsset {
    pub source: String,
    pub sha256: String,
    pub output_file: String,
    pub public_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WasmBuildPlan {
    pub source: String,
    pub source_sha256: String,
    pub final_wasm_sha256: Option<String>,
    pub public_path: Option<String>,
    pub immutable_cache: bool,
}

#[derive(Debug, Clone)]
pub struct PageBuildOutputs {
    pub manifest_path: PathBuf,
    pub compile_glue_path: PathBuf,
    pub rerun_if_changed: Vec<PathBuf>,
}

pub fn write_page_build_outputs(
    repo_root: &Path,
    out_dir: &Path,
) -> Result<PageBuildOutputs, PageBuildError> {
    let repo_root = repo_root.canonicalize()?;
    fs::create_dir_all(out_dir)?;
    let assets_out = out_dir.join("page-assets");
    fs::create_dir_all(&assets_out)?;

    let mut sources = Vec::new();
    collect_pages(&repo_root, &repo_root.join("src/pages"), &mut sources)?;
    let parsed = sources
        .iter()
        .map(|source| FsRoute::page(source.clone()).map_err(|error| PageBuildError::Route(error.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    let routes = validate_and_sort_fs_routes(parsed)
        .map_err(|error| PageBuildError::Route(error.to_string()))?;

    let mut manifest_routes = Vec::with_capacity(routes.len());
    let mut rerun_if_changed = vec![repo_root.join("src/pages")];
    for route in &routes {
        let page_path = repo_root.join(&route.source);
        rerun_if_changed.push(page_path.clone());
        let source = fs::read_to_string(&page_path)?;
        let analysis = analyze_page_source(&route.source, &source)
            .map_err(|error| PageBuildError::Module(error.to_string()))?;
        let metadata = analysis
            .page
            .ok_or_else(|| PageBuildError::Module(format!("{} missing page metadata", route.source)))?;
        let page_dir = page_path.parent().ok_or_else(|| {
            PageBuildError::Route(format!("{} has no parent directory", route.source))
        })?;

        let generator_path = page_dir.join("gen.rs");
        let generator = if generator_path.is_file() {
            let relative = relative_string(&repo_root, &generator_path)?;
            let source = fs::read_to_string(&generator_path)?;
            analyze_generator_source(&relative, &source)
                .map_err(|error| PageBuildError::Module(error.to_string()))?;
            rerun_if_changed.push(generator_path.clone());
            Some(relative)
        } else {
            None
        };
        let dynamic = route.segments.iter().any(|segment| {
            !matches!(segment, crate::FsRouteSegment::Static(_))
        });
        if dynamic && metadata.render == "static_only" && generator.is_none() {
            return Err(PageBuildError::Module(format!(
                "{} is dynamic + static_only and requires sibling gen.rs",
                route.source
            )));
        }

        let css = bundle_local_css(&repo_root, page_dir, &assets_out, &mut rerun_if_changed)?;
        let wasm = match metadata.client.as_deref() {
            Some(client) => {
                let client_path = page_dir.join(client);
                if !client_path.is_file() {
                    return Err(PageBuildError::Asset(format!(
                        "{} declares client={client:?}, but {} does not exist",
                        route.source,
                        client_path.display()
                    )));
                }
                rerun_if_changed.push(client_path.clone());
                let bytes = fs::read(&client_path)?;
                Some(WasmBuildPlan {
                    source: relative_string(&repo_root, &client_path)?,
                    source_sha256: sha256_hex(&bytes),
                    final_wasm_sha256: None,
                    public_path: None,
                    immutable_cache: true,
                })
            }
            None => None,
        };

        manifest_routes.push(PageBuildRoute {
            source: route.source.clone(),
            generator,
            canonical_path: route.canonical_path(),
            axum_paths: route.axum_paths(),
            dioxus_paths: route.dioxus_paths(),
            renderer: metadata.renderer,
            delivery: metadata.delivery,
            render: metadata.render,
            revalidate_secs: metadata.revalidate_secs,
            on_demand: metadata.on_demand,
            css,
            wasm,
        });
    }

    let manifest = PageBuildManifest {
        schema_version: "1.0.0",
        route_root: "src/pages",
        wasm_have_header: WASM_HAVE_HEADER,
        wasm_have_cookie: WASM_HAVE_COOKIE,
        routes: manifest_routes,
    };
    let manifest_path = out_dir.join("ores-page-manifest.json");
    let json = serde_json::to_vec_pretty(&manifest)?;
    fs::write(&manifest_path, json)?;

    let compile_glue_path = out_dir.join("ores_pages_compile.rs");
    fs::write(&compile_glue_path, page_compile_glue(&repo_root, &routes).map_err(PageBuildError::Route)?)?;

    rerun_if_changed.sort();
    rerun_if_changed.dedup();
    Ok(PageBuildOutputs {
        manifest_path,
        compile_glue_path,
        rerun_if_changed,
    })
}

fn collect_pages(
    repo_root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
) -> Result<(), PageBuildError> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_pages(repo_root, &path, out)?;
        } else if path.file_name().and_then(|value| value.to_str()) == Some("page.rs") {
            out.push(relative_string(repo_root, &path)?);
        }
    }
    Ok(())
}

fn bundle_local_css(
    repo_root: &Path,
    page_dir: &Path,
    assets_out: &Path,
    rerun_if_changed: &mut Vec<PathBuf>,
) -> Result<Option<ContentAsset>, PageBuildError> {
    let css_path = page_dir.join("style.css");
    if !css_path.is_file() {
        return Ok(None);
    }
    rerun_if_changed.push(css_path.clone());
    let bytes = fs::read(&css_path)?;
    let sha256 = sha256_hex(&bytes);
    let output_file = format!("page-{sha256}.css");
    fs::write(assets_out.join(&output_file), &bytes)?;
    Ok(Some(ContentAsset {
        source: relative_string(repo_root, &css_path)?,
        sha256: sha256.clone(),
        public_path: format!("/__ores/assets/{output_file}"),
        output_file,
    }))
}

fn relative_string(repo_root: &Path, path: &Path) -> Result<String, PageBuildError> {
    let relative = path.strip_prefix(repo_root).map_err(|error| {
        PageBuildError::Route(format!("{} is outside {}: {error}", path.display(), repo_root.display()))
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_wasm_hint_is_not_a_route_query_parameter() {
        assert_eq!(WASM_HAVE_HEADER, "x-ores-wasm-have");
        assert!(!WASM_HAVE_HEADER.contains('?'));
    }
}
