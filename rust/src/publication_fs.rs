//! Hardened filesystem materialization for deterministic publication files.
//!
//! Rendering stays pure in the `ores-api-docs` library. This module owns the
//! stateful boundary used by the publisher executable: replace an output tree
//! with exactly one rendered file map so stale files cannot survive reruns.

#![allow(clippy::needless_return)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublicationFsError {
    #[error("publication output path must not be a symbolic link: {0}")]
    OutputSymlink(String),
    #[error("publication output path must resolve to a directory: {0}")]
    OutputNotDirectory(String),
    #[error("generated publication artifact path is unsafe: {0}")]
    UnsafeArtifactPath(String),
    #[error("publication filesystem operation failed for {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Replace `out_dir` with exactly `files`.
///
/// The output root may not be a symlink or regular file. Artifact names must
/// remain relative descendants of the output root and may not contain parent
/// traversal, root/prefix components, or empty paths.
pub fn materialize_publication_files(
    files: &BTreeMap<String, String>,
    out_dir: &Path,
) -> Result<(), PublicationFsError> {
    validate_output_root(out_dir)?;

    if out_dir.exists() {
        fs::remove_dir_all(out_dir).map_err(|source| io_error(out_dir, source))?;
    }
    fs::create_dir_all(out_dir).map_err(|source| io_error(out_dir, source))?;

    for (relative, content) in files {
        validate_artifact_path(relative)?;
        let destination = out_dir.join(relative);
        let Some(parent) = destination.parent() else {
            return Err(PublicationFsError::UnsafeArtifactPath(relative.clone()));
        };
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        fs::write(&destination, content).map_err(|source| io_error(&destination, source))?;
    }

    return Ok(());
}

fn validate_output_root(out_dir: &Path) -> Result<(), PublicationFsError> {
    if !out_dir.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(out_dir).map_err(|source| io_error(out_dir, source))?;
    if metadata.file_type().is_symlink() {
        return Err(PublicationFsError::OutputSymlink(
            out_dir.display().to_string(),
        ));
    }
    if !metadata.is_dir() {
        return Err(PublicationFsError::OutputNotDirectory(
            out_dir.display().to_string(),
        ));
    }

    return Ok(());
}

fn validate_artifact_path(path: &str) -> Result<(), PublicationFsError> {
    if path.trim().is_empty() {
        return Err(PublicationFsError::UnsafeArtifactPath(path.to_owned()));
    }

    let parsed = Path::new(path);
    if parsed.is_absolute() {
        return Err(PublicationFsError::UnsafeArtifactPath(path.to_owned()));
    }

    let has_normal_component = parsed
        .components()
        .any(|component| matches!(component, Component::Normal(_)));
    let unsafe_component = parsed.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if !has_normal_component || unsafe_component {
        return Err(PublicationFsError::UnsafeArtifactPath(path.to_owned()));
    }

    return Ok(());
}

fn io_error(path: &Path, source: std::io::Error) -> PublicationFsError {
    return PublicationFsError::Io {
        path: path.display().to_string(),
        source,
    };
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    fn temp_root(label: &str) -> std::path::PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        return std::env::temp_dir()
            .join(format!("ores-api-docs-{label}-{}-{id}", std::process::id()));
    }

    #[test]
    fn replaces_stale_output_with_exact_file_tree() {
        let root = temp_root("replace");
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
        assert!(fs::create_dir_all(&root).is_ok());
        assert!(fs::write(root.join("stale.txt"), "stale").is_ok());

        let files = BTreeMap::from([
            ("api/openapi.json".to_owned(), "{}\n".to_owned()),
            ("publication.json".to_owned(), "{}\n".to_owned()),
        ]);
        assert!(materialize_publication_files(&files, &root).is_ok());

        assert!(!root.join("stale.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("api/openapi.json"))
                .ok()
                .as_deref(),
            Some("{}\n")
        );
        assert_eq!(
            fs::read_to_string(root.join("publication.json"))
                .ok()
                .as_deref(),
            Some("{}\n")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_parent_traversal_artifacts() {
        assert!(validate_artifact_path("../outside.json").is_err());
        assert!(validate_artifact_path("api/../../outside.json").is_err());
        assert!(validate_artifact_path("").is_err());
    }
}
