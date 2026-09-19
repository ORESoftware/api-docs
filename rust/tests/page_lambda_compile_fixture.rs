//! Compiles and runs a generated web-page `lambda.rs` as its own bin crate.
//!
//! String assertions cannot prove the contract that matters here: `lambda.rs`
//! is a separate crate root, the page it serves uses `crate::` paths into the
//! web-server library, the page module is private, and response finalization is
//! bound to the same admitted page build metadata used by the Axum router. This
//! fixture builds that exact shape with a stub provider runtime and checks the
//! page really ran. WEB SERVER pages only; nothing here touches the API-server
//! RPC surface.

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

    // The product web server: a library whose page reaches back into the crate.
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

    // Stand-in for an organization's `*-lambdas` provider runtime.
    let runtime = root.join("runtime");
    write(
        &runtime.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"fixture-lambdas\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nores-api-docs-client = {{ path = {client:?} }}\n"
        ),
    );
    write(
        &runtime.join("src/lib.rs"),
        r#"use ores_api_docs_client::{
    PageContext, PageFinalizeFn, PageFn, PageLambdaStateError, PageResponseRequestHints, PageState,
};
use std::{collections::BTreeMap, future::Future};

pub struct PageHttpRequest {
    pub path: String,
}

pub struct PageHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug)]
pub struct RuntimeError(pub String);

impl RuntimeError {
    pub fn state_init(error: PageLambdaStateError) -> Self {
        Self(error.to_string())
    }
}

pub async fn invoke_page(
    request: PageHttpRequest,
    state: PageState,
    _route: &'static str,
    _axum_paths: &'static [&'static str],
    page: PageFn,
    finalize: PageFinalizeFn,
) -> Result<PageHttpResponse, RuntimeError> {
    let mut params = BTreeMap::new();
    if let Some(id) = request.path.rsplit('/').next() {
        params.insert("id".to_owned(), id.to_owned());
    }
    let mut ctx = PageContext::new(params, request.path);
    ctx.state = state;
    let result = page(ctx).await;
    let finalized = finalize(result, PageResponseRequestHints::default());
    let body = String::from_utf8(finalized.body)
        .map_err(|error| RuntimeError(format!("non-utf8 fixture body: {error}")))?;
    Ok(PageHttpResponse {
        status: finalized.status,
        body,
    })
}

async fn run_once<H, Fut>(provider: &str, state: PageState, handler: H) -> Result<(), RuntimeError>
where
    H: Fn(PageState, PageHttpRequest) -> Fut,
    Fut: Future<Output = Result<PageHttpResponse, RuntimeError>>,
{
    let response = handler(
        state,
        PageHttpRequest {
            path: "/users/42".to_owned(),
        },
    )
    .await?;
    println!("{provider} {} {}", response.status, response.body);
    Ok(())
}

pub mod aws {
    use super::*;
    pub async fn run_page<H, Fut>(state: PageState, handler: H) -> Result<(), RuntimeError>
    where
        H: Fn(PageState, PageHttpRequest) -> Fut,
        Fut: Future<Output = Result<PageHttpResponse, RuntimeError>>,
    {
        run_once("aws", state, handler).await
    }
}

pub mod gcp {
    use super::*;
    pub async fn run_page<H, Fut>(state: PageState, handler: H) -> Result<(), RuntimeError>
    where
        H: Fn(PageState, PageHttpRequest) -> Fut,
        Fut: Future<Output = Result<PageHttpResponse, RuntimeError>>,
    {
        run_once("gcp", state, handler).await
    }
}
"#,
    );

    // The build unit `ores-stack` generates: its own workspace root, a bin whose
    // path points back at the sibling lambda.rs, and the two stable aliases.
    let lambda = web.join("src/pages/users/[id]/lambda.rs");
    write(
        &root.join("unit/Cargo.toml"),
        &format!(
            "[package]\nname = \"fixture-page-lambda-unit\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [workspace]\n\n\
             [[bin]]\nname = \"page-lambda\"\npath = {lambda:?}\n\n\
             [features]\nores-page-lambda-aws = []\nores-page-lambda-gcp = []\n\n\
             [dependencies]\n\
             ores_web_app = {{ package = \"fixture-web-server\", path = {web:?} }}\n\
             ores_page_lambda_runtime = {{ package = \"fixture-lambdas\", path = {runtime:?} }}\n\
             ores-api-docs-client = {{ path = {client:?} }}\n\
             tokio = {{ version = \"1\", features = [\"macros\", \"rt-multi-thread\"] }}\n"
        ),
    );
    root
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
fn generated_lambda_builds_as_its_own_bin_and_runs_a_page_that_uses_crate_paths() {
    let root = fixture();

    for provider in ["aws", "gcp"] {
        let feature = format!("ores-page-lambda-{provider}");
        let output = cargo(&root, &["run", "--quiet", "--features", &feature]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{provider} build/run failed:\n{stderr}"
        );
        assert_eq!(
            stdout.trim(),
            format!("{provider} 200 <p>fixture user 42</p>"),
            "page did not run through crate:: paths with application state and shared finalization"
        );
    }

    let neither = cargo(&root, &["check", "--quiet"]);
    assert!(!neither.status.success());
    assert!(
        String::from_utf8_lossy(&neither.stderr).contains("requires exactly one provider feature")
    );

    let both = cargo(
        &root,
        &[
            "check",
            "--quiet",
            "--features",
            "ores-page-lambda-aws,ores-page-lambda-gcp",
        ],
    );
    assert!(!both.status.success());
    assert!(String::from_utf8_lossy(&both.stderr).contains("mutually exclusive"));
}
