//! Self-contained HTML. No CDN, no Scalar, no unpkg. Escape every string.

use crate::catalog::Catalog;

#[must_use]
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[must_use]
pub fn render_html(catalog: &Catalog) -> String {
    let title = catalog
        .map
        .title
        .clone()
        .unwrap_or_else(|| catalog.map.service.clone());
    let mut rows = String::new();
    for (key, entry) in &catalog.map.map {
        let binding = entry
            .binding
            .as_ref()
            .map(|b| {
                let mut parts = Vec::new();
                if let Some(a) = &b.annotation {
                    parts.push(format!("annotation {}", a));
                }
                if !b.param_types.is_empty() {
                    parts.push(format!("params {}", b.param_types.join(", ")));
                }
                if let Some(r) = &b.return_type {
                    parts.push(format!("returns {r}"));
                }
                if let Some(f) = &b.function_type {
                    parts.push(format!("fn {f}"));
                }
                parts.join(" · ")
            })
            .unwrap_or_default();
        rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
            escape_html(key),
            escape_html(&entry.path),
            escape_html(&entry.methods.join(", ")),
            escape_html(&binding),
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — API docs</title>
<style>
body {{ font-family: ui-sans-serif, system-ui, sans-serif; margin: 1.5rem; color: #111; background: #fff; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid #ccc; padding: 0.4rem 0.6rem; text-align: left; vertical-align: top; }}
code {{ font-size: 0.9em; }}
.note {{ max-width: 52rem; line-height: 1.45; }}
</style>
</head>
<body>
<h1>{title}</h1>
<p class="note">Keys in the map are operations; values are HTTP routes. The same key may be an annotation on a method, a param type, a return type, a function type, or a combination. This page is a local table — no CDN.</p>
<p class="note">Close to OpenAPI 3.1 (<a href="/openapi.json">/openapi.json</a>), OpenRPC (<a href="/openrpc.json">/openrpc.json</a>), Connect JSON unary (<a href="/connect.json">/connect.json</a>), and JSON Schema catalog (<a href="/api/docs.json">/api/docs.json</a>).</p>
<table>
<thead><tr><th>key</th><th>route</th><th>methods</th><th>language binding</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
</body>
</html>
"#,
        title = escape_html(&title),
        rows = rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_injection() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        let html = render_html_smoke();
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    fn render_html_smoke() -> String {
        let json = r#"{
            "schema_version": "1.0.0",
            "service": "x",
            "map": {
              "evil": {
                "path": "/x",
                "methods": ["GET"],
                "binding": { "annotation": "<script>alert(1)</script>" }
              }
            }
        }"#;
        let map = crate::map::RouteMap::from_json_str(json).unwrap();
        let catalog = crate::catalog::Catalog::from_map(map).unwrap();
        render_html(&catalog)
    }
}
