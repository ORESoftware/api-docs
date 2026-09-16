use crate::PageDocument;

const DEV_RELOAD_ORIGIN_ENV: &str = "ORES_STACK_DEV_RELOAD_ORIGIN";

/// Inject the development reload bootstrap only when `ores-stack dev` launches
/// the product server with a validated localhost reload origin.
///
/// This remains client-facade code: it registers no server route and opens no
/// connection from Rust. The browser polls the separate development supervisor.
pub fn maybe_inject_dev_reload(document: &mut PageDocument) {
    let Ok(origin) = std::env::var(DEV_RELOAD_ORIGIN_ENV) else {
        return;
    };
    if !is_loopback_http_origin(&origin) {
        return;
    }

    let script = format!(
        r#"<script type="module" data-ores-dev-reload>
(() => {{
  const endpoint = {origin:?} + "/state";
  let generation = null;
  const routeMatches = (pattern, path) => {{
    if (!pattern) return true;
    const want = pattern.split("/").filter(Boolean);
    const have = path.split("/").filter(Boolean);
    let i = 0;
    for (; i < want.length; i += 1) {{
      const segment = want[i];
      if (segment.startsWith("{{*") && segment.endsWith("}}")) return true;
      if (i >= have.length) return false;
      if (segment.startsWith("{{") && segment.endsWith("}}")) continue;
      if (segment !== have[i]) return false;
    }}
    return i === have.length;
  }};
  const refreshStyles = async () => {{
    const response = await fetch(location.href, {{ cache: "no-store" }});
    if (!response.ok) throw new Error("page refresh probe failed");
    const html = await response.text();
    const next = new DOMParser().parseFromString(html, "text/html");
    const oldLinks = [...document.querySelectorAll('link[rel="stylesheet"]')];
    const newLinks = [...next.querySelectorAll('link[rel="stylesheet"]')];
    if (oldLinks.length !== newLinks.length) throw new Error("stylesheet shape changed");
    oldLinks.forEach((link, index) => {{
      const href = newLinks[index]?.getAttribute("href");
      if (href && link.getAttribute("href") !== href) link.setAttribute("href", href);
    }});
  }};
  const apply = async (state) => {{
    if ((state.kind === "page" || state.kind === "style") && !routeMatches(state.route, location.pathname)) return;
    if (state.kind === "style") {{
      try {{ await refreshStyles(); return; }} catch (_) {{ location.reload(); return; }}
    }}
    if (state.kind === "pagelet") {{
      const event = new CustomEvent("ores:pagelet-reload", {{ detail: state, cancelable: true }});
      if (!window.dispatchEvent(event)) return;
    }}
    location.reload();
  }};
  const tick = async () => {{
    try {{
      const response = await fetch(endpoint, {{ cache: "no-store" }});
      if (!response.ok) return;
      const state = await response.json();
      if (generation === null) {{ generation = state.generation; return; }}
      if (state.generation === generation) return;
      generation = state.generation;
      await apply(state);
    }} catch (_) {{ /* server may be between supervised restarts */ }}
  }};
  void tick();
  setInterval(() => {{ void tick(); }}, 750);
}})();
</script>"#
    );

    if let Some(index) = document.html.find("</body>") {
        document.html.insert_str(index, &script);
    } else {
        document.html.push_str(&script);
    }
}

fn is_loopback_http_origin(origin: &str) -> bool {
    let port = origin
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| origin.strip_prefix("http://localhost:"));
    port.is_some_and(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::is_loopback_http_origin;

    #[test]
    fn dev_reload_origin_is_loopback_only() {
        assert!(is_loopback_http_origin("http://127.0.0.1:35729"));
        assert!(is_loopback_http_origin("http://localhost:4000"));
        assert!(!is_loopback_http_origin("https://127.0.0.1:35729"));
        assert!(!is_loopback_http_origin("http://example.com:35729"));
        assert!(!is_loopback_http_origin("http://127.0.0.1:x"));
    }
}
