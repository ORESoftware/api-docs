use ores_api_docs::{classify_web_surface, validate_page_first_segment, WebSurface, WebSurfaceError};

#[test]
fn public_contract_keeps_surface_selection_disjoint() {
    assert_eq!(classify_web_surface("/static/a.svg").unwrap(), WebSurface::Static);
    assert_eq!(classify_web_surface("/_/docs/rpc").unwrap(), WebSurface::Docs);
    assert_eq!(classify_web_surface("/users/42").unwrap(), WebSurface::Page);
}

#[test]
fn public_contract_reserves_page_roots() {
    assert_eq!(
        validate_page_first_segment("static"),
        Err(WebSurfaceError::ReservedPageSegment {
            segment: "static".to_owned(),
        })
    );
    assert!(validate_page_first_segment("users").is_ok());
}
