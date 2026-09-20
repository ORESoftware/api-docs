use crate::{page_lambda_entry_ident, FsRoute, FsRouteKind, PageBuildRoute};
use std::path::Path;

/// Generate the standalone Axum page router and force every request through the
/// same exported page entry used by generated web `lambda.rs`.
///
/// The underlying router generator intentionally remains unchanged; this thin
/// rewrite removes its historical direct call to `page::__ores_page_boxed` so
/// layout composition, and any future page-entry middleware, cannot diverge
/// between the standalone server and a provider wrapper.
pub fn page_router_glue_with_layouts(
    repo_root: &Path,
    routes: &[FsRoute],
    manifest: &[PageBuildRoute],
) -> Result<String, String> {
    let source = crate::page_router_codegen::page_router_glue(repo_root, routes, manifest)?;
    rewrite_router_page_entries(source, routes)
}

fn rewrite_router_page_entries(mut source: String, routes: &[FsRoute]) -> Result<String, String> {
    for route in routes {
        if route.kind != FsRouteKind::Page {
            return Err(format!("{} is not a page route", route.source));
        }
        let module = module_ident("page", &route.source);
        let entry = page_lambda_entry_ident(&route.source);
        let needle = format!("let result = {module}::__ores_page_boxed(ctx).await;");
        let count = source.matches(&needle).count();
        if count != 1 {
            return Err(format!(
                "expected exactly one standalone direct page call for {}, found {count}",
                route.source
            ));
        }
        source = source.replacen(&needle, &format!("let result = {entry}(ctx).await;"), 1);
    }
    Ok(source)
}

fn module_ident(prefix: &str, source: &str) -> String {
    let mut out = format!("__ores_{prefix}_");
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_handler_is_redirected_to_shared_exported_entry() {
        let route = FsRoute::page("src/pages/users/[id]/page.rs").unwrap();
        let module = module_ident("page", &route.source);
        let entry = page_lambda_entry_ident(&route.source);
        let input = format!(
            "async fn handler() {{ let result = {module}::__ores_page_boxed(ctx).await; }}\n\
             pub fn {entry}(ctx: PageContext) -> PageFuture {{ todo!() }}\n"
        );
        let output = rewrite_router_page_entries(input, &[route]).unwrap();
        assert!(output.contains(&format!("let result = {entry}(ctx).await;")));
        assert!(!output.contains(&format!(
            "let result = {module}::__ores_page_boxed(ctx).await;"
        )));
    }

    #[test]
    fn missing_or_duplicate_direct_calls_fail_closed() {
        let route = FsRoute::page("src/pages/page.rs").unwrap();
        assert!(rewrite_router_page_entries(String::new(), &[route.clone()]).is_err());
        let module = module_ident("page", &route.source);
        let one = format!("let result = {module}::__ores_page_boxed(ctx).await;");
        assert!(rewrite_router_page_entries(format!("{one}\n{one}\n"), &[route]).is_err());
    }
}
