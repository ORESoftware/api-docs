#[path = "../src/api_surface.rs"]
mod api_surface;

use api_surface::{
    classify_api_surface, validate_rest_projection, ApiSurface, ApiSurfaceError,
};

#[test]
fn explicit_namespaces_are_disjoint() {
    assert_eq!(
        classify_api_surface("/static/logo.svg", Some("/ws")).unwrap(),
        ApiSurface::Static {
            relative_path: "logo.svg"
        }
    );
    assert_eq!(
        classify_api_surface("/_/docs/rpc", Some("/ws")).unwrap(),
        ApiSurface::Docs {
            relative_path: "rpc"
        }
    );
    assert_eq!(
        classify_api_surface("/_/admin/cache", Some("/ws")).unwrap(),
        ApiSurface::Admin {
            relative_path: "cache"
        }
    );
    assert_eq!(
        classify_api_surface("/rest/users/42", Some("/ws")).unwrap(),
        ApiSurface::Rest {
            relative_path: "users/42"
        }
    );
    assert_eq!(
        classify_api_surface("/v1/rpc", Some("/ws")).unwrap(),
        ApiSurface::Rpc
    );
    assert_eq!(
        classify_api_surface("/v1/graphql", Some("/ws")).unwrap(),
        ApiSurface::Graphql
    );
    assert_eq!(
        classify_api_surface("/ws", Some("/ws")).unwrap(),
        ApiSurface::WebSocket
    );
}

#[test]
fn rest_is_never_a_fallback() {
    assert!(matches!(
        classify_api_surface("/users/42", Some("/ws")),
        Err(ApiSurfaceError::UnknownApiPath { .. })
    ));
    assert!(validate_rest_projection("/rest/users/42", Some("/ws")).is_ok());
    assert!(validate_rest_projection("/users/42", Some("/ws")).is_err());
}

#[test]
fn misses_inside_reserved_surfaces_are_terminal() {
    assert!(matches!(
        classify_api_surface("/_/unknown", Some("/ws")),
        Err(ApiSurfaceError::UnknownInternalPath { .. })
    ));
    assert!(matches!(
        classify_api_surface("/v1/rpc/unknown", Some("/ws")),
        Err(ApiSurfaceError::UnknownProtocolPath { .. })
    ));
    assert!(matches!(
        classify_api_surface("/ws/unknown", Some("/ws")),
        Err(ApiSurfaceError::UnknownProtocolPath { .. })
    ));
}

#[test]
fn websocket_cannot_steal_a_reserved_surface() {
    for ws in [
        "/rest",
        "/rest/ws",
        "/static/ws",
        "/_/ws",
        "/v1/rpc",
        "/v1/graphql",
    ] {
        assert!(matches!(
            classify_api_surface("/rest/users", Some(ws)),
            Err(ApiSurfaceError::InvalidWebSocketPath { .. })
        ));
    }
}
