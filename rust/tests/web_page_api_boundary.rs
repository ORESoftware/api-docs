use std::{
    fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use ores_api_docs::{read_page_build_manifest, write_page_build_outputs};

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ores-api-docs-{label}-{}-{nonce}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("test temp root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn api_route_gen_is_ignored_while_page_sibling_gen_is_admitted() {
    let temp = TestDir::new("web-page-api-boundary");
    let root = temp.path();
    let page_dir = root.join("src/pages/blog/[slug]");
    let api_dir = root.join("src/routes/v1/blog/[slug]");
    let out = root.join("out");
    fs::create_dir_all(&page_dir).expect("page dir");
    fs::create_dir_all(&api_dir).expect("api dir");

    fs::write(
        page_dir.join("page.rs"),
        r#"
#[ores_page(
    renderer = "mash",
    delivery = "ssr_only",
    render = "static_only",
    auth = "public",
    stability = "stable",
    database = "none"
)]
pub async fn page() {}
"#,
    )
    .expect("page source");
    fs::write(
        page_dir.join("gen.rs"),
        "#[ores_generate] pub async fn generate_static_params() {}\n",
    )
    .expect("page gen");

    // Deliberately valid-looking but semantically irrelevant to the browser page
    // scanner. Its content must never become API route authority or page input.
    fs::write(
        api_dir.join("gen.rs"),
        "#[ores_generate] pub async fn generate_static_params() { panic!(\"API gen must be ignored\") }\n",
    )
    .expect("stray API gen");

    let outputs = write_page_build_outputs(root, &out).expect("page build");
    let manifest = read_page_build_manifest(&outputs.manifest_path).expect("manifest");
    assert_eq!(manifest.routes.len(), 1);
    assert_eq!(manifest.routes[0].source, "src/pages/blog/[slug]/page.rs");
    assert_eq!(
        manifest.routes[0].generator.as_deref(),
        Some("src/pages/blog/[slug]/gen.rs")
    );
    assert!(outputs
        .rerun_if_changed
        .iter()
        .all(|path| !path.ends_with("src/routes/v1/blog/[slug]/gen.rs")));
}
