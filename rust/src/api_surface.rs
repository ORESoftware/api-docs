//! Canonical `*-api-server.rs` HTTP surface classification.
//!
//! The public namespace is explicit. REST is never inferred as a catch-all:
//! only `/rest` and `/rest/**` select REST. Classification occurs before any
//! filesystem or semantic-source lookup.

use thiserror::Error;

pub const STATIC_HTTP_PREFIX: &str = "/static";
pub const INTERNAL_HTTP_PREFIX: &str = "/_";
pub const DOCS_HTTP_PREFIX: &str = "/_/docs";
pub const ADMIN_HTTP_PREFIX: &str = "/_/admin";
pub const REST_HTTP_PREFIX: &str = "/rest";
pub const RPC_HTTP_PATH: &str = "/v1/rpc";
pub const GRAPHQL_HTTP_PATH: &str = "/v1/graphql";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiSurface<'a> {
    Static { relative_path: &'a str },
    Docs { relative_path: &'a str },
    Admin { relative_path: &'a str },
    Rest { relative_path: &'a str },
    Rpc,
    Graphql,
    WebSocket,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiSurfaceError {
    #[error("API path must be absolute")]
    NonAbsolutePath,
    #[error("API classifier requires path-only input without query/fragment")]
    QueryOrFragment,
    #[error("API path contains a dot segment")]
    DotSegment,
    #[error("API path contains repeated separators")]
    RepeatedSeparator,
    #[error("configured WebSocket path collides with a reserved API namespace: {path}")]
    InvalidWebSocketPath { path: String },
    #[error("reserved internal API path has no admitted surface: {path}")]
    UnknownInternalPath { path: String },
    #[error("reserved protocol path has no admitted endpoint: {path}")]
    UnknownProtocolPath { path: String },
    #[error("API path belongs to no explicit surface: {path}")]
    UnknownApiPath { path: String },
    #[error("REST projection must live under /rest: {path}")]
    InvalidRestProjection { path: String },
}

/// Classify one normalized API request before any resource lookup.
///
/// # Errors
///
/// Rejects malformed paths, invalid WebSocket configuration, unknown reserved
/// subpaths, and any path outside the explicit API namespaces.
pub fn classify_api_surface<'a>(
    path: &'a str,
    websocket_path: Option<&str>,
) -> Result<ApiSurface<'a>, ApiSurfaceError> {
    validate_path(path)?;
    validate_websocket_path(websocket_path)?;

    if let Some(relative_path) = strip_prefix(path, STATIC_HTTP_PREFIX) {
        return Ok(ApiSurface::Static { relative_path });
    }
    if let Some(relative_path) = strip_prefix(path, DOCS_HTTP_PREFIX) {
        return Ok(ApiSurface::Docs { relative_path });
    }
    if let Some(relative_path) = strip_prefix(path, ADMIN_HTTP_PREFIX) {
        return Ok(ApiSurface::Admin { relative_path });
    }
    if has_prefix(path, INTERNAL_HTTP_PREFIX) {
        return Err(ApiSurfaceError::UnknownInternalPath {
            path: path.to_owned(),
        });
    }
    if let Some(relative_path) = strip_prefix(path, REST_HTTP_PREFIX) {
        return Ok(ApiSurface::Rest { relative_path });
    }
    if path == RPC_HTTP_PATH {
        return Ok(ApiSurface::Rpc);
    }
    if has_prefix(path, RPC_HTTP_PATH) {
        return Err(ApiSurfaceError::UnknownProtocolPath {
            path: path.to_owned(),
        });
    }
    if path == GRAPHQL_HTTP_PATH {
        return Ok(ApiSurface::Graphql);
    }
    if has_prefix(path, GRAPHQL_HTTP_PATH) {
        return Err(ApiSurfaceError::UnknownProtocolPath {
            path: path.to_owned(),
        });
    }
    if let Some(ws) = websocket_path {
        if path == ws {
            return Ok(ApiSurface::WebSocket);
        }
        if has_prefix(path, ws) {
            return Err(ApiSurfaceError::UnknownProtocolPath {
                path: path.to_owned(),
            });
        }
    }

    Err(ApiSurfaceError::UnknownApiPath {
        path: path.to_owned(),
    })
}

/// Build-time guard for public REST projections.
///
/// # Errors
///
/// Rejects any projection outside the explicit `/rest` namespace.
pub fn validate_rest_projection(
    path: &str,
    websocket_path: Option<&str>,
) -> Result<(), ApiSurfaceError> {
    match classify_api_surface(path, websocket_path)? {
        ApiSurface::Rest { .. } => Ok(()),
        _ => Err(ApiSurfaceError::InvalidRestProjection {
            path: path.to_owned(),
        }),
    }
}

fn validate_websocket_path(path: Option<&str>) -> Result<(), ApiSurfaceError> {
    let Some(path) = path else {
        return Ok(());
    };
    if validate_path(path).is_err()
        || path == "/"
        || has_prefix(path, STATIC_HTTP_PREFIX)
        || has_prefix(path, INTERNAL_HTTP_PREFIX)
        || has_prefix(path, REST_HTTP_PREFIX)
        || has_prefix(path, RPC_HTTP_PATH)
        || has_prefix(path, GRAPHQL_HTTP_PATH)
    {
        return Err(ApiSurfaceError::InvalidWebSocketPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn strip_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if path == prefix {
        return Some("");
    }
    path.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
}

fn has_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn validate_path(path: &str) -> Result<(), ApiSurfaceError> {
    if !path.starts_with('/') {
        return Err(ApiSurfaceError::NonAbsolutePath);
    }
    if path.contains('?') || path.contains('#') {
        return Err(ApiSurfaceError::QueryOrFragment);
    }
    if path.len() > 1 && path.contains("//") {
        return Err(ApiSurfaceError::RepeatedSeparator);
    }
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(ApiSurfaceError::DotSegment);
    }
    Ok(())
}
