use crate::{page_layout_sources, project::sha256_hex};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// One authored Rust source file that contributes to browser-page rendering
/// semantics. Paths are repository-relative and use `/` separators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSemanticSource {
    pub source: String,
    pub sha256: String,
}

/// Deterministic source identity for one browser page.
///
/// `layouts` is ordered root-to-leaf, matching [`page_layout_sources`]. The
/// semantic digest binds both the page source and every inherited `layout.rs`
/// source/digest pair, so changing an inherited layout necessarily changes the
/// page semantic identity even when `page.rs` itself is byte-identical.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSemanticInputs {
    pub page: PageSemanticSource,
    pub layouts: Vec<PageSemanticSource>,
    pub semantic_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageSemanticDigestPayload<'a> {
    page: &'a PageSemanticSource,
    layouts: &'a [PageSemanticSource],
}

/// Resolve and hash the complete authored source set that controls one page's
/// rendered semantics.
///
/// Layout discovery is delegated to the canonical [`page_layout_sources`]
/// implementation. This function does not introduce another filesystem grammar
/// or independently infer route ancestry.
pub fn page_semantic_inputs(
    repo_root: &Path,
    page_source: &str,
) -> Result<PageSemanticInputs, String> {
    let layouts = page_layout_sources(repo_root, page_source)?;
    let page = semantic_source(repo_root, page_source)?;
    let layouts = layouts
        .into_iter()
        .map(|source| semantic_source(repo_root, &source))
        .collect::<Result<Vec<_>, _>>()?;

    let payload = PageSemanticDigestPayload {
        page: &page,
        layouts: &layouts,
    };
    let canonical = serde_json::to_vec(&payload)
        .map_err(|error| format!("serialize page semantic inputs: {error}"))?;
    let semantic_sha256 = sha256_hex(&canonical);

    Ok(PageSemanticInputs {
        page,
        layouts,
        semantic_sha256,
    })
}

fn semantic_source(repo_root: &Path, source: &str) -> Result<PageSemanticSource, String> {
    let root = fs::canonicalize(repo_root)
        .map_err(|error| format!("canonicalize repository root {}: {error}", repo_root.display()))?;
    let path = root.join(source);
    let bytes = fs::read(&path)
        .map_err(|error| format!("read semantic source {}: {error}", path.display()))?;
    Ok(PageSemanticSource {
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
            "ores-page-semantic-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn binds_page_and_root_to_leaf_layout_chain() {
        let root = temp_root("chain");
        fs::create_dir_all(root.join("src/pages/orgs/[org_id]/settings")).unwrap();
        fs::write(root.join("src/pages/layout.rs"), "// root layout\n").unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/layout.rs"),
            "// org layout\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/settings/page.rs"),
            "// page\n",
        )
        .unwrap();

        let inputs = page_semantic_inputs(
            &root,
            "src/pages/orgs/[org_id]/settings/page.rs",
        )
        .unwrap();
        assert_eq!(
            inputs
                .layouts
                .iter()
                .map(|item| item.source.as_str())
                .collect::<Vec<_>>(),
            vec!["src/pages/layout.rs", "src/pages/orgs/[org_id]/layout.rs"]
        );
        assert_eq!(inputs.page.source, "src/pages/orgs/[org_id]/settings/page.rs");
        assert_eq!(inputs.semantic_sha256.len(), 64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inherited_layout_edit_changes_semantic_identity_without_page_edit() {
        let root = temp_root("layout-drift");
        fs::create_dir_all(root.join("src/pages/a")).unwrap();
        fs::write(root.join("src/pages/layout.rs"), "// layout v1\n").unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// stable page\n").unwrap();

        let before = page_semantic_inputs(&root, "src/pages/a/page.rs").unwrap();
        fs::write(root.join("src/pages/layout.rs"), "// layout v2\n").unwrap();
        let after = page_semantic_inputs(&root, "src/pages/a/page.rs").unwrap();

        assert_eq!(before.page.sha256, after.page.sha256);
        assert_ne!(before.layouts[0].sha256, after.layouts[0].sha256);
        assert_ne!(before.semantic_sha256, after.semantic_sha256);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn page_edit_changes_semantic_identity() {
        let root = temp_root("page-drift");
        fs::create_dir_all(root.join("src/pages/a")).unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// page v1\n").unwrap();
        let before = page_semantic_inputs(&root, "src/pages/a/page.rs").unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// page v2\n").unwrap();
        let after = page_semantic_inputs(&root, "src/pages/a/page.rs").unwrap();
        assert_ne!(before.page.sha256, after.page.sha256);
        assert_ne!(before.semantic_sha256, after.semantic_sha256);
        let _ = fs::remove_dir_all(root);
    }
}
