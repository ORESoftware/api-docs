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

#[must_use]
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
        .with_state(state)
}

fn apply(kind: BodyKind, extra: &[(&str, &str)], body: Body) -> Response {
    let mut builder = Response::builder().status(StatusCode::OK);
    for (k, v) in hardening_headers(kind).into_iter().chain(extra.iter().copied()) {
        builder = builder.header(
            HeaderName::from_bytes(k.as_bytes()).expect("header name"),
            HeaderValue::from_str(v).expect("header value"),
        );
    }
    builder.body(body).expect("docs response")
}

async fn discovery_get(State(cat): State<Arc<Catalog>>) -> Response {
    match DocsDiscoveryManifest::from_catalog(cat.as_ref()).to_pretty_json() {
        Ok(body) => apply(BodyKind::Json, &[], Body::from(body)),
        Err(_) => encode_failed(),
    }
}

async fn discovery_head(State(_cat): State<Arc<Catalog>>) -> Response {
    apply(BodyKind::Json, &[], Body::empty())
}

async fn html_get(State(cat): State<Arc<Catalog>>) -> Response {
    apply(BodyKind::Html, &[], Body::from(render_html(&cat)))
}

async fn html_head(State(cat): State<Arc<Catalog>>) -> Response {
    let _ = cat;
    apply(BodyKind::Html, &[], Body::empty())
}

async fn catalog_get(State(cat): State<Arc<Catalog>>) -> Response {
    match cat.to_pretty_json() {
        Ok(body) => apply(BodyKind::Json, &[], Body::from(body)),
        Err(_) => encode_failed(),
    }
}

async fn catalog_head(State(_cat): State<Arc<Catalog>>) -> Response {
    apply(BodyKind::Json, &[], Body::empty())
}

async fn openapi_get(State(cat): State<Arc<Catalog>>) -> Response {
    match serde_json::to_string_pretty(&cat.openapi) {
        Ok(body) => apply(BodyKind::Json, &[], Body::from(body)),
        Err(_) => encode_failed(),
    }
}

async fn openapi_head(State(_cat): State<Arc<Catalog>>) -> Response {
    apply(BodyKind::Json, &[], Body::empty())
}

async fn openrpc_get(State(cat): State<Arc<Catalog>>) -> Response {
    match serde_json::to_string_pretty(&cat.openrpc) {
        Ok(body) => apply(BodyKind::Json, &[], Body::from(body)),
        Err(_) => encode_failed(),
    }
}

async fn openrpc_head(State(_cat): State<Arc<Catalog>>) -> Response {
    apply(BodyKind::Json, &[], Body::empty())
}

async fn connect_get(State(cat): State<Arc<Catalog>>) -> Response {
    match serde_json::to_string_pretty(&cat.connect) {
        Ok(body) => apply(BodyKind::Json, &[], Body::from(body)),
        Err(_) => encode_failed(),
    }
}

async fn connect_head(State(_cat): State<Arc<Catalog>>) -> Response {
    apply(BodyKind::Json, &[], Body::empty())
}

fn encode_failed() -> Response {
    let mut builder = Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR);
    for (k, v) in hardening_headers(BodyKind::Json) {
        builder = builder.header(
            HeaderName::from_bytes(k.as_bytes()).expect("header name"),
            HeaderValue::from_str(v).expect("header value"),
        );
    }
    builder
        .body(Body::from("{\"ok\":false,\"error\":\"encode_failed\"}"))
        .expect("500")
}

async fn method_not_allowed() -> Response {
    let mut builder = Response::builder().status(StatusCode::METHOD_NOT_ALLOWED);
    for (k, v) in method_not_allowed_headers() {
        builder = builder.header(
            HeaderName::from_bytes(k.as_bytes()).expect("header name"),
            HeaderValue::from_str(v).expect("header value"),
        );
    }
    builder
        .body(Body::from("{\"ok\":false,\"error\":\"method_not_allowed\"}"))
        .expect("405")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::RouteMap;
    use axum::http::HeaderMap;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app() -> Router {
        let map = RouteMap::from_json_str(include_str!("../../examples/pmap-api.route-map.json"))
            .unwrap();
        router(Catalog::from_map(map).unwrap())
    }

    async fn call(method: &str, path: &str) -> (StatusCode, HeaderMap, bytes::Bytes) {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", "Bearer secret-token")
            .body(Body::empty())
            .unwrap();
        let res = app().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (status, headers, body)
    }

    #[tokio::test]
    async fn html_aliases_and_head() {
        for path in ["/docs/api", "/api/docs", "/api-docs", "/api-docs/"] {
            let (st, h, body) = call("GET", path).await;
            assert_eq!(st, StatusCode::OK, "{path}");
            assert_eq!(h.get("cache-control").unwrap(), "no-store");
            assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
            assert!(h
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'"));
            assert!(std::str::from_utf8(&body)
                .unwrap()
                .contains("CheckFieldSanity"));
            let (st, h, body) = call("HEAD", path).await;
            assert_eq!(st, StatusCode::OK);
            assert_eq!(h.get("cache-control").unwrap(), "no-store");
            assert!(body.is_empty());
        }
    }

    #[tokio::test]
    async fn discovery_is_relative_digest_bound_and_hardened() {
        let (st, h, body) = call("GET", "/api-docs/manifest.json").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h.get("cache-control").unwrap(), "no-store");
        assert!(h
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("application/json"));
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["schemaVersion"], "1.0.0");
        assert_eq!(value["service"], "pmap-api");
        assert_eq!(value["discovery"], "/api-docs/manifest.json");
        assert_eq!(value["html"], "/docs/api");
        assert_eq!(value["catalog"], "/api/docs.json");
        assert_eq!(value["projections"]["openapi"], "/openapi.json");
        assert_eq!(value["contractSha256"].as_str().unwrap().len(), 64);
        assert!(!std::str::from_utf8(&body).unwrap().contains("secret-token"));

        let (st, h, body) = call("HEAD", "/api-docs/manifest.json").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h.get("cache-control").unwrap(), "no-store");
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn json_catalog_and_projections() {
        let (st, h, body) = call("GET", "/api/docs.json").await;
        assert_eq!(st, StatusCode::OK);
        assert!(h
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("application/json"));
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["map"]["AskCounsel"]["path"], "/pmap.v1.Interview/AskCounsel");
        let (st, _, body) = call("GET", "/openapi.json").await;
        assert_eq!(st, StatusCode::OK);
        let oa: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(oa["openapi"], "3.1.0");
        let (st, _, body) = call("GET", "/connect.json").await;
        assert_eq!(st, StatusCode::OK);
        let c: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(c["protocol"], "connect");
        assert_eq!(c["codec"], "json");
    }

    #[tokio::test]
    async fn post_is_405_without_reflecting_credentials() {
        for path in ["/docs/api", "/api-docs/manifest.json"] {
            let (st, h, body) = call("POST", path).await;
            assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(h.get("allow").unwrap(), "GET, HEAD");
            let s = std::str::from_utf8(&body).unwrap();
            assert!(!s.contains("secret-token"));
            assert!(!s.contains("Bearer"));
            assert!(!format!("{h:?}").contains("secret-token"));
        }
    }
}
