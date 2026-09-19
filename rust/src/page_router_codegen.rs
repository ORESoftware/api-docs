use crate::{page_compile_glue, FsRoute, FsRouteSegment, PageBuildRoute};
use std::{collections::BTreeSet, path::Path};

/// Emit an Axum router directly from the validated filesystem page inventory.
/// The generated router is product-web code, not an api-docs RPC server.
pub fn page_router_glue(
    repo_root: &Path,
    routes: &[FsRoute],
    manifest: &[PageBuildRoute],
) -> Result<String, String> {
    if routes.len() != manifest.len() {
        return Err("page router manifest length does not match route inventory".to_owned());
    }

    let mut out = page_compile_glue(repo_root, routes)?;
    out.push_str("\n// Generated browser-page router. This is not an RPC surface.\n");
    out.push_str(
        "pub fn ores_pages_router<S>() -> ::axum::Router<S>\n\
         where S: Clone + Send + Sync + 'static {\n\
         let router = ::axum::Router::<S>::new()\n",
    );
    for (index, route) in routes.iter().enumerate() {
        for path in route.axum_paths() {
            out.push_str(&format!(
                "        .route({path:?}, ::axum::routing::get(__ores_page_{index}::<S>))\n"
            ));
        }
    }

    let assets = collect_assets(manifest);
    for (index, css) in assets.css.iter().enumerate() {
        out.push_str(&format!(
            "        .route({:?}, ::axum::routing::get(__ores_css_{index}))\n",
            css.public_path
        ));
    }
    for (index, (public_path, _)) in assets.js.iter().enumerate() {
        out.push_str(&format!(
            "        .route({public_path:?}, ::axum::routing::get(__ores_js_{index}))\n"
        ));
    }
    for (index, (public_path, _)) in assets.wasm.iter().enumerate() {
        out.push_str(&format!(
            "        .route({public_path:?}, ::axum::routing::get(__ores_wasm_{index}))\n"
        ));
    }
    out.push_str("        ;\n    router\n}\n\n");

    for (index, (route, item)) in routes.iter().zip(manifest).enumerate() {
        push_page_handler(&mut out, index, route, item);
    }
    out.push_str(RESPONSE_HELPERS);
    push_asset_handlers(&mut out, &assets);
    Ok(out)
}

#[derive(Default)]
struct RouterAssets {
    css: Vec<crate::ContentAsset>,
    js: Vec<(String, String)>,
    wasm: Vec<(String, String)>,
}

fn collect_assets(manifest: &[PageBuildRoute]) -> RouterAssets {
    let mut assets = RouterAssets::default();
    let mut css_seen = BTreeSet::new();
    let mut js_seen = BTreeSet::new();
    let mut wasm_seen = BTreeSet::new();

    for item in manifest {
        if let Some(css) = &item.css {
            if css_seen.insert(css.public_path.clone()) {
                assets.css.push(css.clone());
            }
        }
        if let Some(wasm) = &item.wasm {
            if let (Some(public_path), Some(output_file)) =
                (&wasm.js_public_path, &wasm.js_output_file)
            {
                if js_seen.insert(public_path.clone()) {
                    assets.js.push((public_path.clone(), output_file.clone()));
                }
            }
            if let (Some(public_path), Some(output_file)) =
                (&wasm.public_path, &wasm.wasm_output_file)
            {
                if wasm_seen.insert(public_path.clone()) {
                    assets.wasm.push((public_path.clone(), output_file.clone()));
                }
            }
        }
    }
    assets
}

fn push_page_handler(out: &mut String, index: usize, route: &FsRoute, item: &PageBuildRoute) {
    let module = module_ident("page", &route.source);
    let css = item
        .css
        .as_ref()
        .map(|asset| format!("Some({:?})", asset.public_path))
        .unwrap_or_else(|| "None".to_owned());
    let final_wasm = item
        .wasm
        .as_ref()
        .and_then(|wasm| wasm.final_wasm_sha256.as_ref())
        .map(|digest| format!("Some({digest:?})"))
        .unwrap_or_else(|| "None".to_owned());
    let js_public_path = item
        .wasm
        .as_ref()
        .and_then(|wasm| wasm.js_public_path.as_ref())
        .map(|path| format!("Some({path:?})"))
        .unwrap_or_else(|| "None".to_owned());
    let dynamic = route
        .segments
        .iter()
        .any(|segment| !matches!(segment, FsRouteSegment::Static(_)));

    if dynamic {
        out.push_str(&format!(
            "async fn __ores_page_{index}<S>(\n\
                 ::axum::extract::State(state): ::axum::extract::State<S>,\n\
                 ::axum::extract::Path(params): ::axum::extract::Path<::std::collections::BTreeMap<String, String>>,\n\
                 ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,\n\
                 headers: ::axum::http::HeaderMap,\n\
             ) -> ::axum::response::Response\n\
             where S: Clone + Send + Sync + 'static {{\n\
                 let ctx = ::ores_api_docs_client::PageContext::with_state(params, uri.path(), state);\n\
                 let result = {module}::__ores_page_boxed(ctx).await;\n\
                 __ores_page_response(result, {css}, {final_wasm}, {js_public_path}, &headers)\n\
             }}\n\n"
        ));
    } else {
        out.push_str(&format!(
            "async fn __ores_page_{index}<S>(\n\
                 ::axum::extract::State(state): ::axum::extract::State<S>,\n\
                 ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,\n\
                 headers: ::axum::http::HeaderMap,\n\
             ) -> ::axum::response::Response\n\
             where S: Clone + Send + Sync + 'static {{\n\
                 let ctx = ::ores_api_docs_client::PageContext::with_state(::std::collections::BTreeMap::new(), uri.path(), state);\n\
                 let result = {module}::__ores_page_boxed(ctx).await;\n\
                 __ores_page_response(result, {css}, {final_wasm}, {js_public_path}, &headers)\n\
             }}\n\n"
        ));
    }
}

const RESPONSE_HELPERS: &str = r##"
fn __ores_page_response(
    result: ::ores_api_docs_client::PageResult,
    css: Option<&'static str>,
    final_wasm_sha256: Option<&'static str>,
    js_public_path: Option<&'static str>,
    request_headers: &::axum::http::HeaderMap,
) -> ::axum::response::Response {
    let wasm_have = request_headers
        .get("x-ores-wasm-have")
        .and_then(|value| value.to_str().ok());
    let dev_reload = ::std::env::var("ORES_STACK_DEV_RELOAD_SCRIPT").ok();
    let finalized = ::ores_api_docs_client::finalize_page_response(
        result,
        ::ores_api_docs_client::PageResponseAssets {
            css_href: css,
            wasm_sha256: final_wasm_sha256,
            js_src: js_public_path,
        },
        ::ores_api_docs_client::PageResponseRequestHints {
            wasm_have,
            dev_reload_script: dev_reload.as_deref(),
        },
    );

    let mut response = ::axum::response::Response::builder().status(finalized.status);
    for (name, value) in finalized.headers {
        response = response.header(name, value);
    }
    response
        .body(::axum::body::Body::from(finalized.body))
        .expect("valid finalized page response")
}

"##;

fn push_asset_handlers(out: &mut String, assets: &RouterAssets) {
    for (index, css) in assets.css.iter().enumerate() {
        out.push_str(&format!(
            "async fn __ores_css_{index}() -> ::axum::response::Response {{\n\
                 ::axum::response::Response::builder()\n\
                     .status(::axum::http::StatusCode::OK)\n\
                     .header(\"content-type\", \"text/css; charset=utf-8\")\n\
                     .header(\"cache-control\", \"public, max-age=31536000, immutable\")\n\
                     .body(::axum::body::Body::from(include_str!(concat!(env!(\"OUT_DIR\"), \"/page-assets/{file}\"))))\n\
                     .expect(\"valid css response\")\n\
             }}\n\n",
            file = css.output_file,
        ));
    }
    for (index, (_, file)) in assets.js.iter().enumerate() {
        out.push_str(&format!(
            "async fn __ores_js_{index}() -> ::axum::response::Response {{\n\
                 ::axum::response::Response::builder()\n\
                     .status(::axum::http::StatusCode::OK)\n\
                     .header(\"content-type\", \"text/javascript; charset=utf-8\")\n\
                     .header(\"cache-control\", \"public, max-age=31536000, immutable\")\n\
                     .body(::axum::body::Body::from(include_str!(concat!(env!(\"OUT_DIR\"), \"/page-assets/{file}\"))))\n\
                     .expect(\"valid js response\")\n\
             }}\n\n"
        ));
    }
    for (index, (_, file)) in assets.wasm.iter().enumerate() {
        out.push_str(&format!(
            "async fn __ores_wasm_{index}() -> ::axum::response::Response {{\n\
                 ::axum::response::Response::builder()\n\
                     .status(::axum::http::StatusCode::OK)\n\
                     .header(\"content-type\", \"application/wasm\")\n\
                     .header(\"cache-control\", \"public, max-age=31536000, immutable\")\n\
                     .body(::axum::body::Body::from(&include_bytes!(concat!(env!(\"OUT_DIR\"), \"/page-assets/{file}\"))[..]))\n\
                     .expect(\"valid wasm response\")\n\
             }}\n\n"
        ));
    }
}

fn module_ident(prefix: &str, source: &str) -> String {
    let mut out = format!("__ores_{prefix}_");
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn fixture_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ores-api-docs-page-router-{}-{unique}",
            process::id()
        ));
        let pages = root.join("src/pages");
        fs::create_dir_all(&pages).expect("fixture pages");
        fs::write(
            pages.join("page.rs"),
            r#"#[ores_page(renderer = "mash", delivery = "ssr_only")]
pub async fn page(_ctx: ::ores_api_docs_client::PageContext) -> ::ores_api_docs_client::PageResult {
    unimplemented!()
}
"#
            .replace("\\\"", "\""),
        )
        .expect("fixture page");
        root
    }

    #[test]
    fn generated_router_uses_shared_framework_neutral_finalizer() {
        let root = fixture_root();
        let route = FsRoute::page("src/pages/page.rs").expect("route");
        let item = PageBuildRoute {
            source: "src/pages/page.rs".to_owned(),
            generator: None,
            canonical_path: "/".to_owned(),
            axum_paths: vec!["/".to_owned()],
            dioxus_paths: vec!["/".to_owned()],
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
        let glue = page_router_glue(&root, &[route], &[item]).expect("glue");
        fs::remove_dir_all(&root).expect("fixture cleanup");

        assert!(glue.contains("finalize_page_response"));
        assert!(glue.contains("PageResponseAssets"));
        assert!(glue.contains("PageResponseRequestHints"));
        assert!(glue.contains("ORES_STACK_DEV_RELOAD_SCRIPT"));
        assert!(!glue.contains("fn __ores_inject_head"));
        assert!(!glue.contains("fn __ores_inject_body"));
    }
}
