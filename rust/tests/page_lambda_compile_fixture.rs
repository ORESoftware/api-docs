//! Compiles and runs provider-neutral generated web `lambda.rs` through tiny
//! temporary provider mains.
//!
//! This is the same ownership shape used by `*-lambdas`: server source owns the
//! page-function module, while a generated build directory owns provider main().
//! The fixture proves private page modules, `crate::` references, state creation
//! and shared response finalization without making server source provider-aware.

use ores_api_docs::{page_lambda_glue, page_router_glue, FsRoute, PageBuildRoute};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PAGE_SOURCE: &str = "src/pages/users/[id]/page.rs";

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
    fs::write(path, contents).expect("fixture file");
}

fn fixture() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("page-lambda-compile-fixture");
    let _ = fs::remove_dir_all(&root);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    let client = repo.join("clients/rust");
    let macros = repo.join("macros/rust");

    let web = root.join("web");
    write(
        &web.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"fixture-web-server\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\naxum = \"0.8\"\nores-api-docs-client = {{ path = {client:?} }}\nores-api-docs-macros = {{ path = {macros:?} }}\n"
        ),
    );
    write(
        &web.join(PAGE_SOURCE),
        r#"use ores_api_docs_macros::ores_page;

#[ores_page(renderer = "mash", delivery = "ssr_only")]
pub async fn page(ctx: ::ores_api_docs_client::PageContext) -> ::ores_api_docs_client::PageResult {
    let app = ctx
        .state::<crate::AppState>()
        .expect("page lambda must hand the page the application state");
    let id = ctx.route_params.get("id").cloned().unwrap_or_default();
    Ok(::ores_api_docs_client::PageDocument::html(
        crate::shared::greeting(&app.site, &id),
    ))
}
"#,
    );

    let route = FsRoute::page(PAGE_SOURCE).expect("page route");
    let item = PageBuildRoute {
        source: PAGE_SOURCE.to_owned(),
        generator: None,
        canonical_path: "/users/{id}".to_owned(),
        axum_paths: vec!["/users/{id}".to_owned()],
        dioxus_paths: vec!["/users/:id".to_owned()],
        renderer: "mash".to_owned(),
        delivery: "ssr_only".to_owned(),
        render: "dynamic".to_owned(),
        revalidate_secs: None,
        on_demand: None,
        title: None,
        summary: None,
        auth: "public".to_owned(),
        stability: "stable".to_owned(),
        database: "none".to_owned(),
        features: vec![],
        data_sources: vec![],
        tags: vec![],
        css: None,
        wasm: None,
    };
    let glue = page_router_glue(&web, std::slice::from_ref(&route), &[item]).expect("page glue");
    write(&web.join("src/ores_pages_glue.rs"), &glue);
    write(
        &web.join("src/lib.rs"),
        r#"pub struct AppState {
    pub site: String,
}

pub mod shared {
    pub fn greeting(site: &str, id: &str) -> String {
        format!("<p>{site} user {id}</p>")
    }
}

pub mod ores_pages {
    include!("ores_pages_glue.rs");
}

pub fn ores_page_lambda_state() -> ::ores_api_docs_client::PageLambdaStateFuture {
    Box::pin(async {
        Ok(::ores_api_docs_client::PageState::new(AppState {
            site: "fixture".to_owned(),
        }))
    })
}
"#,
    );
    write(
        &web.join("src/pages/users/[id]/lambda.rs"),
        &page_lambda_glue(&route).expect("lambda source"),
    );

    let lambda = web.join("src/pages/users/[id]/lambda.rs");
    write(
        &root.join("unit/Cargo.toml"),
        &format!(
            "[package]\nname = \"fixture-page-lambda-unit\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [workspace]\n\n\
             [dependencies]\n\
             ores_web_app = {{ package = \"fixture-web-server\", path = {web:?} }}\n\
             ores-api-docs-client = {{ path = {client:?} }}\n\
             tokio = {{ version = \"1\", features = [\"macros\", \"rt-multi-thread\"] }}\n"
        ),
    );
    write(
        &root.join("unit/lambda-path.txt"),
        &lambda.to_string_lossy(),
    );
    root
}

fn wrapper(root: &Path, provider: &str) -> String {
    let lambda = fs::read_to_string(root.join("unit/lambda-path.txt")).expect("lambda path");
    format!(
        r#"#[path = {lambda:?}]
mod generated_lambda;

use std::collections::BTreeMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {{
    let state = generated_lambda::init_state().await?;
    let mut params = BTreeMap::new();
    params.insert("id".to_owned(), "42".to_owned());
    let mut context = ::ores_api_docs_client::PageContext::new(params, "/users/42");
    context.state = state;
    let response = generated_lambda::run(
        context,
        ::ores_api_docs_client::PageResponseRequestHints::default(),
    )
    .await;
    let body = String::from_utf8(response.body)?;
    println!("{provider} {{}} {{}}", response.status, body);
    Ok(())
}}
"#
    )
}

fn cargo(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO"))
        .args(args)
        .arg("--manifest-path")
        .arg(root.join("unit/Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("spawn cargo")
}

#[test]
fn generated_page_module_runs_through_external_aws_and_gcp_mains() {
    let root = fixture();
    for provider in ["aws", "gcp"] {
        write(&root.join("unit/src/main.rs"), &wrapper(&root, provider));
        let output = cargo(&root, &["run", "--quiet"]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{provider} temp wrapper build/run failed:\n{stderr}"
        );
        assert_eq!(
            stdout.trim(),
            format!("{provider} 200 <p>fixture user 42</p>"),
            "provider wrapper did not execute the one generated page function"
        );
    }
}
