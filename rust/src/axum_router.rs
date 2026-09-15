//! Axum router for hardened docs aliases.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header::HeaderName, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use crate::catalog::Catalog;
use crate::discovery::DocsDiscoveryManifest;
use crate::headers::{hardening_headers, method_not_allowed_headers, BodyKind};
use crate::html::render_html;

pub fn router(catalog: Catalog) -> Router {
    let state = Arc::new(catalog);
    Router::new()
        .route(
            "/api-docs/manifest.json",
            get(discovery_get)
                .head(discovery_head)
                .post(method_not_allowed),
        )
        .route(
            "/docs/api",
            get(html_get).head(html_head).post(method_not_allowed),
        )
        .route(
            "/api/docs",
            get(html_get).head(html_head).post(method_not_allowed),
        )
        .route(
            "/api-docs",
            get(html_get).head(html_head).post(method_not_allowed),
        )
        .route(
            "/api-docs/",
            get(html_get).head(html_head).post(method_not_allowed),
        )
        .route(
            "/api/docs.json",
            get(catalog_get).head(catalog_head).post(method_not_allowed),
        )
        .route(
            "/api-docs.json",
            get(catalog_get).head(catalog_head).post(method_not_allowed),
        )
        .route(
            "/openapi.json",
            get(openapi_get).head(openapi_head).post(method_not_allowed),
        )
        .route(
            "/openrpc.json",
            get(openrpc_get).head(openrpc_head).post(method_not_allowed),
        )
        .route(
            "/connect.json",
            get(connect_get).head(connect_head).post(method_not_allowed),
        )
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(state)
}

fn apply(kind: BodyKind, extra: &[(&str, &str)], body: Body) -> Response {
    let mut builder = Response::builder().status(StatusCode::OK);
    for (k, v) in hardening_headers(kind)
        .into_iter()
        .chain(extra.iter().copied())
    {
        builder = builder.header(
            HeaderName::from_bytes(k.as_bytes()).expect("header name"),
            HeaderValue::from_str(v).expect("header value"),
        );
    }
    builder.body(body).expect("docs response")
}
