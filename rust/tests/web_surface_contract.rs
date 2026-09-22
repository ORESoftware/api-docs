use ores_api_docs::web_surface::{
    classify_web_request, validate_page_first_segment, WebRequestMethod, WebSurface,
    WebSurfaceError,
};

#[test]
fn contract_keeps_surface_selection_disjoint() {
    assert_eq!(
        classify_web_request(WebRequestMethod::Get, "/static/a.svg").unwrap(),
        WebSurface::Static
    );
    assert_eq!(
        classify_web_request(WebRequestMethod::Head, "/_/docs/rpc").unwrap(),
        WebSurface::Docs
    );
    assert_eq!(
        classify_web_request(WebRequestMethod::Get, "/users/42").unwrap(),
        WebSurface::Page
    );
}

#[test]
fn contract_reserves_page_roots_and_internal_namespace() {
    assert_eq!(
        validate_page_first_segment("static"),
        Err(WebSurfaceError::ReservedPageSegment {
            segment: "static".to_owned(),
        })
    );
    assert!(validate_page_first_segment("users").is_ok());
    assert_eq!(
        classify_web_request(WebRequestMethod::Get, "/_/unknown"),
        Err(WebSurfaceError::ReservedInternalPath {
            path: "/_/unknown".to_owned(),
        })
    );
}
