//! Hardened filesystem materialization for deterministic publication files.
//!
//! Rendering stays pure in the `ores-api-docs` library. This module owns the
//! stateful boundary used by the publisher executable: replace an output tree
//! with exactly one rendered file map so stale files cannot survive reruns.

#![allow(clippy::needless_return)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

static NEXT_TRANSACTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum PublicationFsError {
    #[error("publication output path must not be a symbolic link: {0}")]
    OutputSymlink(String),
    #[error("publication output path must resolve to a directory: {0}")]
    OutputNotDirectory(String),
    #[error("publication output path is unsafe to replace: {0}")]
    UnsafeOutputRoot(String),
    #[error("generated publication artifact path is unsafe: {0}")]
    UnsafeArtifactPath(String),
    #[error("publication transaction path already exists: {0}")]
    TransactionPathExists(String),
    #[error("publication swap failed for {path}: {source}; rollback: {rollback}")]
    SwapFailed {
        path: String,
        #[source]
        source: std::io::Error,
        rollback: String,
    },
    #[error("publication filesystem operation failed for {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Replace `out_dir` with exactly `files` using a sibling staging directory.
///
/// All artifact paths are validated before filesystem mutation. The previous
/// output tree remains in place while the complete replacement is written. If
/// the final staging rename fails after the old tree is moved aside, the
/// function attempts to restore the previous tree before returning an error.
///
/// The output root may not be a symlink, regular file, filesystem root, `.` or
/// `..`. Artifact names must remain relative descendants of the output root and
/// may not contain parent traversal, root/prefix components, or empty paths.
pub fn materialize_publication_files(
    files: &BTreeMap<String, String>,
    out_dir: &Path,
) -> Result<(), PublicationFsError> {
    validate_replaceable_output_path(out_dir)?;
    validate_output_root(out_dir)?;
    for relative in files.keys() {
        validate_artifact_path(relative)?;
    }

    let parent = output_parent(out_dir);
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;

    // Recheck after parent creation so a concurrently introduced output object
    // is not trusted based on stale preflight state.
    validate_output_root(out_dir)?;

    let (staging, backup) = transaction_paths(out_dir)?;
    fs::create_dir(&staging).map_err(|source| io_error(&staging, source))?;

    if let Err(error) = write_complete_tree(files, &staging) {
        let _ = remove_path_if_present(&staging);
        return Err(error);
    }

    let output_exists = path_exists_without_following(out_dir)?;
    if !output_exists {
        if let Err(source) = fs::rename(&staging, out_dir) {
            let _ = remove_path_if_present(&staging);
            return Err(io_error(out_dir, source));
        }
        return Ok(());
    }

    fs::rename(out_dir, &backup).map_err(|source| io_error(out_dir, source))?;
    if let Err(source) = fs::rename(&staging, out_dir) {
        let rollback = match fs::rename(&backup, out_dir) {
            Ok(()) => "restored previous output tree".to_owned(),
            Err(error) => {
                format!("FAILED to restore previous output tree: {error}")
            }
        };
        let _ = remove_path_if_present(&staging);
        return Err(PublicationFsError::SwapFailed {
            path: out_dir.display().to_string(),
            source,
            rollback,
        });
    }

    remove_path_if_present(&backup)?;
    return Ok(());
}

fn write_complete_tree(
    files: &BTreeMap<String, String>,
    staging: &Path,
) -> Result<(), PublicationFsError> {
    for (relative, content) in files {
        let destination = staging.join(relative);
        let Some(parent) = destination.parent() else {
            return Err(PublicationFsError::UnsafeArtifactPath(relative.clone()));
        };
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        fs::write(&destination, content).map_err(|source| io_error(&destination, source))?;
    }

    return Ok(());
}

fn validate_replaceable_output_path(out_dir: &Path) -> Result<(), PublicationFsError> {
    let Some(file_name) = out_dir.file_name() else {
        return Err(PublicationFsError::UnsafeOutputRoot(
            out_dir.display().to_string(),
        ));
    };
    if file_name.is_empty() {
        return Err(PublicationFsError::UnsafeOutputRoot(
            out_dir.display().to_string(),
        ));
    }

    let Some(last_component) = out_dir.components().next_back() else {
        return Err(PublicationFsError::UnsafeOutputRoot(
            out_dir.display().to_string(),
        ));
    };
    if !matches!(last_component, Component::Normal(_)) {
        return Err(PublicationFsError::UnsafeOutputRoot(
            out_dir.display().to_string(),
        ));
    }

    return Ok(());
}

fn validate_output_root(out_dir: &Path) -> Result<(), PublicationFsError> {
    let metadata = match fs::symlink_metadata(out_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(source) => {
            return Err(io_error(out_dir, source));
        }
    };

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

fn output_parent(out_dir: &Path) -> &Path {
    let Some(parent) = out_dir.parent() else {
        return Path::new(".");
    };
    if parent.as_os_str().is_empty() {
        return Path::new(".");
    }

    return parent;
}

fn transaction_paths(out_dir: &Path) -> Result<(PathBuf, PathBuf), PublicationFsError> {
    let parent = output_parent(out_dir);
    let Some(file_name) = out_dir.file_name().and_then(|value| value.to_str()) else {
        return Err(PublicationFsError::UnsafeOutputRoot(
            out_dir.display().to_string(),
        ));
    };
    let transaction_id = NEXT_TRANSACTION_ID.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("{}-{transaction_id}", std::process::id());
    let staging = parent.join(format!(".{file_name}.staging-{suffix}"));
    let backup = parent.join(format!(".{file_name}.backup-{suffix}"));

    for candidate in [&staging, &backup] {
        if path_exists_without_following(candidate)? {
            return Err(PublicationFsError::TransactionPathExists(
                candidate.display().to_string(),
            ));
        }
    }

    return Ok((staging, backup));
}

fn path_exists_without_following(path: &Path) -> Result<bool, PublicationFsError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return Ok(true);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(false);
        }
        Err(source) => {
            return Err(io_error(path, source));
        }
    }
}

fn remove_path_if_present(path: &Path) -> Result<(), PublicationFsError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(source) => {
            return Err(io_error(path, source));
        }
    };

    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|source| io_error(path, source))?;
    } else {
        fs::remove_file(path).map_err(|source| io_error(path, source))?;
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
    fn rejects_parent_traversal_without_destroying_previous_output() {
        let root = temp_root("unsafe-path");
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
        assert!(fs::create_dir_all(&root).is_ok());
        assert!(fs::write(root.join("known-good.txt"), "known-good").is_ok());

        let files = BTreeMap::from([
            ("publication.json".to_owned(), "{}\n".to_owned()),
            ("../outside.json".to_owned(), "bad\n".to_owned()),
        ]);
        assert!(materialize_publication_files(&files, &root).is_err());
        assert_eq!(
            fs::read_to_string(root.join("known-good.txt"))
                .ok()
                .as_deref(),
            Some("known-good")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staging_write_failure_preserves_previous_output() {
        let root = temp_root("staging-failure");
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
        assert!(fs::create_dir_all(&root).is_ok());
        assert!(fs::write(root.join("known-good.txt"), "known-good").is_ok());

        let files = BTreeMap::from([
            ("api".to_owned(), "file blocks directory\n".to_owned()),
            ("api/openapi.json".to_owned(), "{}\n".to_owned()),
        ]);
        assert!(materialize_publication_files(&files, &root).is_err());
        assert_eq!(
            fs::read_to_string(root.join("known-good.txt"))
                .ok()
                .as_deref(),
            Some("known-good")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unsafe_output_roots() {
        let files = BTreeMap::from([("publication.json".to_owned(), "{}\n".to_owned())]);
        assert!(materialize_publication_files(&files, Path::new(".")).is_err());
        assert!(materialize_publication_files(&files, Path::new("..")).is_err());
    }
}
