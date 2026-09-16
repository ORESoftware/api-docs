//! Deterministic filesystem discovery for ORE Rust page/API route modules.
//!
//! Consumers should not hand-maintain route inventories. Build tooling can scan
//! `src/pages/**/page.rs` or `src/routes/**/route.rs`, parse each path through the
//! same [`FsRoute`] contract, then use the resulting stable ordering for codegen.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{validate_and_sort_fs_routes, FsRoute, FsRouteKind};

/// Discover all authored filesystem routes for one surface.
///
/// The walker never follows symlinks and sorts directory entries before
/// descending, so output is deterministic across filesystems. Missing route
/// roots are treated as an empty surface; malformed/non-UTF8 paths fail closed.
pub fn discover_fs_routes(repo_root: &Path, kind: FsRouteKind) -> Result<Vec<FsRoute>, String> {
    let (relative_root, leaf) = match kind {
        FsRouteKind::Page => ("src/pages", "page.rs"),
        FsRouteKind::ApiHandler => ("src/routes", "route.rs"),
    };
    let root = repo_root.join(relative_root);
    if !root.exists() {
        return Ok(Vec::new());
    }
    if !root.is_dir() {
        return Err(format!(
            "filesystem route root is not a directory: {}",
            root.display()
        ));
    }

    let mut files = Vec::new();
    walk_route_files(&root, leaf, &mut files)?;
    files.sort();

    let mut routes = Vec::with_capacity(files.len());
    for file in files {
        let relative = file
            .strip_prefix(repo_root)
            .map_err(|error| format!("strip repository prefix from {}: {error}", file.display()))?;
        let source = relative
            .to_str()
            .ok_or_else(|| format!("filesystem route path is not UTF-8: {}", relative.display()))?
            .replace('\\', "/");
        let route = match kind {
            FsRouteKind::Page => FsRoute::page(source),
            FsRouteKind::ApiHandler => FsRoute::api_handler(source),
        }
        .map_err(|error| error.to_string())?;
        routes.push(route);
    }

    validate_and_sort_fs_routes(routes).map_err(|error| error.to_string())
}

fn walk_route_files(root: &Path, leaf: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(root)
        .map_err(|error| {
            format!(
                "read filesystem route directory {}: {error}",
                root.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read filesystem route entry in {}: {error}", root.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "inspect filesystem route entry {}: {error}",
                entry.path().display()
            )
        })?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            walk_route_files(&entry.path(), leaf, out)?;
        } else if file_type.is_file() && entry.file_name() == leaf {
            out.push(entry.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ores-api-docs-fs-discovery-{unique}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn discovers_next_style_route_files_in_stable_precedence_order() {
        let root = temp_root();
        for path in [
            "src/routes/users/[id]/route.rs",
            "src/routes/users/new/route.rs",
            "src/routes/users/[...rest]/route.rs",
        ] {
            let file = root.join(path);
            fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
            fs::write(file, "// route\n").expect("write route");
        }

        let routes = discover_fs_routes(&root, FsRouteKind::ApiHandler).expect("discover");
        let paths = routes
            .iter()
            .map(FsRoute::canonical_path)
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["/users/new", "/users/{id}", "/users/{*rest}"]);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
