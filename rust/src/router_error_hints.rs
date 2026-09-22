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
    RouterErrorEnvelope {
        status,
        code: code.to_owned(),
        message: message.to_owned(),
        suggestions: (status >= 400)
            .then(|| router_suggestions(status, method, path, candidates))
            .unwrap_or_default(),
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

#[must_use]
pub fn router_suggestions(
    status: u16,
    method: &str,
    path: &str,
    candidates: &[RouterHintCandidate<'_>],
) -> Vec<RouterSuggestion> {
    let path = normalize_path(path);
    let method = method.trim().to_ascii_uppercase();
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
            let exact_path_penalty = usize::from(candidate_path != path);
            let path_distance = levenshtein(&path, &candidate_path);
            let method_penalty = usize::from(!methods.iter().any(|value| value == &method));
            // 405 primarily answers "what methods does this exact path accept?".
            // Other errors primarily rank path similarity, then method affinity.
            let score = if status == 405 {
                (exact_path_penalty, path_distance, method_penalty)
            } else {
                (path_distance, method_penalty, exact_path_penalty)
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
            path: "/secret",
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
    fn method_405_prefers_exact_path_and_lists_allowed_methods() {
        let hints = router_suggestions(405, "POST", "/rest/users/{id}", ROUTES);
        assert_eq!(hints[0].path, "/rest/users/{id}");
        assert_eq!(hints[0].methods, vec!["GET", "PATCH"]);
    }

    #[test]
    fn protected_and_internal_paths_are_not_disclosed() {
        let hints = router_suggestions(404, "GET", "/_/admni", ROUTES);
        assert!(hints.iter().all(|hint| hint.path != "/_/admin/runtime"));
        assert!(hints.iter().all(|hint| hint.path != "/secret"));
    }

    #[test]
    fn JSON_is_deterministic() {
        let first = router_error_json(404, "GET", "/rest/usres", ROUTES).expect("json");
        let second = router_error_json(404, "GET", "/rest/usres", ROUTES).expect("json");
        assert_eq!(first, second);
        assert!(first.contains("\"suggestions\""));
    }
}
