//! Canonical public docs aliases (k8s-cluster generate-api-docs.mjs).

pub const DISCOVERY_ROUTE: &str = "/api-docs/manifest.json";
pub const HTML_ROUTE: &str = "/docs/api";
pub const CATALOG_ROUTE: &str = "/api/docs.json";

pub const STANDARD_DOCS_ROUTES: &[&str] = &[HTML_ROUTE, "/api/docs", CATALOG_ROUTE];

pub const CLUSTER_ALIAS_ROUTES: &[&str] = &["/api-docs", "/api-docs/", "/api-docs.json"];

pub const DOCS_ALIAS_ROUTES: &[&str] = &[
    "/api/docs",
    "/api-docs",
    "/api-docs/",
    "/api-docs.json",
];

pub const OPENAPI_ROUTE: &str = "/openapi.json";
pub const OPENRPC_ROUTE: &str = "/openrpc.json";
pub const CONNECT_ROUTE: &str = "/connect.json";

#[must_use]
pub fn all_public_get_paths() -> Vec<&'static str> {
    let mut v = vec![DISCOVERY_ROUTE];
    v.extend_from_slice(STANDARD_DOCS_ROUTES);
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
        "/api-docs/manifest.json"
            | "/api/docs.json"
            | "/api-docs.json"
            | "/openapi.json"
            | "/openrpc.json"
            | "/connect.json"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_inventory_is_unique_and_classified() {
        let paths = all_public_get_paths();
        let unique = paths.iter().copied().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(paths.len(), unique.len());
        assert!(paths.iter().all(|path| is_html_path(path) || is_json_path(path)));
        assert!(is_json_path(DISCOVERY_ROUTE));
        assert!(is_html_path(HTML_ROUTE));
        assert!(is_json_path(CATALOG_ROUTE));
    }
}
