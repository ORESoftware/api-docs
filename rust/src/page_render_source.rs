use super::page_segment_sources;
use crate::project::sha256_hex;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// One authored Rust source file that contributes directly to browser-page
/// rendering. Paths are repository-relative and use `/` separators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageRenderSource {
    pub source: String,
    pub sha256: String,
}

/// Deterministic authored render-source identity for one browser page.
///
/// Segment convention vectors are each root-to-leaf. The render digest binds
/// `page.rs` plus every inherited `layout.rs`, `template.rs`, `error.rs`,
/// `loading.rs`, and `not_found.rs` source/digest pair. A change to any of those
/// authored files therefore invalidates the page/Lambda render identity even
/// when `page.rs` itself is byte-identical.
///
/// This intentionally does **not** claim to cover sibling `gen.rs`, CSS, or
/// client/WASM assets. Those inputs affect static enumeration or deployment
/// artifacts and must be bound separately by higher-level manifests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageRenderSourceInputs {
    pub page: PageRenderSource,
    pub layouts: Vec<PageRenderSource>,
    #[serde(default)]
    pub templates: Vec<PageRenderSource>,
    #[serde(default)]
    pub errors: Vec<PageRenderSource>,
    #[serde(default)]
    pub loadings: Vec<PageRenderSource>,
    #[serde(default)]
    pub not_found: Vec<PageRenderSource>,
    pub render_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageRenderDigestPayload<'a> {
    page: &'a PageRenderSource,
    layouts: &'a [PageRenderSource],
    templates: &'a [PageRenderSource],
    errors: &'a [PageRenderSource],
    loadings: &'a [PageRenderSource],
    not_found: &'a [PageRenderSource],
}

/// Resolve and hash the complete authored Rust source set that directly wraps
/// one page's rendering.
///
/// Segment discovery is delegated to the canonical [`page_segment_sources`]
/// implementation. This function does not introduce another filesystem grammar
/// or independently infer route ancestry.
pub fn page_render_source_inputs(
    repo_root: &Path,
    page_source: &str,
) -> Result<PageRenderSourceInputs, String> {
    let segments = page_segment_sources(repo_root, page_source)?;
    let page = render_source(repo_root, page_source)?;
    let layouts = segments
        .iter()
        .filter_map(|segment| segment.layout.as_deref())
        .map(|source| render_source(repo_root, source))
        .collect::<Result<Vec<_>, _>>()?;
    let templates = segments
        .iter()
        .filter_map(|segment| segment.template.as_deref())
        .map(|source| render_source(repo_root, source))
        .collect::<Result<Vec<_>, _>>()?;
    let errors = segments
        .iter()
        .filter_map(|segment| segment.error.as_deref())
        .map(|source| render_source(repo_root, source))
        .collect::<Result<Vec<_>, _>>()?;
    let loadings = segments
        .iter()
        .filter_map(|segment| segment.loading.as_deref())
        .map(|source| render_source(repo_root, source))
        .collect::<Result<Vec<_>, _>>()?;
    let not_found = segments
        .iter()
        .filter_map(|segment| segment.not_found.as_deref())
        .map(|source| render_source(repo_root, source))
        .collect::<Result<Vec<_>, _>>()?;

    let payload = PageRenderDigestPayload {
        page: &page,
        layouts: &layouts,
        templates: &templates,
        errors: &errors,
        loadings: &loadings,
        not_found: &not_found,
    };
    let canonical = serde_json::to_vec(&payload)
        .map_err(|error| format!("serialize page render-source inputs: {error}"))?;
    let render_sha256 = sha256_hex(&canonical);

    Ok(PageRenderSourceInputs {
        page,
        layouts,
        templates,
        errors,
        loadings,
        not_found,
        render_sha256,
    })
}

fn render_source(repo_root: &Path, source: &str) -> Result<PageRenderSource, String> {
    let root = fs::canonicalize(repo_root).map_err(|error| {
        format!(
            "canonicalize repository root {}: {error}",
            repo_root.display()
        )
    })?;
    let path = root.join(source);
    let bytes = fs::read(&path)
        .map_err(|error| format!("read page render source {}: {error}", path.display()))?;
    Ok(PageRenderSource {
        source: source.replace('\\', "/"),
        sha256: sha256_hex(&bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ores-page-render-source-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn binds_page_and_root_to_leaf_segment_chain() {
        let root = temp_root("chain");
        fs::create_dir_all(root.join("src/pages/orgs/[org_id]/settings")).unwrap();
        fs::write(root.join("src/pages/layout.rs"), "// root layout\n").unwrap();
        fs::write(root.join("src/pages/error.rs"), "// root error\n").unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/layout.rs"),
            "// org layout\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/template.rs"),
            "// org template\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/settings/loading.rs"),
            "// loading\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/settings/not_found.rs"),
            "// not found\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/settings/page.rs"),
            "// page\n",
        )
        .unwrap();

        let inputs =
            page_render_source_inputs(&root, "src/pages/orgs/[org_id]/settings/page.rs").unwrap();
        assert_eq!(
            inputs
                .layouts
                .iter()
                .map(|item| item.source.as_str())
                .collect::<Vec<_>>(),
            vec!["src/pages/layout.rs", "src/pages/orgs/[org_id]/layout.rs"]
        );
        assert_eq!(
            inputs.templates[0].source,
            "src/pages/orgs/[org_id]/template.rs"
        );
        assert_eq!(inputs.errors[0].source, "src/pages/error.rs");
        assert_eq!(
            inputs.loadings[0].source,
            "src/pages/orgs/[org_id]/settings/loading.rs"
        );
        assert_eq!(
            inputs.not_found[0].source,
            "src/pages/orgs/[org_id]/settings/not_found.rs"
        );
        assert_eq!(
            inputs.page.source,
            "src/pages/orgs/[org_id]/settings/page.rs"
        );
        assert_eq!(inputs.render_sha256.len(), 64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inherited_boundary_edit_changes_render_identity_without_page_edit() {
        let root = temp_root("boundary-drift");
        fs::create_dir_all(root.join("src/pages/a")).unwrap();
        fs::write(root.join("src/pages/template.rs"), "// template v1\n").unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// stable page\n").unwrap();

        let before = page_render_source_inputs(&root, "src/pages/a/page.rs").unwrap();
        fs::write(root.join("src/pages/template.rs"), "// template v2\n").unwrap();
        let after = page_render_source_inputs(&root, "src/pages/a/page.rs").unwrap();

        assert_eq!(before.page.sha256, after.page.sha256);
        assert_ne!(before.templates[0].sha256, after.templates[0].sha256);
        assert_ne!(before.render_sha256, after.render_sha256);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn page_edit_changes_render_identity() {
        let root = temp_root("page-drift");
        fs::create_dir_all(root.join("src/pages/a")).unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// page v1\n").unwrap();
        let before = page_render_source_inputs(&root, "src/pages/a/page.rs").unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// page v2\n").unwrap();
        let after = page_render_source_inputs(&root, "src/pages/a/page.rs").unwrap();
        assert_ne!(before.page.sha256, after.page.sha256);
        assert_ne!(before.render_sha256, after.render_sha256);
        let _ = fs::remove_dir_all(root);
    }
}
