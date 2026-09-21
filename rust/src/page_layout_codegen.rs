use crate::{page_lambda_entry_ident, page_layout_sources, FsRoute, FsRouteKind};
use std::{collections::BTreeMap, fs, path::Path};

/// Generate the normal typed page compile glue and wrap each exported page
/// entry with its build-time-resolved `layout.rs` ancestry.
///
/// `page_layout_sources` returns root-to-leaf. Execution is intentionally the
/// reverse: leaf wraps the page first and the root layout is outermost. The
/// exported page-entry symbol remains unchanged, so standalone routing and web
/// Lambda projection can share this exact composed function.
pub fn page_compile_glue_with_layouts(
    repo_root: &Path,
    routes: &[FsRoute],
) -> Result<String, String> {
    let mut out = crate::fs_codegen::page_compile_glue(repo_root, routes)?;
    let root = repo_root
        .canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", repo_root.display()))?;

    let mut chains = Vec::<(String, Vec<String>)>::with_capacity(routes.len());
    let mut modules = BTreeMap::<String, String>::new();
    for route in routes {
        if route.kind != FsRouteKind::Page {
            return Err(format!("{} is not a page route", route.source));
        }
        let layouts = page_layout_sources(&root, &route.source)?;
        for source in &layouts {
            modules
                .entry(source.clone())
                .or_insert_with(|| layout_module_ident(source));
        }
        chains.push((route.source.clone(), layouts));
    }

    if modules.is_empty() {
        return Ok(out);
    }

    out.push_str("\n// Build-time-resolved page layout modules. Runtime never walks src/pages.\n");
    for (source, module) in &modules {
        let path = root.join(source);
        let canonical = fs::canonicalize(&path)
            .map_err(|error| format!("canonicalize layout {}: {error}", path.display()))?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err(format!(
                "layout source escaped repository root: {}",
                path.display()
            ));
        }
        let literal = format!("{:?}", canonical.to_string_lossy());
        out.push_str(&format!(
            "#[path = {literal}]\nmod {module};\n\
             const _: ::ores_api_docs_client::PageLayoutFn = {module}::layout;\n"
        ));
    }

    for (page_source, layouts) in chains {
        if layouts.is_empty() {
            continue;
        }
        let entry = page_lambda_entry_ident(&page_source);
        let raw_entry = format!("{entry}__without_layouts");
        let needle = format!("pub fn {entry}(");
        let count = out.matches(&needle).count();
        if count != 1 {
            return Err(format!(
                "expected exactly one exported page entry {entry} for {page_source}, found {count}"
            ));
        }
        out = out.replacen(&needle, &format!("pub fn {raw_entry}("), 1);

        out.push_str(&format!(
            "\n#[doc(hidden)]\n#[allow(dead_code)]\n\
             pub fn {entry}(ctx: ::ores_api_docs_client::PageContext) -> ::ores_api_docs_client::PageFuture {{\n\
                 ::std::boxed::Box::pin(async move {{\n\
                     let __ores_layout_ctx = ctx.clone();\n\
                     let mut __ores_document = {raw_entry}(ctx).await?;\n"
        ));
        for source in layouts.iter().rev() {
            let module = modules
                .get(source)
                .ok_or_else(|| format!("layout module missing for {source}"))?;
            out.push_str(&format!(
                "                    __ores_document = {module}::layout(__ores_layout_ctx.clone(), __ores_document).await?;\n"
            ));
        }
        out.push_str(
            "                    Ok(__ores_document)\n                })\n            }\n",
        );
    }

    Ok(out)
}

fn layout_module_ident(source: &str) -> String {
    let mut out = String::from("__ores_layout_");
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.push('_');
    out.push_str(&crate::project::sha256_hex(source.as_bytes())[..16]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn fixture_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ores-layout-codegen-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src/pages/account/settings")).unwrap();
        fs::write(
            root.join("src/pages/account/settings/page.rs"),
            r#"#[ores_page(renderer = "mash", delivery = "ssr_only")]
pub async fn page(_ctx: ::ores_api_docs_client::PageContext) -> ::ores_api_docs_client::PageResult {
    unimplemented!()
}
"#,
        )
        .unwrap();
        root
    }

    #[test]
    fn wraps_leaf_to_root_and_preserves_exported_entry_identity() {
        let root = fixture_root();
        fs::write(root.join("src/pages/layout.rs"), "pub fn layout() {}\n").unwrap();
        fs::write(
            root.join("src/pages/account/layout.rs"),
            "pub fn layout() {}\n",
        )
        .unwrap();
        let route = FsRoute::page("src/pages/account/settings/page.rs").unwrap();
        let source = page_compile_glue_with_layouts(&root, std::slice::from_ref(&route)).unwrap();
        let entry = page_lambda_entry_ident(&route.source);
        assert!(source.contains(&format!("pub fn {entry}__without_layouts(")));
        assert!(source.contains(&format!("pub fn {entry}(")));
        let leaf = layout_module_ident("src/pages/account/layout.rs");
        let root_layout = layout_module_ident("src/pages/layout.rs");
        let leaf_call = format!("__ores_document = {leaf}::layout(");
        let root_call = format!("__ores_document = {root_layout}::layout(");
        let leaf_at = source.find(&leaf_call).unwrap();
        let root_at = source.find(&root_call).unwrap();
        assert!(
            leaf_at < root_at,
            "leaf layout must execute before root layout"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_layouts_leave_existing_page_glue_byte_shape_alone() {
        let root = fixture_root();
        let route = FsRoute::page("src/pages/account/settings/page.rs").unwrap();
        let base =
            crate::fs_codegen::page_compile_glue(&root, std::slice::from_ref(&route)).unwrap();
        let composed = page_compile_glue_with_layouts(&root, &[route]).unwrap();
        assert_eq!(base, composed);
        let _ = fs::remove_dir_all(root);
    }
}
