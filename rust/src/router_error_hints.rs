#![forbid(unsafe_code)]

//! Deterministic route/path hints for framework-generated HTTP errors.
//!
//! Callers pass the already-admitted in-memory route inventory. This module
//! never reads the filesystem and deliberately filters non-disclosable/internal
//! candidates before ranking suggestions.

use serde::Serialize;

const MAX_SUGGESTIONS: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterHintCandidate<'a> {
    pub path: &'a str,
    pub methods: &'a [&'a str],
    /// Whether this route is safe to disclose to the current caller.
    pub disclose: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RouterSuggestion {
    pub path: String,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RouterErrorEnvelope {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub suggestions: Vec<RouterSuggestion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterMissClassification {
    pub status: u16,
    pub allow: Vec<String>,
}

#[must_use]
pub fn router_error_envelope(
    status: u16,
    method: &str,
    path: &str,
    candidates: &[RouterHintCandidate<'_>],
) -> RouterErrorEnvelope {
    let (code, message) = match status {
        404 => ("not_found", "route not found"),
        405 => ("method_not_allowed", "method not allowed"),
        400..=499 => ("request_error", "request failed"),
        500..=599 => ("server_error", "server error"),
        _ => ("http_error", "http error"),
    };
    let suggestions = if status >= 400 {
        router_suggestions(status, method, path, candidates)
    } else {
        Vec::new()
    };
    RouterErrorEnvelope {
        status,
        code: code.to_owned(),
        message: message.to_owned(),
        suggestions,
    }
}

pub fn router_error_json(
    status: u16,
    method: &str,
    path: &str,
    candidates: &[RouterHintCandidate<'_>],
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&router_error_envelope(status, method, path, candidates))
}

/// Classify a framework-owned router miss without consulting the filesystem.
///
/// A caller-visible route-template match with a different admitted method is a
/// 405 and returns the deterministic method set for the `Allow` header. Hidden
/// candidates are deliberately ignored so classification itself cannot disclose
/// that a protected/internal route exists. If the requested method is already
/// admitted for the matching path, the fallback remains a 404: some deeper
/// routing/admission layer, rather than the method table, caused the miss.
#[must_use]
pub fn classify_router_miss(
    method: &str,
    path: &str,
    candidates: &[RouterHintCandidate<'_>],
) -> RouterMissClassification {
    let requested_method = method.trim().to_ascii_uppercase();
    let mut allow = candidates
        .iter()
        .filter(|candidate| candidate.disclose && safe_public_path(candidate.path))
        .filter(|candidate| route_template_matches(candidate.path, path))
        .flat_map(|candidate| candidate.methods.iter())
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    allow.sort();
    allow.dedup();

    if !allow.is_empty() && !allow.iter().any(|value| value == &requested_method) {
        RouterMissClassification { status: 405, allow }
    } else {
        RouterMissClassification {
            status: 404,
            allow: Vec::new(),
        }
    }
}

/// Match a request path against the ORES/Axum route-template grammar used by
/// generated route inventories. Dynamic `{name}` segments match one segment;
/// a terminal `{*name}` catch-all matches one or more trailing segments.
/// Malformed templates fail closed by returning `false`.
#[must_use]
pub fn route_template_matches(template: &str, request_path: &str) -> bool {
    if !template.starts_with('/') || !request_path.starts_with('/') {
        return false;
    }

    let template = normalize_path(template);
    let request_path = normalize_path(request_path);
    let template_parts = path_segments(&template);
    let request_parts = path_segments(&request_path);
    let mut request_index = 0usize;

    for (index, part) in template_parts.iter().enumerate() {
        if let Some(name) = catch_all_name(part) {
            return !name.is_empty()
                && index + 1 == template_parts.len()
                && request_index < request_parts.len();
        }

        let Some(actual) = request_parts.get(request_index) else {
            return false;
        };
        if let Some(name) = capture_name(part) {
            if name.is_empty() || name.starts_with('*') {
                return false;
            }
        } else if part.contains('{') || part.contains('}') {
            return false;
        } else if part != actual {
            return false;
        }
        request_index += 1;
    }

    request_index == request_parts.len()
}

#[must_use]
pub fn router_suggestions(
    status: u16,
    method: &str,
    path: &str,
    candidates: &[RouterHintCandidate<'_>],
) -> Vec<RouterSuggestion> {
    let path = normalize_path(path);
    let method = method.trim().to_ascii_uppercase();
    let request_segments = path_segments(&path);
    let mut ranked = candidates
        .iter()
        .filter(|candidate| candidate.disclose && safe_public_path(candidate.path))
        .filter_map(|candidate| {
            let candidate_path = normalize_path(candidate.path);
            let mut methods = candidate
                .methods
                .iter()
                .map(|value| value.trim().to_ascii_uppercase())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            methods.sort();
            methods.dedup();
            if methods.is_empty() {
                return None;
            }

            let candidate_segments = path_segments(&candidate_path);
            let exact_path_penalty = usize::from(candidate_path != path);
            let catch_all_shape_match = candidate_segments
                .last()
                .is_some_and(|segment| catch_all_name(segment).is_some())
                && request_segments.len() >= candidate_segments.len();
            let segment_count_penalty = if catch_all_shape_match {
                0
            } else {
                request_segments.len().abs_diff(candidate_segments.len())
            };
            let path_distance = route_template_distance(&request_segments, &candidate_segments);
            let method_penalty = usize::from(!methods.iter().any(|value| value == &method));

            // A router suggestion should prefer a route with the same path shape
            // over a shorter lexical prefix. In particular `/users/42` should
            // prefer `/users/{id}` over `/users` even if raw edit distance would
            // choose the latter. For 405, exact-path identity is authoritative.
            let score = if status == 405 {
                (
                    exact_path_penalty,
                    segment_count_penalty,
                    path_distance,
                    method_penalty,
                )
            } else {
                (
                    segment_count_penalty,
                    path_distance,
                    method_penalty,
                    exact_path_penalty,
                )
            };
            Some((
                score,
                candidate_path.clone(),
                RouterSuggestion {
                    path: candidate_path,
                    methods,
                },
            ))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    ranked.dedup_by(|left, right| left.2 == right.2);
    ranked
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, _, suggestion)| suggestion)
        .collect()
}

fn safe_public_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("/_/admin")
        && !path.starts_with("/__ores")
        && !path.contains("..")
        && !path.contains('\\')
}

fn normalize_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or(path).trim();
    if path.is_empty() {
        return "/".to_owned();
    }
    let mut normalized = String::new();
    if !path.starts_with('/') {
        normalized.push('/');
    }
    let mut slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if slash {
                continue;
            }
            slash = true;
        } else {
            slash = false;
        }
        normalized.push(ch);
    }
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn capture_name(segment: &str) -> Option<&str> {
    segment
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
}

fn catch_all_name(segment: &str) -> Option<&str> {
    segment
        .strip_prefix("{*")
        .and_then(|value| value.strip_suffix('}'))
}

fn route_template_distance(request: &[&str], candidate: &[&str]) -> usize {
    let shared = request.len().min(candidate.len());
    let mut distance = 0;
    for index in 0..shared {
        let candidate_segment = candidate[index];
        if catch_all_name(candidate_segment).is_some() {
            return distance;
        }
        if is_capture(candidate_segment) {
            continue;
        }
        distance += levenshtein(request[index], candidate_segment);
    }
    for segment in request.iter().skip(shared) {
        distance += segment.len().max(1);
    }
    for segment in candidate.iter().skip(shared) {
        distance += if is_capture(segment) {
            1
        } else {
            segment.len().max(1)
        };
    }
    distance
}

fn is_capture(segment: &str) -> bool {
    capture_name(segment).is_some_and(|name| !name.is_empty())
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_byte) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_byte) in right.iter().enumerate() {
            let substitution = usize::from(left_byte != right_byte);
            current[right_index + 1] = (current[right_index] + 1)
                .min(previous[right_index + 1] + 1)
                .min(previous[right_index] + substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTES: &[RouterHintCandidate<'static>] = &[
        RouterHintCandidate {
            path: "/rest/users/{id}",
            methods: &["GET", "PATCH"],
            disclose: true,
        },
        RouterHintCandidate {
            path: "/rest/users",
            methods: &["POST"],
            disclose: true,
        },
        RouterHintCandidate {
            path: "/files/{*path}",
            methods: &["GET", "HEAD"],
            disclose: true,
        },
        RouterHintCandidate {
            path: "/_/docs",
            methods: &["GET", "HEAD"],
            disclose: true,
        },
        RouterHintCandidate {
            path: "/_/admin/runtime",
            methods: &["GET"],
            disclose: true,
        },
        RouterHintCandidate {
            path: "/secret/{id}",
            methods: &["GET"],
            disclose: false,
        },
    ];

    #[test]
    fn typo_404_suggests_nearest_public_route() {
        let hints = router_suggestions(404, "GET", "/rest/usres/42", ROUTES);
        assert_eq!(hints[0].path, "/rest/users/{id}");
        assert_eq!(hints[0].methods, vec!["GET", "PATCH"]);
    }

    #[test]
    fn dynamic_shape_beats_shorter_lexical_prefix() {
        let hints = router_suggestions(404, "GET", "/rest/users/42", ROUTES);
        assert_eq!(hints[0].path, "/rest/users/{id}");
    }

    #[test]
    fn catch_all_shape_beats_shorter_prefix() {
        let hints = router_suggestions(404, "GET", "/files/a/b/c.txt", ROUTES);
        assert_eq!(hints[0].path, "/files/{*path}");
    }

    #[test]
    fn method_405_prefers_exact_path_and_lists_allowed_methods() {
        let hints = router_suggestions(405, "POST", "/rest/users/{id}", ROUTES);
        assert_eq!(hints[0].path, "/rest/users/{id}");
        assert_eq!(hints[0].methods, vec!["GET", "PATCH"]);
    }

    #[test]
    fn route_template_matching_supports_dynamic_and_catch_all_segments() {
        assert!(route_template_matches(
            "/rest/users/{id}",
            "/rest/users/42?expand=1"
        ));
        assert!(route_template_matches(
            "/files/{*path}",
            "/files/a/b/c.txt"
        ));
        assert!(!route_template_matches("/files/{*path}", "/files"));
        assert!(!route_template_matches(
            "/files/{*path}/tail",
            "/files/a/tail"
        ));
    }

    #[test]
    fn router_miss_classifies_dynamic_method_mismatch_as_405() {
        let miss = classify_router_miss("POST", "/rest/users/42", ROUTES);
        assert_eq!(miss.status, 405);
        assert_eq!(miss.allow, vec!["GET", "PATCH"]);
    }

    #[test]
    fn router_miss_stays_404_when_matching_method_is_admitted() {
        let miss = classify_router_miss("GET", "/rest/users/42", ROUTES);
        assert_eq!(miss.status, 404);
        assert!(miss.allow.is_empty());
    }

    #[test]
    fn hidden_route_does_not_turn_a_public_miss_into_405() {
        let miss = classify_router_miss("POST", "/secret/42", ROUTES);
        assert_eq!(miss.status, 404);
        assert!(miss.allow.is_empty());
    }

    #[test]
    fn protected_and_internal_paths_are_not_disclosed() {
        let hints = router_suggestions(404, "GET", "/_/admni", ROUTES);
        assert!(hints.iter().all(|hint| hint.path != "/_/admin/runtime"));
        assert!(hints.iter().all(|hint| hint.path != "/secret/{id}"));
    }

    #[test]
    fn json_is_deterministic() {
        let first = router_error_json(404, "GET", "/rest/usres", ROUTES).expect("json");
        let second = router_error_json(404, "GET", "/rest/usres", ROUTES).expect("json");
        assert_eq!(first, second);
        assert!(first.contains("\"suggestions\""));
    }
}
