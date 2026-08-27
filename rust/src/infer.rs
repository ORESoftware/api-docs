//! Infer HTTP methods from a map key when the value is only a path string.

#[must_use]
pub fn infer_methods(key: &str) -> Vec<String> {
    if key.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return vec!["POST".into()];
    }
    let lower = key.to_ascii_lowercase();
    if lower.starts_with("delete") {
        return vec!["DELETE".into()];
    }
    if lower.starts_with("put") || lower.starts_with("update") || lower.starts_with("replace") {
        return vec!["PUT".into()];
    }
    if lower.starts_with("patch") {
        return vec!["PATCH".into()];
    }
    if lower.contains("create")
        || lower.contains("walk")
        || lower.contains("check")
        || lower.contains("ask")
        || lower.starts_with("post")
        || lower.starts_with("submit")
    {
        return vec!["POST".into()];
    }
    vec!["GET".into()]
}

#[must_use]
pub fn is_connect_method_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && key.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Split `/package.Service/Method` into (service, method).
#[must_use]
pub fn connect_service_method(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix('/')?;
    let (service, method) = rest.rsplit_once('/')?;
    if service.is_empty() || method.is_empty() {
        return None;
    }
    if !method.chars().next()?.is_ascii_uppercase() {
        return None;
    }
    Some((service.to_string(), method.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_is_connect_post() {
        assert_eq!(infer_methods("CheckFieldSanity"), vec!["POST"]);
        assert_eq!(infer_methods("AskCounsel"), vec!["POST"]);
        assert!(is_connect_method_key("CheckFieldSanity"));
        assert!(!is_connect_method_key("healthz"));
    }

    #[test]
    fn rest_verbs_from_key() {
        assert_eq!(infer_methods("healthz"), vec!["GET"]);
        assert_eq!(infer_methods("create_matter"), vec!["POST"]);
        assert_eq!(infer_methods("walk_matter"), vec!["POST"]);
        assert_eq!(infer_methods("get_documents"), vec!["GET"]);
    }

    #[test]
    fn connect_path_split() {
        assert_eq!(
            connect_service_method("/pmap.v1.Interview/CheckFieldSanity"),
            Some(("pmap.v1.Interview".into(), "CheckFieldSanity".into()))
        );
        assert_eq!(connect_service_method("/v1/matters/{id}"), None);
    }
}
