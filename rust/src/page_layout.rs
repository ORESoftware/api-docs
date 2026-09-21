use crate::FsRoute;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[path = "page_render_source.rs"]
mod semantic;
pub use semantic::{page_render_source_inputs, PageRenderSource, PageRenderSourceInputs};

#[path = "page_manifest.rs"]
pub mod manifest;

pub const PAGE_LAYOUT_FILE: &str = "layout.rs";
pub const PAGE_TEMPLATE_FILE: &str = "template.rs";
pub const PAGE_ERROR_FILE: &str = "error.rs";
pub const PAGE_LOADING_FILE: &str = "loading.rs";
pub const PAGE_NOT_FOUND_FILE: &str = "not_found.rs";

/// Authored segment-level sources applicable to one page route.
///
/// Entries are returned root-to-leaf. Every path is repository-relative and
/// validated as a regular non-symlink file before code generation sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSegmentSources {
    pub directory: String,
    pub layout: Option<String>,
    pub template: Option<String>,
    pub error: Option<String>,
    pub loading: Option<String>,
    pub not_found: Option<String>,
}

/// Discover all Rust page-segment conventions for one `src/pages/**/page.rs`.
///
/// This is build/check-time discovery only. Runtime hosts, standalone servers,
/// and Lambda wrappers consume generated symbols and must never walk the source
/// filesystem to decide which boundary applies.
pub fn page_segment_sources(
    repo_root: &Path,
    page_source: &str,
) -> Result<Vec<PageSegmentSources>, String> {
    FsRoute::page(page_source.to_owned()).map_err(|error| error.to_string())?;

    let root = fs::canonicalize(repo_root).map_err(|error| {
        format!(
            "canonicalize repository root {}: {error}",
            repo_root.display()
        )
    })?;
    let pages = root.join("src/pages");
    let pages = fs::canonicalize(&pages)
        .map_err(|error| format!("canonicalize page root {}: {error}", pages.display()))?;
    if !pages.starts_with(&root) {
        return Err("src/pages escapes repository root".to_owned());
    }

    let page = root.join(page_source);
    reject_symlink_components(&root, &page)?;
    let page = fs::canonicalize(&page)
        .map_err(|error| format!("canonicalize page source {}: {error}", page.display()))?;
    if !page.starts_with(&pages) || !page.is_file() {
        return Err(format!(
            "page source {} is not a regular file under src/pages",
            page.display()
        ));
    }
    let page_dir = page
        .parent()
        .ok_or_else(|| "page.rs has no parent directory".to_owned())?;

    let relative_dir = page_dir
        .strip_prefix(&pages)
        .map_err(|_| "page directory escaped src/pages".to_owned())?;
    let mut directories = vec![pages.clone()];
    let mut cursor = pages.clone();
    for component in relative_dir.components() {
        let Component::Normal(segment) = component else {
            return Err("page directory contains a non-normal path component".to_owned());
        };
        cursor.push(segment);
        directories.push(cursor.clone());
    }

    directories
        .into_iter()
        .map(|directory| {
            let relative_directory = directory
                .strip_prefix(&root)
                .map_err(|_| "page segment directory escaped repository root".to_owned())?
                .to_string_lossy()
                .replace('\\', "/");
            Ok(PageSegmentSources {
                directory: relative_directory,
                layout: optional_segment_file(&root, &directory, PAGE_LAYOUT_FILE)?,
                template: optional_segment_file(&root, &directory, PAGE_TEMPLATE_FILE)?,
                error: optional_segment_file(&root, &directory, PAGE_ERROR_FILE)?,
                loading: optional_segment_file(&root, &directory, PAGE_LOADING_FILE)?,
                not_found: optional_segment_file(&root, &directory, PAGE_NOT_FOUND_FILE)?,
            })
        })
        .collect()
}

/// Compatibility helper for callers that need only the authored layout chain.
/// Returned paths remain root-to-leaf.
pub fn page_layout_sources(repo_root: &Path, page_source: &str) -> Result<Vec<String>, String> {
    Ok(page_segment_sources(repo_root, page_source)?
        .into_iter()
        .filter_map(|segment| segment.layout)
        .collect())
}

fn optional_segment_file(
    root: &Path,
    directory: &Path,
    file_name: &str,
) -> Result<Option<String>, String> {
    let candidate = directory.join(file_name);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "page segment source {} must not be a symlink",
            candidate.display()
        )),
        Ok(metadata) if !metadata.is_file() => Err(format!(
            "page segment source {} must be a regular file",
            candidate.display()
        )),
        Ok(_) => Ok(Some(
            candidate
                .strip_prefix(root)
                .map_err(|_| "page segment source escaped repository root".to_owned())?
                .to_string_lossy()
                .replace('\\', "/"),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "inspect page segment source {}: {error}",
            candidate.display()
        )),
    }
}

fn reject_symlink_components(root: &Path, target: &Path) -> Result<(), String> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| format!("{} escapes repository root", target.display()))?;
    let mut current = PathBuf::from(root);
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(format!(
                "{} contains a non-normal component",
                target.display()
            ));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{} traverses symlink {}",
                    target.display(),
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(format!("inspect {}: {error}", current.display())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ores-layout-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn discovers_root_to_leaf_without_changing_route_segments() {
        let root = temp_root("chain");
        fs::create_dir_all(root.join("src/pages/orgs/[org_id]/settings")).unwrap();
        fs::write(root.join("src/pages/layout.rs"), "pub fn layout() {}\n").unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/layout.rs"),
            "pub fn layout() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/orgs/[org_id]/settings/page.rs"),
            "// page\n",
        )
        .unwrap();
        assert_eq!(
            page_layout_sources(&root, "src/pages/orgs/[org_id]/settings/page.rs").unwrap(),
            vec!["src/pages/layout.rs", "src/pages/orgs/[org_id]/layout.rs"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_all_segment_conventions_with_rust_names() {
        let root = temp_root("conventions");
        fs::create_dir_all(root.join("src/pages/account/settings")).unwrap();
        fs::write(root.join("src/pages/layout.rs"), "// layout\n").unwrap();
        fs::write(root.join("src/pages/error.rs"), "// error\n").unwrap();
        fs::write(root.join("src/pages/account/template.rs"), "// template\n").unwrap();
        fs::write(root.join("src/pages/account/loading.rs"), "// loading\n").unwrap();
        fs::write(
            root.join("src/pages/account/not_found.rs"),
            "// not found\n",
        )
        .unwrap();
        fs::write(root.join("src/pages/account/settings/page.rs"), "// page\n").unwrap();

        let segments = page_segment_sources(&root, "src/pages/account/settings/page.rs").unwrap();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].layout.as_deref(), Some("src/pages/layout.rs"));
        assert_eq!(segments[0].error.as_deref(), Some("src/pages/error.rs"));
        assert_eq!(
            segments[1].template.as_deref(),
            Some("src/pages/account/template.rs")
        );
        assert_eq!(
            segments[1].loading.as_deref(),
            Some("src/pages/account/loading.rs")
        );
        assert_eq!(
            segments[1].not_found.as_deref(),
            Some("src/pages/account/not_found.rs")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_segment_sources() {
        use std::os::unix::fs::symlink;
        let root = temp_root("symlink");
        fs::create_dir_all(root.join("src/pages/a")).unwrap();
        fs::write(root.join("outside.rs"), "// outside\n").unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// page\n").unwrap();
        symlink(root.join("outside.rs"), root.join("src/pages/a/layout.rs")).unwrap();
        assert!(page_segment_sources(&root, "src/pages/a/page.rs").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
