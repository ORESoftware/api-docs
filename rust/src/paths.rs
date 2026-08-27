//! Canonical public docs aliases (k8s-cluster generate-api-docs.mjs).

pub const STANDARD_DOCS_ROUTES: &[&str] = &["/docs/api", "/api/docs", "/api/docs.json"];

pub const CLUSTER_ALIAS_ROUTES: &[&str] = &["/api-docs", "/api-docs.json"];

pub const OPENAPI_ROUTE: &str = "/openapi.json";
pub const OPENRPC_ROUTE: &str = "/openrpc.json";
pub const CONNECT_ROUTE: &str = "/connect.json";

#[must_use]
pub fn all_public_get_paths() -> Vec<&'static str> {
    let mut v = Vec::from(STANDARD_DOCS_ROUTES);
    v.extend_from_slice(CLUSTER_ALIAS_ROUTES);
    v.push(OPENAPI_ROUTE);
    v.push(OPENRPC_ROUTE);
    v.push(CONNECT_ROUTE);
    v
}

#[must_use]
pub fn is_html_path(path: &str) -> bool {
    matches!(path, "/docs/api" | "/api/docs" | "/api-docs" | "/api-docs/")
}

#[must_use]
pub fn is_json_path(path: &str) -> bool {
    matches!(
        path,
        "/api/docs.json"
            | "/api-docs.json"
            | "/openapi.json"
            | "/openrpc.json"
            | "/connect.json"
    )
}
