//! Deterministic authored-source and route-metadata manifest for browser pages.
//!
//! This is not the finalized deployment-artifact manifest. CSS/WASM output
//! hashes remain in [`crate::PageBuildManifest`]. This manifest binds authored
//! render sources (`page.rs` + inherited `layout.rs`), optional static-parameter
//! generation source (`gen.rs`), and normalized route/docs metadata.

use crate::{page_layout::page_render_source_inputs, project::sha256_hex, PageBuildManifest};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

pub const WEB_PAGE_MANIFEST_SCHEMA: &str = "ores.web.page-manifest/v1";
pub const WEB_PAGE_MANIFEST_GENERATOR: &str = "ores-api-docs";
pub const MAX_PAGE_MANIFEST_REVALIDATE_SECS: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebPageSourceDigest {
    pub source: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebPageManifestEntry {
    pub canonical_url: String,
    pub source_page_rs: String,
    pub page_source_sha256: String,
    pub layout_sources: Vec<WebPageSourceDigest>,
    pub render_sha256: String,
    pub generator_source: Option<WebPageSourceDigest>,
    pub axum_paths: Vec<String>,
    pub renderer: String,
    pub delivery: String,
    pub render_mode: String,
    pub revalidate_secs: Option<u64>,
    pub on_demand: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub auth: String,
    pub stability: String,
    pub database: String,
    pub features: Vec<String>,
    pub data_sources: Vec<String>,
    pub tags: Vec<String>,
    pub rpc_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebPageManifest {
    pub schema: String,
    pub generated_by: String,
    pub generator_version: String,
    pub route_root: String,
    pub manifest_sha256: String,
    pub pages: Vec<WebPageManifestEntry>,
}

#[derive(Debug, Error)]
pub enum WebPageManifestError {
    #[error("page manifest route root must be src/pages, got {actual:?}")]
    RouteRoot { actual: String },
    #[error("page render-source admission failed for {source}: {message}")]
    RenderSource { source: String, message: String },
    #[error("page generator-source admission failed for {source}: {message}")]
    GeneratorSource { source: String, message: String },
    #[error("page {source} has revalidate_secs={value}, outside the exact JSON safe-integer range 1..={max}")]
    RevalidateRange {
        source: String,
        value: u64,
        max: u64,
    },
    #[error("failed to read page manifest source {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize page manifest: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn web_page_manifest(
    repo_root: &Path,
    build: &PageBuildManifest,
) -> Result<WebPageManifest, WebPageManifestError> {
    if build.route_root != "src/pages" {
        return Err(WebPageManifestError::RouteRoot {
            actual: build.route_root.clone(),
        });
    }

    let mut pages = Vec::with_capacity(build.routes.len());
    for route in &build.routes {
        if let Some(value) = route.revalidate_secs {
            if value == 0 || value > MAX_PAGE_MANIFEST_REVALIDATE_SECS {
                return Err(WebPageManifestError::RevalidateRange {
                    source: route.source.clone(),
                    value,
                    max: MAX_PAGE_MANIFEST_REVALIDATE_SECS,
                });
            }
        }

        let render = page_render_source_inputs(repo_root, &route.source).map_err(|message| {
            WebPageManifestError::RenderSource {
                source: route.source.clone(),
                message,
            }
        })?;

        let generator_source = route
            .generator
            .as_deref()
            .map(|source| generator_source_digest(repo_root, &route.source, source))
            .transpose()?;

        let mut rpc_dependencies = route
            .data_sources
            .iter()
            .filter_map(|value| value.strip_prefix("rpc:"))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        rpc_dependencies.sort();
        rpc_dependencies.dedup();

        let mut features = route.features.clone();
        features.sort();
        features.dedup();
        let mut data_sources = route.data_sources.clone();
        data_sources.sort();
        data_sources.dedup();
        let mut tags = route.tags.clone();
        tags.sort();
        tags.dedup();
        let mut axum_paths = route.axum_paths.clone();
        axum_paths.sort();
        axum_paths.dedup();

        pages.push(WebPageManifestEntry {
            canonical_url: route.canonical_path.clone(),
            source_page_rs: route.source.clone(),
            page_source_sha256: render.page.sha256,
            layout_sources: render
                .layouts
                .into_iter()
                .map(|source| WebPageSourceDigest {
                    source: source.source,
                    sha256: source.sha256,
                })
                .collect(),
            render_sha256: render.render_sha256,
            generator_source,
            axum_paths,
            renderer: route.renderer.clone(),
            delivery: route.delivery.clone(),
            render_mode: route.render.clone(),
            revalidate_secs: route.revalidate_secs,
            on_demand: route.on_demand.clone(),
            title: route.title.clone(),
            summary: route.summary.clone(),
            auth: route.auth.clone(),
            stability: route.stability.clone(),
            database: route.database.clone(),
            features,
            data_sources,
            tags,
            rpc_dependencies,
        });
    }

    pages.sort_by(|left, right| {
        left.canonical_url
            .cmp(&right.canonical_url)
            .then_with(|| left.source_page_rs.cmp(&right.source_page_rs))
    });
    let semantic = serde_json::to_vec(&pages)?;
    Ok(WebPageManifest {
        schema: WEB_PAGE_MANIFEST_SCHEMA.to_owned(),
        generated_by: WEB_PAGE_MANIFEST_GENERATOR.to_owned(),
        generator_version: env!("CARGO_PKG_VERSION").to_owned(),
        route_root: build.route_root.clone(),
        manifest_sha256: sha256_hex(&semantic),
        pages,
    })
}

fn generator_source_digest(
    repo_root: &Path,
    page_source: &str,
    generator_source: &str,
) -> Result<WebPageSourceDigest, WebPageManifestError> {
    let expected = Path::new(page_source)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("gen.rs")
        .to_string_lossy()
        .replace('\\', "/");
    if generator_source != expected {
        return Err(WebPageManifestError::GeneratorSource {
            source: generator_source.to_owned(),
            message: format!(
                "generator must be the sibling gen.rs for {page_source}; expected {expected}"
            ),
        });
    }

    let root = fs::canonicalize(repo_root).map_err(|source| WebPageManifestError::Read {
        path: repo_root.display().to_string(),
        source,
    })?;
    let pages = root.join("src/pages");
    let pages = fs::canonicalize(&pages).map_err(|source| WebPageManifestError::Read {
        path: pages.display().to_string(),
        source,
    })?;
    if !pages.starts_with(&root) {
        return Err(WebPageManifestError::GeneratorSource {
            source: generator_source.to_owned(),
            message: "src/pages escapes repository root".to_owned(),
        });
    }

    let target = root.join(generator_source);
    reject_symlink_components(&root, &target, generator_source)?;
    let canonical = fs::canonicalize(&target).map_err(|source| WebPageManifestError::Read {
        path: target.display().to_string(),
        source,
    })?;
    if !canonical.starts_with(&pages) || !canonical.is_file() {
        return Err(WebPageManifestError::GeneratorSource {
            source: generator_source.to_owned(),
            message: "generator must be a regular file under src/pages".to_owned(),
        });
    }
    let bytes = fs::read(&canonical).map_err(|source| WebPageManifestError::Read {
        path: canonical.display().to_string(),
        source,
    })?;
    Ok(WebPageSourceDigest {
        source: generator_source.to_owned(),
        sha256: sha256_hex(&bytes),
    })
}

fn reject_symlink_components(
    root: &Path,
    target: &Path,
    source_name: &str,
) -> Result<(), WebPageManifestError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| WebPageManifestError::GeneratorSource {
            source: source_name.to_owned(),
            message: "generator path escapes repository root".to_owned(),
        })?;
    let mut current = PathBuf::from(root);
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(WebPageManifestError::GeneratorSource {
                source: source_name.to_owned(),
                message: "generator path contains a non-normal component".to_owned(),
            });
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(WebPageManifestError::GeneratorSource {
                    source: source_name.to_owned(),
                    message: format!("generator path traverses symlink {}", current.display()),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(WebPageManifestError::Read {
                    path: current.display().to_string(),
                    source,
                })
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageBuildRoute;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ores-web-page-manifest-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn fixture(root: &Path) -> PageBuildManifest {
        let page_dir = root.join("src/pages/users/[id]");
        fs::create_dir_all(&page_dir).unwrap();
        fs::write(root.join("src/pages/layout.rs"), "// root layout v1\n").unwrap();
        fs::write(page_dir.join("page.rs"), "// page v1\n").unwrap();
        fs::write(page_dir.join("gen.rs"), "// generator v1\n").unwrap();
        PageBuildManifest {
            schema_version: "1.2.0".to_owned(),
            route_root: "src/pages".to_owned(),
            wasm_have_header: "x-ores-wasm-have".to_owned(),
            wasm_have_cookie: "ores_wasm_have".to_owned(),
            routes: vec![PageBuildRoute {
                source: "src/pages/users/[id]/page.rs".to_owned(),
                generator: Some("src/pages/users/[id]/gen.rs".to_owned()),
                canonical_path: "/users/{id}".to_owned(),
                axum_paths: vec!["/users/{id}".to_owned()],
                dioxus_paths: vec!["/users/:id".to_owned()],
                renderer: "mash".to_owned(),
                delivery: "ssr_only".to_owned(),
                render: "static_with_fallback".to_owned(),
                revalidate_secs: Some(60),
                on_demand: None,
                title: Some("User".to_owned()),
                summary: None,
                auth: "session".to_owned(),
                stability: "stable".to_owned(),
                database: "read_only".to_owned(),
                features: vec!["users".to_owned(), "users".to_owned()],
                data_sources: vec![
                    "rpc:demo.users.find".to_owned(),
                    "orm:user_read".to_owned(),
                    "rpc:demo.users.find".to_owned(),
                ],
                tags: vec!["account".to_owned()],
                css: None,
                wasm: None,
            }],
        }
    }

    #[test]
    fn manifest_binds_page_layout_and_generator_sources() {
        let root = temp_root("sources");
        let manifest = web_page_manifest(&root, &fixture(&root)).unwrap();
        let page = &manifest.pages[0];
        assert_eq!(manifest.generated_by, WEB_PAGE_MANIFEST_GENERATOR);
        assert!(!manifest.generator_version.is_empty());
        assert_eq!(page.layout_sources.len(), 1);
        assert_eq!(page.layout_sources[0].source, "src/pages/layout.rs");
        assert_eq!(
            page.generator_source
                .as_ref()
                .map(|source| source.source.as_str()),
            Some("src/pages/users/[id]/gen.rs")
        );
        assert_eq!(page.rpc_dependencies, vec!["demo.users.find"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_noncanonical_route_root() {
        let root = temp_root("route-root");
        let mut build = fixture(&root);
        build.route_root = "src/routes".to_owned();
        assert!(matches!(
            web_page_manifest(&root, &build),
            Err(WebPageManifestError::RouteRoot { .. })
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_generator_path_that_is_not_sibling_gen_rs() {
        let root = temp_root("generator-path");
        let mut build = fixture(&root);
        fs::write(root.join("outside.rs"), "// outside\n").unwrap();
        build.routes[0].generator = Some("outside.rs".to_owned());
        assert!(matches!(
            web_page_manifest(&root, &build),
            Err(WebPageManifestError::GeneratorSource { .. })
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_sibling_generator() {
        use std::os::unix::fs::symlink;
        let root = temp_root("generator-symlink");
        let mut build = fixture(&root);
        let generator = root.join("src/pages/users/[id]/gen.rs");
        fs::remove_file(&generator).unwrap();
        fs::write(root.join("outside.rs"), "// outside\n").unwrap();
        symlink(root.join("outside.rs"), &generator).unwrap();
        assert!(matches!(
            web_page_manifest(&root, &build),
            Err(WebPageManifestError::GeneratorSource { .. })
        ));
        build.routes[0].generator = None;
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn layout_edit_changes_render_and_manifest_digests() {
        let root = temp_root("layout-drift");
        let build = fixture(&root);
        let before = web_page_manifest(&root, &build).unwrap();
        let page_sha = before.pages[0].page_source_sha256.clone();
        fs::write(root.join("src/pages/layout.rs"), "// root layout v2\n").unwrap();
        let after = web_page_manifest(&root, &build).unwrap();
        assert_eq!(page_sha, after.pages[0].page_source_sha256);
        assert_ne!(before.pages[0].render_sha256, after.pages[0].render_sha256);
        assert_ne!(before.manifest_sha256, after.manifest_sha256);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generator_edit_changes_manifest_without_changing_render_digest() {
        let root = temp_root("gen-drift");
        let build = fixture(&root);
        let before = web_page_manifest(&root, &build).unwrap();
        fs::write(
            root.join("src/pages/users/[id]/gen.rs"),
            "// generator v2\n",
        )
        .unwrap();
        let after = web_page_manifest(&root, &build).unwrap();
        assert_eq!(before.pages[0].render_sha256, after.pages[0].render_sha256);
        assert_ne!(
            before.pages[0].generator_source,
            after.pages[0].generator_source
        );
        assert_ne!(before.manifest_sha256, after.manifest_sha256);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalized_collections_make_input_order_irrelevant() {
        let root = temp_root("normalization");
        let mut build = fixture(&root);
        let first = web_page_manifest(&root, &build).unwrap();
        build.routes[0].features.reverse();
        build.routes[0].data_sources.reverse();
        build.routes[0].tags.reverse();
        build.routes[0].axum_paths.reverse();
        let second = web_page_manifest(&root, &build).unwrap();
        assert_eq!(first, second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn revalidate_seconds_must_round_trip_exactly_through_json_tooling() {
        let root = temp_root("revalidate-range");
        let mut build = fixture(&root);
        for admitted in [1, MAX_PAGE_MANIFEST_REVALIDATE_SECS] {
            build.routes[0].revalidate_secs = Some(admitted);
            assert!(web_page_manifest(&root, &build).is_ok());
        }

        for rejected in [0, MAX_PAGE_MANIFEST_REVALIDATE_SECS + 1] {
            build.routes[0].revalidate_secs = Some(rejected);
            assert!(matches!(
                web_page_manifest(&root, &build),
                Err(WebPageManifestError::RevalidateRange { .. })
            ));
        }
        let _ = fs::remove_dir_all(root);
    }
}
