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
                "        .route({path:?}, ::axum::routing::get(__ores_page_{index}))\n"
            ));
        }
    }

    let mut css_seen = BTreeSet::new();
    let mut css_routes = Vec::new();
    for item in manifest {
        if let Some(css) = &item.css {
            if css_seen.insert(css.public_path.clone()) {
                let index = css_routes.len();
                css_routes.push(css.clone());
                out.push_str(&format!(
                    "        .route({:?}, ::axum::routing::get(__ores_css_{index}))\n",
                    css.public_path
                ));
            }
        }
    }
    out.push_str("        ;\n    router\n}\n\n");

    for (index, (route, item)) in routes.iter().zip(manifest).enumerate() {
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
        let dynamic = route
            .segments
            .iter()
            .any(|segment| !matches!(segment, FsRouteSegment::Static(_)));
        if dynamic {
            out.push_str(&format!(
                "async fn __ores_page_{index}(\n\
                     ::axum::extract::Path(params): ::axum::extract::Path<::std::collections::BTreeMap<String, String>>,\n\
                     ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,\n\
                     headers: ::axum::http::HeaderMap,\n\
                 ) -> ::axum::response::Response {{\n\
                     let ctx = ::ores_api_docs_client::PageContext {{ route_params: params, request_path: uri.path().to_owned() }};\n\
                     __ores_page_response({module}::page(ctx), {css}, {final_wasm}, &headers)\n\
                 }}\n\n"
            ));
        } else {
            out.push_str(&format!(
                "async fn __ores_page_{index}(\n\
                     ::axum::extract::OriginalUri(uri): ::axum::extract::OriginalUri,\n\
                     headers: ::axum::http::HeaderMap,\n\
                 ) -> ::axum::response::Response {{\n\
                     let ctx = ::ores_api_docs_client::PageContext {{ route_params: ::std::collections::BTreeMap::new(), request_path: uri.path().to_owned() }};\n\
                     __ores_page_response({module}::page(ctx), {css}, {final_wasm}, &headers)\n\
                 }}\n\n"
            ));
        }
    }

    out.push_str(
        "fn __ores_page_response(\n\
             result: ::ores_api_docs_client::PageResult,\n\
             css: Option<&'static str>,\n\
             final_wasm_sha256: Option<&'static str>,\n\
             request_headers: &::axum::http::HeaderMap,\n\
         ) -> ::axum::response::Response {\n\
             let document = match result {\n\
                 Ok(document) => document,\n\
                 Err(error) => return ::axum::response::Response::builder()\n\
                     .status(::axum::http::StatusCode::INTERNAL_SERVER_ERROR)\n\
                     .header(\"content-type\", \"text/plain; charset=utf-8\")\n\
                     .body(::axum::body::Body::from(format!(\"page render failed: {error}\")))\n\
                     .expect(\"valid page error response\"),\n\
             };\n\
             let mut html = document.html;\n\
             if let Some(href) = css {\n\
                 let tag = format!(r#\"<link rel=\\\"stylesheet\\\" href=\\\"{href}\\\">\"#);\n\
                 html = __ores_inject_head(html, &tag);\n\
             }\n\
             if let Some(digest) = final_wasm_sha256 {\n\
                 let already_present = request_headers\n\
                     .get(::ores_api_docs::WASM_HAVE_HEADER)\n\
                     .and_then(|value| value.to_str().ok())\n\
                     .map(|value| value.split(',').any(|item| item.trim() == digest))\n\
                     .unwrap_or(false);\n\
                 if !already_present {\n\
                     let marker = format!(r#\"<meta name=\\\"ores-wasm-required\\\" content=\\\"{digest}\\\">\"#);\n\
                     html = __ores_inject_head(html, &marker);\n\
                 }\n\
             }\n\
             let mut response = ::axum::response::Response::builder().status(document.status);\n\
             response = response.header(\"content-type\", \"text/html; charset=utf-8\");\n\
             for (name, value) in document.headers { response = response.header(name, value); }\n\
             response.body(::axum::body::Body::from(html)).expect(\"valid page response\")\n\
         }\n\n\
         fn __ores_inject_head(mut html: String, tag: &str) -> String {\n\
             if let Some(index) = html.find(\"</head>\") {\n\
                 html.insert_str(index, tag);\n\
                 html\n\
             } else {\n\
                 format!(\"{tag}{html}\")\n\
             }\n\
         }\n\n",
    );

    for (index, css) in css_routes.iter().enumerate() {
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
    Ok(out)
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
