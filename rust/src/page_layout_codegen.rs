use crate::{
    page_lambda_entry_ident, page_loading_entry_ident, page_segment_sources, FsRoute, FsRouteKind,
    PageSegmentSources,
};
use std::{collections::BTreeMap, fs, path::Path};

/// Generate the normal typed page compile glue and wrap each exported page
/// entry with its build-time-resolved segment convention ancestry.
///
/// Segment ancestry is root-to-leaf, but composition executes leaf-to-root.
/// Within one segment the child result first crosses `not_found.rs` / `error.rs`,
/// then `template.rs`, then `layout.rs`. This mirrors the useful App-Router
/// boundary shape while keeping the ABI Rust-native and provider-neutral.
///
/// `loading.rs` is deliberately exported as a separate nearest-boundary
/// function. A host must have real streaming or soft-navigation support before
/// it may render that fallback while the page future is still pending; waiting
/// for the page and then rendering loading UI would be false Suspense semantics.
pub fn page_compile_glue_with_layouts(
    repo_root: &Path,
    routes: &[FsRoute],
) -> Result<String, String> {
    let mut out = crate::fs_codegen::page_compile_glue(repo_root, routes)?;
    let root = repo_root
        .canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", repo_root.display()))?;

    let mut chains = Vec::<(String, Vec<PageSegmentSources>)>::with_capacity(routes.len());
    let mut modules = BTreeMap::<String, String>::new();
    for route in routes {
        if route.kind != FsRouteKind::Page {
            return Err(format!("{} is not a page route", route.source));
        }
        let segments = page_segment_sources(&root, &route.source)?;
        for source in segments.iter().flat_map(segment_sources) {
            modules
                .entry(source.to_owned())
                .or_insert_with(|| segment_module_ident(source));
        }
        chains.push((route.source.clone(), segments));
    }

    if !modules.is_empty() {
        out.push_str(
            "\n// Build-time-resolved page segment modules. Runtime never walks src/pages.\n",
        );
        for (source, module) in &modules {
            let path = root.join(source);
            let canonical = fs::canonicalize(&path).map_err(|error| {
                format!("canonicalize page segment {}: {error}", path.display())
            })?;
            if !canonical.starts_with(&root) || !canonical.is_file() {
                return Err(format!(
                    "page segment source escaped repository root: {}",
                    path.display()
                ));
            }
            let literal = format!("{:?}", canonical.to_string_lossy());
            let (function, abi) = segment_function_and_abi(source)?;
            out.push_str(&format!(
                "#[path = {literal}]\nmod {module};\nconst _: ::ores_api_docs_client::{abi} = {module}::{function};\n"
            ));
        }
    }

    for (page_source, segments) in chains {
        let entry = page_lambda_entry_ident(&page_source);
        let loading_entry = page_loading_entry_ident(&page_source);
        let has_runtime_boundaries = segments.iter().any(|segment| {
            segment.layout.is_some()
                || segment.template.is_some()
                || segment.error.is_some()
                || segment.not_found.is_some()
        });

        if has_runtime_boundaries {
            let raw_entry = format!("{entry}__without_segment_boundaries");
            let needle = format!("pub fn {entry}(");
            let count = out.matches(&needle).count();
            if count != 1 {
                return Err(format!(
                    "expected exactly one exported page entry {entry} for {page_source}, found {count}"
                ));
            }
            out = out.replacen(&needle, &format!("pub fn {raw_entry}("), 1);

            out.push_str(&format!(
                "\n#[doc(hidden)]\n#[allow(dead_code)]\npub fn {entry}(ctx: ::ores_api_docs_client::PageContext) -> ::ores_api_docs_client::PageFuture {{\n    ::std::boxed::Box::pin(async move {{\n        let __ores_segment_ctx = ctx.clone();\n        let mut __ores_result = {raw_entry}(ctx).await;\n"
            ));

            for segment in segments.iter().rev() {
                if let Some(source) = segment.not_found.as_deref() {
                    let module = module_for(&modules, source)?;
                    out.push_str(&format!(
                        "        __ores_result = match __ores_result {{\n            Err(::ores_api_docs_client::PageError::NotFound) => {module}::not_found(__ores_segment_ctx.clone()).await,\n            other => other,\n        }};\n"
                    ));
                }
                if let Some(source) = segment.error.as_deref() {
                    let module = module_for(&modules, source)?;
                    out.push_str(&format!(
                        "        __ores_result = match __ores_result {{\n            Err(::ores_api_docs_client::PageError::NotFound) => Err(::ores_api_docs_client::PageError::NotFound),\n            Err(error) => {module}::error(__ores_segment_ctx.clone(), error).await,\n            ok => ok,\n        }};\n"
                    ));
                }
                if let Some(source) = segment.template.as_deref() {
                    let module = module_for(&modules, source)?;
                    out.push_str(&format!(
                        "        __ores_result = match __ores_result {{\n            Ok(document) => {module}::template(__ores_segment_ctx.clone(), document).await,\n            error => error,\n        }};\n"
                    ));
                }
                if let Some(source) = segment.layout.as_deref() {
                    let module = module_for(&modules, source)?;
                    out.push_str(&format!(
                        "        __ores_result = match __ores_result {{\n            Ok(document) => {module}::layout(__ores_segment_ctx.clone(), document).await,\n            error => error,\n        }};\n"
                    ));
                }
            }
            out.push_str("        __ores_result\n    })\n}\n");
        }

        let nearest_loading = segments
            .iter()
            .rev()
            .find_map(|segment| segment.loading.as_deref());
        match nearest_loading {
            Some(source) => {
                let module = module_for(&modules, source)?;
                out.push_str(&format!(
                    "\n#[doc(hidden)]\n#[allow(dead_code)]\npub fn {loading_entry}(ctx: ::ores_api_docs_client::PageContext) -> Option<::ores_api_docs_client::PageFuture> {{\n    Some({module}::loading(ctx))\n}}\n"
                ));
            }
            None => out.push_str(&format!(
                "\n#[doc(hidden)]\n#[allow(dead_code)]\npub fn {loading_entry}(_ctx: ::ores_api_docs_client::PageContext) -> Option<::ores_api_docs_client::PageFuture> {{\n    None\n}}\n"
            )),
        }
    }

    Ok(out)
}

fn segment_sources<'a>(
    segment: &'a PageSegmentSources,
) -> impl Iterator<Item = &'a str> + 'a {
    [
        segment.layout.as_deref(),
        segment.template.as_deref(),
        segment.error.as_deref(),
        segment.loading.as_deref(),
        segment.not_found.as_deref(),
    ]
    .into_iter()
    .flatten()
}

fn module_for<'a>(modules: &'a BTreeMap<String, String>, source: &str) -> Result<&'a str, String> {
    modules
        .get(source)
        .map(String::as_str)
        .ok_or_else(|| format!("page segment module missing for {source}"))
}

fn segment_function_and_abi(source: &str) -> Result<(&'static str, &'static str), String> {
    if source.ends_with("/layout.rs") {
        Ok(("layout", "PageLayoutFn"))
    } else if source.ends_with("/template.rs") {
        Ok(("template", "PageTemplateFn"))
    } else if source.ends_with("/error.rs") {
        Ok(("error", "PageErrorBoundaryFn"))
    } else if source.ends_with("/loading.rs") {
        Ok(("loading", "PageLoadingFn"))
    } else if source.ends_with("/not_found.rs") {
        Ok(("not_found", "PageNotFoundFn"))
    } else {
        Err(format!("unsupported page segment source {source:?}"))
    }
}

fn segment_module_ident(source: &str) -> String {
    let mut out = String::from("__ores_segment_");
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
    fn composes_child_boundary_template_layout_then_parent() {
        let root = fixture_root();
        fs::write(root.join("src/pages/layout.rs"), "pub fn layout() {}\n").unwrap();
        fs::write(root.join("src/pages/error.rs"), "pub fn error() {}\n").unwrap();
        fs::write(
            root.join("src/pages/account/template.rs"),
            "pub fn template() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/account/not_found.rs"),
            "pub fn not_found() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/pages/account/settings/layout.rs"),
            "pub fn layout() {}\n",
        )
        .unwrap();
        let route = FsRoute::page("src/pages/account/settings/page.rs").unwrap();
        let source = page_compile_glue_with_layouts(&root, &[route.clone()]).unwrap();
        let entry = page_lambda_entry_ident(&route.source);
        assert!(source.contains(&format!(
            "pub fn {entry}__without_segment_boundaries("
        )));
        assert!(source.contains(&format!("pub fn {entry}(")));

        let leaf_layout = segment_module_ident("src/pages/account/settings/layout.rs");
        let account_not_found = segment_module_ident("src/pages/account/not_found.rs");
        let account_template = segment_module_ident("src/pages/account/template.rs");
        let root_error = segment_module_ident("src/pages/error.rs");
        let root_layout = segment_module_ident("src/pages/layout.rs");
        let positions = [
            source.find(&format!("{leaf_layout}::layout(")).unwrap(),
            source.find(&format!("{account_not_found}::not_found(")).unwrap(),
            source.find(&format!("{account_template}::template(")).unwrap(),
            source.find(&format!("{root_error}::error(")).unwrap(),
            source.find(&format!("{root_layout}::layout(")).unwrap(),
        ];
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exports_nearest_loading_boundary_separately() {
        let root = fixture_root();
        fs::write(root.join("src/pages/loading.rs"), "pub fn loading() {}\n").unwrap();
        fs::write(
            root.join("src/pages/account/loading.rs"),
            "pub fn loading() {}\n",
        )
        .unwrap();
        let route = FsRoute::page("src/pages/account/settings/page.rs").unwrap();
        let source = page_compile_glue_with_layouts(&root, &[route.clone()]).unwrap();
        let loading_entry = page_loading_entry_ident(&route.source);
        let nearest = segment_module_ident("src/pages/account/loading.rs");
        assert!(source.contains(&format!("pub fn {loading_entry}(")));
        assert!(source.contains(&format!("Some({nearest}::loading(ctx))")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_boundaries_preserve_page_entry_and_export_no_loading() {
        let root = fixture_root();
        let route = FsRoute::page("src/pages/account/settings/page.rs").unwrap();
        let base = crate::fs_codegen::page_compile_glue(&root, &[route.clone()]).unwrap();
        let composed = page_compile_glue_with_layouts(&root, &[route.clone()]).unwrap();
        let entry = page_lambda_entry_ident(&route.source);
        let loading_entry = page_loading_entry_ident(&route.source);
        assert!(composed.contains(&format!("pub fn {entry}(")));
        assert!(composed.contains(&format!("pub fn {loading_entry}(")));
        assert!(composed.starts_with(&base));
        let _ = fs::remove_dir_all(root);
    }
}