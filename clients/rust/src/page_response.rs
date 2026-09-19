use crate::PageResult;

pub const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";
pub const ERROR_CONTENT_TYPE: &str = "text/plain; charset=utf-8";

/// Build-time asset references needed to finalize one rendered page.
///
/// These values come from the admitted page build manifest. Keeping them in a
/// framework-neutral type lets standalone Axum and provider runtimes execute
/// the exact same HTML/CSS/WASM finalization logic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageResponseAssets<'a> {
    pub css_href: Option<&'a str>,
    pub wasm_sha256: Option<&'a str>,
    pub js_src: Option<&'a str>,
}

/// Request-scoped hints that may affect response finalization but are not page
/// business inputs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageResponseRequestHints<'a> {
    /// Comma-separated immutable WASM digests already active in a controlled
    /// soft-navigation client. This is only an optimization hint.
    pub wasm_have: Option<&'a str>,
    /// Development-only reload script candidate. The finalizer accepts only
    /// loopback HTTP URLs with a deliberately tiny character surface.
    pub dev_reload_script: Option<&'a str>,
}

/// Provider-neutral response produced after page rendering and asset injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedPageResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// ABI of the one generated finalization trampoline for a page.
///
/// `page_router_glue` closes over the page's admitted build metadata (CSS and
/// WASM/JS digests) in a normal generated function. Standalone Axum and a page
/// Lambda both call that exact function with request-scoped hints, so provider
/// runtimes do not need to rediscover or duplicate asset metadata.
pub type PageFinalizeFn =
    for<'a> fn(PageResult, PageResponseRequestHints<'a>) -> FinalizedPageResponse;

impl FinalizedPageResponse {
    fn render_error() -> Self {
        Self {
            status: 500,
            headers: vec![
                ("content-type".to_owned(), ERROR_CONTENT_TYPE.to_owned()),
                ("cache-control".to_owned(), "no-store".to_owned()),
            ],
            // Deliberately stable and non-sensitive. Hosts that need detailed
            // telemetry should record the PageError before finalization.
            body: b"page render failed".to_vec(),
        }
    }
}

/// Finalize a page without depending on Axum, a cloud provider, or a concrete
/// application state type.
///
/// Standalone web servers and generated page Lambdas must both call this
/// function before their thin host-specific response conversion. That keeps
/// status, content type, CSS/WASM injection and development cache behavior on
/// one implementation path.
#[must_use]
pub fn finalize_page_response(
    result: PageResult,
    assets: PageResponseAssets<'_>,
    hints: PageResponseRequestHints<'_>,
) -> FinalizedPageResponse {
    let document = match result {
        Ok(document) => document,
        Err(_) => return FinalizedPageResponse::render_error(),
    };

    let mut html = document.html;
    if let Some(href) = assets.css_href {
        let tag = format!(r#"<link rel="stylesheet" href="{href}">"#);
        html = inject_head(html, &tag);
    }

    if let (Some(digest), Some(src)) = (assets.wasm_sha256, assets.js_src) {
        if !wasm_digest_present(hints.wasm_have, digest) {
            let script =
                format!(r#"<script type="module" src="{src}" data-ores-wasm="{digest}"></script>"#);
            html = inject_body(html, &script);
        }
    }

    let dev_reload = hints
        .dev_reload_script
        .filter(|candidate| valid_dev_reload_script(candidate));
    if let Some(src) = dev_reload {
        let script = format!(r#"<script type="module" src="{src}" data-ores-dev-reload></script>"#);
        html = inject_body(html, &script);
    }

    let mut headers = Vec::with_capacity(document.headers.len() + 3);
    headers.push(("content-type".to_owned(), HTML_CONTENT_TYPE.to_owned()));
    headers.extend(document.headers);
    if dev_reload.is_some() {
        headers.push(("cache-control".to_owned(), "no-store".to_owned()));
        headers.push(("x-ores-dev-reload".to_owned(), "1".to_owned()));
    }

    FinalizedPageResponse {
        status: document.status,
        headers,
        body: html.into_bytes(),
    }
}

fn wasm_digest_present(header: Option<&str>, digest: &str) -> bool {
    header
        .map(|value| value.split(',').any(|item| item.trim() == digest))
        .unwrap_or(false)
}

fn valid_dev_reload_script(value: &str) -> bool {
    let loopback = value.starts_with("http://127.0.0.1:")
        || value.starts_with("http://[::1]:")
        || value.starts_with("http://localhost:");
    loopback
        && !value
            .bytes()
            .any(|byte| matches!(byte, b'"' | b'\'' | b'<' | b'>'))
}

fn inject_head(mut html: String, tag: &str) -> String {
    if let Some(index) = html.find("</head>") {
        html.insert_str(index, tag);
        html
    } else {
        format!("{tag}{html}")
    }
}

fn inject_body(mut html: String, tag: &str) -> String {
    if let Some(index) = html.find("</body>") {
        html.insert_str(index, tag);
        html
    } else {
        html.push_str(tag);
        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PageDocument, PageError};

    fn document() -> PageDocument {
        PageDocument {
            html: "<html><head></head><body><p>ok</p></body></html>".to_owned(),
            status: 201,
            headers: vec![("x-page".to_owned(), "1".to_owned())],
        }
    }

    #[test]
    fn injects_css_and_wasm_once() {
        let response = finalize_page_response(
            Ok(document()),
            PageResponseAssets {
                css_href: Some("/assets/page.css"),
                wasm_sha256: Some("abc123"),
                js_src: Some("/assets/page.js"),
            },
            PageResponseRequestHints::default(),
        );
        let body = String::from_utf8(response.body).expect("utf8 html");
        assert_eq!(response.status, 201);
        assert!(body.contains("<link rel=\"stylesheet\" href=\"/assets/page.css\">"));
        assert!(body.contains("data-ores-wasm=\"abc123\""));
        assert_eq!(response.headers[0].1, HTML_CONTENT_TYPE);
        assert!(response.headers.contains(&("x-page".to_owned(), "1".to_owned())));
    }

    #[test]
    fn skips_wasm_when_client_already_has_digest() {
        let response = finalize_page_response(
            Ok(document()),
            PageResponseAssets {
                wasm_sha256: Some("abc123"),
                js_src: Some("/assets/page.js"),
                ..PageResponseAssets::default()
            },
            PageResponseRequestHints {
                wasm_have: Some("old, abc123, other"),
                ..PageResponseRequestHints::default()
            },
        );
        let body = String::from_utf8(response.body).expect("utf8 html");
        assert!(!body.contains("data-ores-wasm"));
    }

    #[test]
    fn accepts_only_safe_loopback_dev_reload_script() {
        let safe = finalize_page_response(
            Ok(document()),
            PageResponseAssets::default(),
            PageResponseRequestHints {
                dev_reload_script: Some("http://127.0.0.1:4318/reload.js"),
                ..PageResponseRequestHints::default()
            },
        );
        let safe_body = String::from_utf8(safe.body).expect("utf8 html");
        assert!(safe_body.contains("data-ores-dev-reload"));
        assert!(safe
            .headers
            .contains(&("cache-control".to_owned(), "no-store".to_owned())));

        let unsafe_response = finalize_page_response(
            Ok(document()),
            PageResponseAssets::default(),
            PageResponseRequestHints {
                dev_reload_script: Some("https://example.com/reload.js"),
                ..PageResponseRequestHints::default()
            },
        );
        let unsafe_body = String::from_utf8(unsafe_response.body).expect("utf8 html");
        assert!(!unsafe_body.contains("data-ores-dev-reload"));
    }

    #[test]
    fn render_errors_are_stable_non_cacheable_and_do_not_leak_details() {
        let response = finalize_page_response(
            Err(PageError::Render("database password=secret".to_owned())),
            PageResponseAssets::default(),
            PageResponseRequestHints::default(),
        );
        assert_eq!(response.status, 500);
        assert_eq!(response.body, b"page render failed");
        assert!(!String::from_utf8_lossy(&response.body).contains("secret"));
        assert_eq!(
            response.headers,
            vec![
                ("content-type".to_owned(), ERROR_CONTENT_TYPE.to_owned()),
                ("cache-control".to_owned(), "no-store".to_owned()),
            ]
        );
    }
}