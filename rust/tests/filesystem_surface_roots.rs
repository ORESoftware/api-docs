use ores_api_docs::{discover_fs_routes, FsRouteKind};
use std::{
    fs, process,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture_root() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ores-api-docs-surface-roots-{}-{nonce}",
        process::id()
    ));
    fs::create_dir_all(root.join("src/pages/(marketing)/users/[id]")).expect("page fixture");
    fs::create_dir_all(root.join("src/routes/v1/users/[id]")).expect("route fixture");
    fs::create_dir_all(root.join("pages/decoy")).expect("outside-root fixture");
    fs::create_dir_all(root.join("src/not-pages/decoy")).expect("inside-src decoy fixture");

    fs::write(
        root.join("src/pages/(marketing)/users/[id]/page.rs"),
        "// page fixture\n",
    )
    .expect("write page");
    fs::write(
        root.join("src/routes/v1/users/[id]/route.rs"),
        "// route fixture\n",
    )
    .expect("write route");
    fs::write(root.join("pages/decoy/page.rs"), "// must not discover\n")
        .expect("write outside page");
    fs::write(
        root.join("src/not-pages/decoy/page.rs"),
        "// must not discover\n",
    )
    .expect("write inside-src decoy");
    root
}

#[test]
fn page_and_api_discovery_have_disjoint_top_level_roots() {
    let root = fixture_root();
    let pages = discover_fs_routes(&root, FsRouteKind::Page).expect("discover pages");
    let api = discover_fs_routes(&root, FsRouteKind::ApiHandler).expect("discover api routes");
    fs::remove_dir_all(&root).expect("cleanup fixture");

    assert_eq!(pages.len(), 1);
    assert_eq!(api.len(), 1);
    assert_eq!(pages[0].source, "src/pages/(marketing)/users/[id]/page.rs");
    assert_eq!(api[0].source, "src/routes/v1/users/[id]/route.rs");

    // We intentionally do not implement Next.js hidden route groups. A
    // parenthesized directory is a literal URL segment unless a future,
    // separately reviewed route grammar says otherwise.
    assert_eq!(pages[0].canonical_path(), "/(marketing)/users/{id}");
    assert_eq!(api[0].canonical_path(), "/v1/users/{id}");
}
