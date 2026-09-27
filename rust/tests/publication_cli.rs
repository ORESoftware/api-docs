#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_root() -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    return std::env::temp_dir().join(format!(
        "ores-api-docs-publisher-cli-{}-{id}",
        std::process::id()
    ));
}

fn collect_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(current) else {
            panic!("cannot read generated directory {}", current.display());
        };
        for entry in entries {
            let Ok(entry) = entry else {
                panic!("cannot read generated directory entry");
            };
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, files);
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                panic!("generated file escaped output root");
            };
            let Some(relative) = relative.to_str() else {
                panic!("generated file path is not UTF-8");
            };
            let Ok(content) = fs::read(&path) else {
                panic!("cannot read generated file {}", path.display());
            };
            files.insert(relative.replace('\\', "/"), content);
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    return files;
}

fn run_publisher(route_map: &Path, out_dir: &Path) {
    let status = Command::new(env!("CARGO_BIN_EXE_api-docs-publish"))
        .arg("--route-map")
        .arg(route_map)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--mode")
        .arg("publisher_external")
        .arg("--producer")
        .arg("fiducia-cloud")
        .status();
    assert!(matches!(status, Ok(value) if value.success()));
}

#[test]
fn publisher_cli_replaces_stale_files_and_reproduces_byte_identically() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let route_map =
        manifest_dir.join("../conformance/docs-publication/fiducia-cloud.route-map.json");
    let root = temp_root();
    if root.exists() {
        let _ = fs::remove_dir_all(&root);
    }
    assert!(fs::create_dir_all(&root).is_ok());
    assert!(fs::write(root.join("stale.txt"), "stale").is_ok());

    run_publisher(&route_map, &root);
    assert!(!root.join("stale.txt").exists());
    let first = collect_tree(&root);

    run_publisher(&route_map, &root);
    let second = collect_tree(&root);
    assert_eq!(first, second);

    let Some(publication_bytes) = second.get("publication.json") else {
        panic!("publisher output is missing publication.json");
    };
    let Ok(publication) = serde_json::from_slice::<Value>(publication_bytes) else {
        panic!("publication.json is invalid JSON");
    };
    assert_eq!(
        publication.get("publication_mode").and_then(Value::as_str),
        Some("publisher_external")
    );
    assert_eq!(
        publication.get("authority_scope").and_then(Value::as_str),
        Some("platform_external_developers")
    );
    assert_eq!(
        publication
            .get("publisher_provenance_required")
            .and_then(Value::as_bool),
        Some(true)
    );

    let _ = fs::remove_dir_all(root);
}
