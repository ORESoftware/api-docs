//! Framework-neutral filesystem routing for Rust web and API servers.
//!
//! This module deliberately does **not** turn browser pages into RPC methods.
//! `src/pages/**/page.rs` is a browser-route authoring surface; `src/routes/**/route.rs`
//! is an optional API handler organization surface. The reviewed `RouteMap` remains
//! authoritative for RPC operation identity and wire contracts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsRouteKind {
    Page,
    ApiHandler,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FsRouteSegment {
    Static(String),
    Dynamic(String),
    CatchAll(String),
    OptionalCatchAll(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRoute {
    pub kind: FsRouteKind,
    pub source: String,
    pub segments: Vec<FsRouteSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FsRouteError {
    #[error("route source `{0}` must use forward slashes and cannot escape its route root")]
    InvalidSource(String),
    #[error("route source `{source}` must end in `{expected_leaf}`")]
    WrongLeaf { source: String, expected_leaf: &'static str },
    #[error("invalid route segment `{segment}` in `{source}`")]
    InvalidSegment { source: String, segment: String },
    #[error("duplicate route parameter `{parameter}` in `{source}`")]
    DuplicateParameter { source: String, parameter: String },
    #[error("catch-all segment `{segment}` in `{source}` must be the final route segment")]
    CatchAllNotFinal { source: String, segment: String },
    #[error("filesystem routes conflict: `{left}` and `{right}` both match `{shape}`")]
    Conflict { left: String, right: String, shape: String },
}

impl FsRoute {
    pub fn page(source: impl Into<String>) -> Result<Self, FsRouteError> {
        Self::parse(FsRouteKind::Page, source.into(), "src/pages", "page.rs")
    }

    pub fn api_handler(source: impl Into<String>) -> Result<Self, FsRouteError> {
        Self::parse(FsRouteKind::ApiHandler, source.into(), "src/routes", "route.rs")
    }

    fn parse(kind: FsRouteKind, source: String, route_root: &str, expected_leaf: &'static str) -> Result<Self, FsRouteError> {
        if source.contains('\\') || source.split('/').any(|part| part == "..") {
            return Err(FsRouteError::InvalidSource(source));
        }
        let prefix = format!("{route_root}/");
        let relative = source.strip_prefix(&prefix).ok_or_else(|| FsRouteError::InvalidSource(source.clone()))?;
        let mut parts: Vec<&str> = relative.split('/').collect();
        if parts.pop() != Some(expected_leaf) {
            return Err(FsRouteError::WrongLeaf { source, expected_leaf });
        }

        let mut seen = BTreeSet::new();
        let mut segments = Vec::with_capacity(parts.len());
        for (index, raw) in parts.iter().enumerate() {
            let segment = parse_segment(&source, raw)?;
            if let Some(name) = segment.parameter_name() {
                if !seen.insert(name.to_owned()) {
                    return Err(FsRouteError::DuplicateParameter { source, parameter: name.to_owned() });
                }
            }
            if matches!(segment, FsRouteSegment::CatchAll(_) | FsRouteSegment::OptionalCatchAll(_)) && index + 1 != parts.len() {
                return Err(FsRouteError::CatchAllNotFinal { source, segment: (*raw).to_owned() });
            }
            segments.push(segment);
        }
        Ok(Self { kind, source, segments })
    }

    /// Stable public URL template. Dynamic parameters use RFC6570/OpenAPI-style
    /// braces so API handler files can be compared directly with `api-docs` paths.
    pub fn canonical_path(&self) -> String {
        if self.segments.is_empty() { return "/".to_owned(); }
        let mut out = String::new();
        for segment in &self.segments {
            out.push('/');
            match segment {
                FsRouteSegment::Static(value) => out.push_str(value),
                FsRouteSegment::Dynamic(name) => { out.push('{'); out.push_str(name); out.push('}'); }
                FsRouteSegment::CatchAll(name) => { out.push_str("{*"); out.push_str(name); out.push('}'); }
                FsRouteSegment::OptionalCatchAll(name) => { out.push_str("{*"); out.push_str(name); out.push_str("?}"); }
            }
        }
        out
    }

    /// Axum 0.8 route syntax. Optional catch-all expands to parent + wildcard.
    pub fn axum_paths(&self) -> Vec<String> {
        let render = |segments: &[FsRouteSegment]| {
            if segments.is_empty() { return "/".to_owned(); }
            let mut out = String::new();
            for segment in segments {
                out.push('/');
                match segment {
                    FsRouteSegment::Static(value) => out.push_str(value),
                    FsRouteSegment::Dynamic(name) => { out.push('{'); out.push_str(name); out.push('}'); }
                    FsRouteSegment::CatchAll(name) | FsRouteSegment::OptionalCatchAll(name) => { out.push_str("{*"); out.push_str(name); out.push('}'); }
                }
            }
            out
        };
        if matches!(self.segments.last(), Some(FsRouteSegment::OptionalCatchAll(_))) {
            vec![render(&self.segments[..self.segments.len() - 1]), render(&self.segments)]
        } else { vec![render(&self.segments)] }
    }

    /// Dioxus Router 0.7 syntax (`:id`, `:..segments`).
    pub fn dioxus_paths(&self) -> Vec<String> {
        let render = |segments: &[FsRouteSegment]| {
            if segments.is_empty() { return "/".to_owned(); }
            let mut out = String::new();
            for segment in segments {
                out.push('/');
                match segment {
                    FsRouteSegment::Static(value) => out.push_str(value),
                    FsRouteSegment::Dynamic(name) => { out.push(':'); out.push_str(name); }
                    FsRouteSegment::CatchAll(name) | FsRouteSegment::OptionalCatchAll(name) => { out.push_str(":.."); out.push_str(name); }
                }
            }
            out
        };
        if matches!(self.segments.last(), Some(FsRouteSegment::OptionalCatchAll(_))) {
            vec![render(&self.segments[..self.segments.len() - 1]), render(&self.segments)]
        } else { vec![render(&self.segments)] }
    }

    /// Matching shape for deterministic conflict detection. Parameter names do
    /// not make otherwise-identical dynamic siblings distinct.
    pub fn match_shape(&self) -> String {
        if self.segments.is_empty() { return "/".to_owned(); }
        let mut out = String::new();
        for segment in &self.segments {
            out.push('/');
            match segment {
                FsRouteSegment::Static(value) => out.push_str(value),
                FsRouteSegment::Dynamic(_) => out.push_str("{}"),
                FsRouteSegment::CatchAll(_) => out.push_str("{*}"),
                FsRouteSegment::OptionalCatchAll(_) => out.push_str("{*?}"),
            }
        }
        out
    }

    pub fn precedence_key(&self) -> Vec<u8> {
        self.segments.iter().map(|segment| match segment {
            FsRouteSegment::Static(_) => 0,
            FsRouteSegment::Dynamic(_) => 1,
            FsRouteSegment::CatchAll(_) => 2,
            FsRouteSegment::OptionalCatchAll(_) => 3,
        }).collect()
    }
}

impl FsRouteSegment {
    pub fn parameter_name(&self) -> Option<&str> {
        match self {
            Self::Static(_) => None,
            Self::Dynamic(name) | Self::CatchAll(name) | Self::OptionalCatchAll(name) => Some(name),
        }
    }
}

fn parse_segment(source: &str, raw: &str) -> Result<FsRouteSegment, FsRouteError> {
    if raw.is_empty() || raw == "." || raw.contains(['{', '}', ':', '*']) {
        return Err(FsRouteError::InvalidSegment { source: source.to_owned(), segment: raw.to_owned() });
    }
    let (kind, name) = if let Some(name) = raw.strip_prefix("[[...").and_then(|v| v.strip_suffix("]]")) {
        ("optional", name)
    } else if let Some(name) = raw.strip_prefix("[...").and_then(|v| v.strip_suffix(']')) {
        ("catch_all", name)
    } else if let Some(name) = raw.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        ("dynamic", name)
    } else {
        return Ok(FsRouteSegment::Static(raw.to_owned()));
    };
    if name.is_empty() || !name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        return Err(FsRouteError::InvalidSegment { source: source.to_owned(), segment: raw.to_owned() });
    }
    Ok(match kind {
        "dynamic" => FsRouteSegment::Dynamic(name.to_owned()),
        "catch_all" => FsRouteSegment::CatchAll(name.to_owned()),
        "optional" => FsRouteSegment::OptionalCatchAll(name.to_owned()),
        _ => unreachable!(),
    })
}

/// Fail closed on ambiguous authored files and return stable route ordering:
/// static before dynamic before catch-all, then canonical path, then source.
pub fn validate_and_sort_fs_routes(routes: impl IntoIterator<Item = FsRoute>) -> Result<Vec<FsRoute>, FsRouteError> {
    let mut by_shape: BTreeMap<String, String> = BTreeMap::new();
    let mut routes: Vec<FsRoute> = routes.into_iter().collect();
    for route in &routes {
        let shape = route.match_shape();
        if let Some(previous) = by_shape.insert(shape.clone(), route.source.clone()) {
            return Err(FsRouteError::Conflict { left: previous, right: route.source.clone(), shape });
        }
    }
    routes.sort_by(|left, right| left.precedence_key().cmp(&right.precedence_key())
        .then_with(|| left.canonical_path().cmp(&right.canonical_path()))
        .then_with(|| left.source.cmp(&right.source)));
    Ok(routes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_style_page_paths_are_framework_neutral() {
        let route = FsRoute::page("src/pages/orgs/[org_id]/packages/[...slug]/page.rs").unwrap();
        assert_eq!(route.canonical_path(), "/orgs/{org_id}/packages/{*slug}");
        assert_eq!(route.axum_paths(), ["/orgs/{org_id}/packages/{*slug}"]);
        assert_eq!(route.dioxus_paths(), ["/orgs/:org_id/packages/:..slug"]);
    }

    #[test]
    fn api_route_files_can_be_cross_checked_against_route_maps() {
        let route = FsRoute::api_handler("src/routes/v1/matters/[id]/route.rs").unwrap();
        assert_eq!(route.canonical_path(), "/v1/matters/{id}");
    }

    #[test]
    fn optional_catch_all_expands_to_parent_and_wildcard() {
        let route = FsRoute::page("src/pages/docs/[[...slug]]/page.rs").unwrap();
        assert_eq!(route.axum_paths(), ["/docs", "/docs/{*slug}"]);
        assert_eq!(route.dioxus_paths(), ["/docs", "/docs/:..slug"]);
    }

    #[test]
    fn dynamic_sibling_names_do_not_hide_conflicts() {
        let err = validate_and_sort_fs_routes([
            FsRoute::page("src/pages/users/[id]/page.rs").unwrap(),
            FsRoute::page("src/pages/users/[slug]/page.rs").unwrap(),
        ]).unwrap_err();
        assert!(matches!(err, FsRouteError::Conflict { .. }));
    }

    #[test]
    fn static_routes_sort_before_dynamic_and_catch_all() {
        let routes = validate_and_sort_fs_routes([
            FsRoute::page("src/pages/users/[...rest]/page.rs").unwrap(),
            FsRoute::page("src/pages/users/[id]/page.rs").unwrap(),
            FsRoute::page("src/pages/users/new/page.rs").unwrap(),
        ]).unwrap();
        let paths: Vec<_> = routes.iter().map(FsRoute::canonical_path).collect();
        assert_eq!(paths, ["/users/new", "/users/{id}", "/users/{*rest}"]);
    }
}
