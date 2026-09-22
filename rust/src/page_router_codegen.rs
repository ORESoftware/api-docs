use crate::{
    page_compile_glue, page_lambda_codegen::page_lambda_finalize_ident, FsRoute, FsRouteSegment,
    PageBuildRoute,
};
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
                "        .route({path:?}, ::axum::routing::get(__ores_page_{index}::<S>).fallback(__ores_page_method_not_allowed))\n"
            ));
        }
    }

    let assets = collect_assets(manifest);
    for (index, css) in assets.css.iter().enumerate() {
        out.push_str(&format!(
            "        .route({:?}, ::axum::routing::get(__ores_css_{index}).fallback(__ores_page_method_not_allowed))\n",
            css.public_path
        ));
    }
    for (index, (public_path, _)) in assets.js.iter().enumerate() {
        out.push_str(&format!(
            "        .route({public_path:?}, ::axum::routing::get(__ores_js_{index}).fallback(__ores_page_method_not_allowed))\n"
        ));
    }
    for (index, (public_path, _)) in assets.wasm.iter().enumerate() {
        out.push_str(&format!(
            "        .route({public_path:?}, ::axum::routing::get(__ores_wasm_{index}).fallback(__ores_page_method_not_allowed))\n"
        ));
    }
    out.push_str("        .fallback(__ores_page_route_not_found)\n        ;\n    router\n}\n\n");

    for (index, (route, item)) in routes.iter().zip(manifest).enumerate() {
        push_page_handler(&mut out, index, route, item);
        push_page_finalizer(&mut out, route, item);
    }
    push_page_route_hints(&mut out, routes, manifest, &assets);
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
    if item.auth == "public" {
        push_public_page_handler(out, index, route);
    } else {
        push_admitted_page_handler(out, index, route, &item.auth);
    }
}

fn push_public_page_handler(out: &mut String, index: usize, route: &FsRoute) {
    let module = module_ident("page", &route.source);
    let finalize = page_lambda_finalize_ident(&route.source);
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
                 __ores_page_response(result, {finalize}, &headers)\n\
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
                 __ores_page_response(result, {finalize}, &headers)\n\
             }}\n\n"
        ));
    }
}

fn push_admitted_page_handler(out: &mut String, index: usize, route: &FsRoute, auth: &str) {
    let module = module_ident("page", &route.source);
    let finalize = page_lambda_finalize_ident(&route.source);
    let dynamic = route
        .segments
        .iter()
        .any(|segment| !matches!(segment, FsRouteSegment::Static(_)));
    let params = if dynamic {
        "params"
    } else {
        "::std::collections::BTreeMap::new()"
    };
    let path_extractor = if dynamic {
        "                 ::axum::extract::Path(params): ::axum::extract::Path<::std::collections::BTreeMap<String, String>>,\n"
    } else {
        ""
    };

    out.push_str(&format!(
        "async fn __ores_page_{index}<S>(\n\
             ::axum::extract::State(state): ::axum::extract::State<S>,\n\
{path_extractor}\
             method: ::axum::http::Method,\n\
             ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,\n\
             headers: ::axum::http::HeaderMap,\n\
         ) -> ::axum::response::Response\n\
         where S: Clone + Send + Sync + 'static {{\n\
             let request = match __ores_page_request_context(&method, &uri, &headers) {{\n\
                 Ok(request) => request,\n\
                 Err(rejection) => return __ores_page_admission_response(rejection),\n\
             }};\n\
             let input = ::ores_api_docs_client::PageAdmissionInput {{\n\
                 auth: {auth:?}.to_owned(),\n\
                 route_params: {params},\n\
                 request,\n\
                 state: ::ores_api_docs_client::PageState::new(state),\n\
             }};\n\
             let ctx = match crate::ores_page_admit_request(input).await {{\n\
                 Ok(context) => context,\n\
                 Err(rejection) => return __ores_page_admission_response(rejection),\n\
             }};\n\
             let result = {module}::__ores_page_boxed(ctx).await;\n\
             __ores_page_response(result, {finalize}, &headers)\n\
         }}\n\n"
    ));
}

fn push_page_finalizer(out: &mut String, route: &FsRoute, item: &PageBuildRoute) {
    let finalize = page_lambda_finalize_ident(&route.source);
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

    out.push_str(&format!(
        "#[doc(hidden)]\n#[allow(dead_code)]\n\
         pub fn {finalize}(\n    result: ::ores_api_docs_client::PageResult,\n    hints: ::ores_api_docs_client::PageResponseRequestHints<'_>,\n) -> ::ores_api_docs_client::FinalizedPageResponse {{\n    ::ores_api_docs_client::finalize_page_response(\n        result,\n        ::ores_api_docs_client::PageResponseAssets {{\n            css_href: {css},\n            wasm_sha256: {final_wasm},\n            js_src: {js_public_path},\n        }},\n        hints,\n    )\n}}\n\
         const _: ::ores_api_docs_client::PageFinalizeFn = {finalize};\n\n"
    ));
}

fn push_page_route_hints(
    out: &mut String,
    routes: &[FsRoute],
    manifest: &[PageBuildRoute],
    assets: &RouterAssets,
) {
    out.push_str("static __ORES_PAGE_ROUTE_HINTS: &[::ores_api_docs_client::router_error_hints::RouterHintCandidate<'static>] = &[\n");
    for (route, item) in routes.iter().zip(manifest) {
        if item.auth != "public" {
            continue;
        }
        for path in route.axum_paths() {
            out.push_str(&format!(
                "    ::ores_api_docs_client::router_error_hints::RouterHintCandidate {{ path: {path:?}, methods: &[\"GET\", \"HEAD\"], disclose: true }},\n"
            ));
        }
    }
    for css in &assets.css {
        out.push_str(&format!(
            "    ::ores_api_docs_client::router_error_hints::RouterHintCandidate {{ path: {:?}, methods: &[\"GET\", \"HEAD\"], disclose: true }},\n",
            css.public_path
        ));
    }
    for (path, _) in assets.js.iter().chain(assets.wasm.iter()) {
        out.push_str(&format!(
            "    ::ores_api_docs_client::router_error_hints::RouterHintCandidate {{ path: {path:?}, methods: &[\"GET\", \"HEAD\"], disclose: true }},\n"
        ));
    }
    out.push_str("];\n\n");
}

const RESPONSE_HELPERS: &str = r##"
async fn __ores_page_route_not_found(
    method: ::axum::http::Method,
    ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,
) -> ::axum::response::Response {
    __ores_page_router_error_response(404, &method, uri.path())
}

async fn __ores_page_method_not_allowed(
    method: ::axum::http::Method,
    ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,
) -> ::axum::response::Response {
    __ores_page_router_error_response(405, &method, uri.path())
}

fn __ores_page_router_error_response(
    status: u16,
    method: &::axum::http::Method,
    path: &str,
) -> ::axum::response::Response {
    let body = ::ores_api_docs_client::router_error_hints::router_error_json(
        status,
        method.as_str(),
        path,
        __ORES_PAGE_ROUTE_HINTS,
    )
    .unwrap_or_else(|_| format!(
        "{{\"status\":{status},\"code\":\"router_error\",\"message\":\"router error\",\"suggestions\":[]}}"
    ));
    let mut response = ::axum::response::Response::builder()
        .status(status)
        .header(::axum::http::header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(::axum::http::header::CACHE_CONTROL, "no-store");
    if status == 405 {
        response = response.header(::axum::http::header::ALLOW, "GET, HEAD");
    }
    response
        .body(::axum::body::Body::from(body))
        .expect("valid generated router error response")
}

fn __ores_page_request_context(
    method: &::axum::http::Method,
    uri: &::axum::http::Uri,
    headers: &::axum::http::HeaderMap,
) -> Result<::ores_api_docs_client::PageRequestContext, ::ores_api_docs_client::PageAdmissionRejection> {
    let method = if *method == ::axum::http::Method::GET {
        ::ores_api_docs_client::PageRequestMethod::Get
    } else if *method == ::axum::http::Method::HEAD {
        ::ores_api_docs_client::PageRequestMethod::Head
    } else {
        return Err(::ores_api_docs_client::PageAdmissionRejection::text(405, "method not allowed"));
    };
    let mut normalized = ::std::collections::BTreeMap::<String, Vec<String>>::new();
    for (name, value) in headers.iter() {
        let value = value
            .to_str()
            .map_err(|_| ::ores_api_docs_client::PageAdmissionRejection::text(400, "invalid request headers"))?;
        normalized
            .entry(name.as_str().to_ascii_lowercase())
            .or_default()
            .push(value.to_owned());
    }
    let cookies = headers
        .get_all(::axum::http::header::COOKIE)
        .iter()
        .map(|value| {
            value
                .to_str()
                .map(ToOwned::to_owned)
                .map_err(|_| ::ores_api_docs_client::PageAdmissionRejection::text(400, "invalid request cookies"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(::ores_api_docs_client::PageRequestContext {
        method,
        raw_path: uri.path().to_owned(),
        raw_query: uri.query().map(ToOwned::to_owned),
        headers: normalized,
        cookies,
    })
}

fn __ores_page_admission_response(
    rejection: ::ores_api_docs_client::PageAdmissionRejection,
) -> ::axum::response::Response {
    let status = ::axum::http::StatusCode::from_u16(rejection.status)
        .unwrap_or(::axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = ::axum::response::Response::new(::axum::body::Body::from(rejection.body));
    *response.status_mut() = status;
    for (name, value) in rejection.headers {
        let Ok(name) = ::axum::http::HeaderName::from_bytes(name.as_bytes()) else {
            return ::axum::response::Response::builder()
                .status(::axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                .body(::axum::body::Body::from("page admission failed"))
                .expect("static page admission failure response");
        };
        let Ok(value) = ::axum::http::HeaderValue::from_str(&value) else {
            return ::axum::response::Response::builder()
                .status(::axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                .body(::axum::body::Body::from("page admission failed"))
                .expect("static page admission failure response");
        };
        response.headers_mut().append(name, value);
    }
    response
}

fn __ores_page_response(
    result: ::ores_api_docs_client::PageResult,
    finalize: ::ores_api_docs_client::PageFinalizeFn,
    request_headers: &::axum::http::HeaderMap,
) -> ::axum::response::Response {
    let wasm_have = request_headers
        .get("x-ores-wasm-have")
        .and_then(|value| value.to_str().ok());
    let dev_reload = ::std::env::var("ORES_STACK_DEV_RELOAD_SCRIPT").ok();
    let finalized = finalize(
        result,
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

    fn item(auth: &str) -> PageBuildRoute {
        PageBuildRoute {
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
            auth: auth.to_owned(),
            stability: "stable".to_owned(),
            database: "none".to_owned(),
            features: vec![],
            data_sources: vec![],
            tags: vec![],
            css: None,
            wasm: None,
        }
    }

    #[test]
    fn generated_router_uses_shared_framework_neutral_finalizer() {
        let root = fixture_root();
        let route = FsRoute::page("src/pages/page.rs").expect("route");
        let glue =
            page_router_glue(&root, std::slice::from_ref(&route), &[item("public")]).expect("glue");
        fs::remove_dir_all(&root).expect("fixture cleanup");

        let finalize = page_lambda_finalize_ident(&route.source);
        assert!(glue.contains(&format!("pub fn {finalize}(")));
        assert!(glue.contains("const _: ::ores_api_docs_client::PageFinalizeFn"));
        assert!(glue.contains("finalize_page_response"));
        assert!(glue.contains("PageResponseAssets"));
        assert!(glue.contains("PageResponseRequestHints"));
        assert!(glue.contains("ORES_STACK_DEV_RELOAD_SCRIPT"));
        assert!(glue.contains("fallback(__ores_page_route_not_found)"));
        assert!(glue.contains("fallback(__ores_page_method_not_allowed)"));
        assert!(glue.contains("router_error_json"));
        assert!(glue.contains("application/json; charset=utf-8"));
        assert!(!glue.contains("crate::ores_page_admit_request(input).await"));
        assert!(!glue.contains("fn __ores_inject_head"));
        assert!(!glue.contains("fn __ores_inject_body"));
    }

    #[test]
    fn non_public_router_uses_product_admission_before_page() {
        let root = fixture_root();
        let route = FsRoute::page("src/pages/page.rs").expect("route");
        let glue = page_router_glue(&root, std::slice::from_ref(&route), &[item("session")])
            .expect("glue");
        fs::remove_dir_all(&root).expect("fixture cleanup");

        let admission = glue
            .find("crate::ores_page_admit_request(input).await")
            .expect("product admission hook");
        let invoke = glue
            .find("__ores_page_boxed(ctx).await")
            .expect("page invocation");
        assert!(admission < invoke);
        assert!(glue.contains("auth: \"session\".to_owned()"));
        assert!(glue.contains("PageRequestContext"));
        assert!(glue.contains("PageAdmissionRejection"));
    }
}
