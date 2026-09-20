use crate::FsRoute;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[path = "page_semantic.rs"]
mod semantic;
pub use semantic::{page_render_source_inputs, PageRenderSource, PageRenderSourceInputs};

pub const PAGE_LAYOUT_FILE: &str = "layout.rs";

/// Discover authored layouts for one `src/pages/**/page.rs` route.
///
/// The returned paths are repository-relative and ordered root-to-leaf. Runtime
/// code must not repeat this filesystem walk; generated compile glue embeds the
/// resolved chain and applies it leaf-to-root so the root layout is outermost.
pub fn page_layout_sources(repo_root: &Path, page_source: &str) -> Result<Vec<String>, String> {
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

    let mut layouts = Vec::new();
    for directory in directories {
        let candidate = directory.join(PAGE_LAYOUT_FILE);
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "layout source {} must not be a symlink",
                    candidate.display()
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(format!(
                    "layout source {} must be a regular file",
                    candidate.display()
                ));
            }
            Ok(_) => {
                let relative = candidate
                    .strip_prefix(&root)
                    .map_err(|_| "layout source escaped repository root".to_owned())?
                    .to_string_lossy()
                    .replace('\\', "/");
                layouts.push(relative);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "inspect layout source {}: {error}",
                    candidate.display()
                ))
            }
        }
    }
    Ok(layouts)
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

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_layouts() {
        use std::os::unix::fs::symlink;
        let root = temp_root("symlink");
        fs::create_dir_all(root.join("src/pages/a")).unwrap();
        fs::write(root.join("outside.rs"), "// outside\n").unwrap();
        fs::write(root.join("src/pages/a/page.rs"), "// page\n").unwrap();
        symlink(root.join("outside.rs"), root.join("src/pages/a/layout.rs")).unwrap();
        assert!(page_layout_sources(&root, "src/pages/a/page.rs").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
