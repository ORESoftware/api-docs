//! Deterministic browser-web request surface classification.
//!
//! The web server must classify an admitted normalized GET/HEAD request path
//! before any filesystem or object-store lookup. A miss inside one surface is
//! terminal; it must never fall through to another surface.

use thiserror::Error;

pub const STATIC_PREFIX: &str = "/static";
pub const INTERNAL_PREFIX: &str = "/_";
pub const DOCS_PREFIX: &str = "/_/docs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRequestMethod {
    Get,
    Head,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSurface {
    Page,
    Static,
    Docs,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WebSurfaceError {
    #[error("web surface classification requires an absolute normalized path")]
    NonAbsolutePath,
    #[error("web surface classification refuses query/fragment text in the normalized path")]
    QueryOrFragment,
    #[error("web surface classification refuses dot segments")]
    DotSegment,
    #[error("web surface classification refuses repeated path separators")]
    RepeatedSeparator,
    #[error("reserved internal web path has no admitted production surface: {path}")]
    ReservedInternalPath { path: String },
    #[error("web page route uses reserved first segment {segment:?}")]
    ReservedPageSegment { segment: String },
}

/// Classify an already-decoded GET/HEAD HTTP path without touching the
/// filesystem. The closed method enum prevents static/docs/page dispatch from
/// silently becoming an authority for mutation methods.
///
/// Reserved namespaces are exact prefix boundaries:
/// - `/static` and `/static/**` -> static asset surface;
/// - `/_/docs` and `/_/docs/**` -> generated docs surface;
/// - all other admitted non-internal paths -> page surface.
///
/// `/_/**` is reserved for framework-owned web surfaces. In v1 only docs is
/// admitted in production; unknown internal paths fail closed instead of
/// falling through to page routing. Dev-only transports are admitted by the
/// `ores-stack dev` host before the production classifier is invoked.
pub fn classify_web_request(
    _method: WebRequestMethod,
    path: &str,
) -> Result<WebSurface, WebSurfaceError> {
    validate_normalized_path(path)?;

    if has_prefix_boundary(path, STATIC_PREFIX) {
        return Ok(WebSurface::Static);
    }
    if has_prefix_boundary(path, DOCS_PREFIX) {
        return Ok(WebSurface::Docs);
    }
    if has_prefix_boundary(path, INTERNAL_PREFIX) {
        return Err(WebSurfaceError::ReservedInternalPath {
            path: path.to_owned(),
        });
    }

    Ok(WebSurface::Page)
}

/// Reject page filesystem roots that could collide with reserved HTTP
/// namespaces. Callers should pass the first URL-producing segment below
/// `src/pages` after applying their filesystem-route grammar.
pub fn validate_page_first_segment(segment: &str) -> Result<(), WebSurfaceError> {
    if segment == "static" || segment == "_" {
        return Err(WebSurfaceError::ReservedPageSegment {
            segment: segment.to_owned(),
        });
    }
    Ok(())
}

fn has_prefix_boundary(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn validate_normalized_path(path: &str) -> Result<(), WebSurfaceError> {
    if !path.starts_with('/') {
        return Err(WebSurfaceError::NonAbsolutePath);
    }
    if path.contains('?') || path.contains('#') {
        return Err(WebSurfaceError::QueryOrFragment);
    }
    if path.len() > 1 && path.contains("//") {
        return Err(WebSurfaceError::RepeatedSeparator);
    }
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(WebSurfaceError::DotSegment);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_disjoint_surfaces_without_fallback() {
        for path in ["/static", "/static/", "/static/images/logo.svg"] {
            assert_eq!(
                classify_web_request(WebRequestMethod::Get, path).unwrap(),
                WebSurface::Static
            );
        }
        for path in ["/_/docs", "/_/docs/", "/_/docs/reference/rpc"] {
            assert_eq!(
                classify_web_request(WebRequestMethod::Head, path).unwrap(),
                WebSurface::Docs
            );
        }
        for path in ["/", "/users/123", "/staticity"] {
            assert_eq!(
                classify_web_request(WebRequestMethod::Get, path).unwrap(),
                WebSurface::Page
            );
        }
        assert_eq!(
            classify_web_request(WebRequestMethod::Get, "/_/dev/ws"),
            Err(WebSurfaceError::ReservedInternalPath {
                path: "/_/dev/ws".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_ambiguous_or_non_normalized_input() {
        assert_eq!(
            classify_web_request(WebRequestMethod::Get, "users/123"),
            Err(WebSurfaceError::NonAbsolutePath)
        );
        assert_eq!(
            classify_web_request(WebRequestMethod::Get, "/users//123"),
            Err(WebSurfaceError::RepeatedSeparator)
        );
        assert_eq!(
            classify_web_request(WebRequestMethod::Get, "/users/../admin"),
            Err(WebSurfaceError::DotSegment)
        );
        assert_eq!(
            classify_web_request(WebRequestMethod::Get, "/users/123?x=1"),
            Err(WebSurfaceError::QueryOrFragment)
        );
    }

    #[test]
    fn reserves_static_and_internal_page_roots() {
        assert!(validate_page_first_segment("users").is_ok());
        assert_eq!(
            validate_page_first_segment("static"),
            Err(WebSurfaceError::ReservedPageSegment {
                segment: "static".to_owned()
            })
        );
        assert_eq!(
            validate_page_first_segment("_"),
            Err(WebSurfaceError::ReservedPageSegment {
                segment: "_".to_owned()
            })
        );
    }
}
