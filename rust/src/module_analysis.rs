//! Static source analysis for reserved filesystem-route modules.
//!
//! The analyzer parses Rust syntax but never executes application code. It
//! validates special-file/reserved-export structure first; generated compile
//! glue then lets rustc verify exact types and framework-specific code.

use std::collections::{BTreeMap, BTreeSet};
use syn::{
    punctuated::Punctuated, Expr, ExprLit, Item, Lit, LitStr, Meta, MetaNameValue, Token,
    Visibility,
};
use thiserror::Error;

/// Largest page revalidation interval that can round-trip exactly through the
/// JSON/JavaScript tooling boundary used by the deterministic page manifest.
/// Keep this equal to the TypeSpec `safeint` / authored JSON Schema maximum.
pub const MAX_PAGE_REVALIDATE_SECS: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteModuleKind {
    Page,
    Generator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageModuleMetadata {
    pub renderer: String,
    pub delivery: String,
    pub render: String,
    pub client: Option<String>,
    pub revalidate_secs: Option<u64>,
    pub on_demand: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub auth: String,
    pub stability: String,
    pub database: String,
    pub features: Vec<String>,
    pub data_sources: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteModuleAnalysis {
    pub kind: RouteModuleKind,
    pub public_functions: Vec<String>,
    pub page: Option<PageModuleMetadata>,
}

#[derive(Debug, Error)]
pub enum ModuleAnalysisError {
    #[error("Rust syntax error in {path}: {detail}")]
    Syntax { path: String, detail: String },
    #[error("{path}: missing required public function `{name}`")]
    MissingExport { path: String, name: &'static str },
    #[error("{path}: reserved export `{name}` belongs in {expected}")]
    WrongModule {
        path: String,
        name: &'static str,
        expected: &'static str,
    },
    #[error("{path}: `{name}` must carry #[{attribute}(...)]")]
    MissingAttribute {
        path: String,
        name: &'static str,
        attribute: &'static str,
    },
    #[error("{path}: duplicate reserved function `{name}`")]
    DuplicateExport { path: String, name: String },
    #[error("{path}: reserved function `{name}` has invalid signature: {detail}")]
    InvalidSignature {
        path: String,
        name: &'static str,
        detail: &'static str,
    },
    #[error("{path}: invalid #[ores_page] metadata: {detail}")]
    InvalidPageMetadata { path: String, detail: String },
}

pub fn analyze_page_source(
    path: &str,
    source: &str,
) -> Result<RouteModuleAnalysis, ModuleAnalysisError> {
    let file = syn::parse_file(source).map_err(|error| ModuleAnalysisError::Syntax {
        path: path.to_owned(),
        detail: error.to_string(),
    })?;
    let functions = public_functions(path, &file.items)?;
    if functions.contains_key("generate_static_params") {
        return Err(ModuleAnalysisError::WrongModule {
            path: path.to_owned(),
            name: "generate_static_params",
            expected: "sibling gen.rs",
        });
    }
    let page = functions
        .get("page")
        .ok_or_else(|| ModuleAnalysisError::MissingExport {
            path: path.to_owned(),
            name: "page",
        })?;
    require_async(path, "page", page)?;
    let attr = page
        .attrs
        .iter()
        .find(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "ores_page")
        })
        .ok_or_else(|| ModuleAnalysisError::MissingAttribute {
            path: path.to_owned(),
            name: "page",
            attribute: "ores_page",
        })?;
    let metadata = parse_page_metadata(path, attr)?;
    Ok(RouteModuleAnalysis {
        kind: RouteModuleKind::Page,
        public_functions: functions.keys().cloned().collect(),
        page: Some(metadata),
    })
}

pub fn analyze_generator_source(
    path: &str,
    source: &str,
) -> Result<RouteModuleAnalysis, ModuleAnalysisError> {
    let file = syn::parse_file(source).map_err(|error| ModuleAnalysisError::Syntax {
        path: path.to_owned(),
        detail: error.to_string(),
    })?;
    let functions = public_functions(path, &file.items)?;
    for reserved in ["page", "config", "assets"] {
        if functions.contains_key(reserved) {
            return Err(ModuleAnalysisError::WrongModule {
                path: path.to_owned(),
                name: reserved,
                expected: "page.rs",
            });
        }
    }
    let generate = functions.get("generate_static_params").ok_or_else(|| {
        ModuleAnalysisError::MissingExport {
            path: path.to_owned(),
            name: "generate_static_params",
        }
    })?;
    require_async(path, "generate_static_params", generate)?;
    if !generate.attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "ores_generate")
    }) {
        return Err(ModuleAnalysisError::MissingAttribute {
            path: path.to_owned(),
            name: "generate_static_params",
            attribute: "ores_generate",
        });
    }
    Ok(RouteModuleAnalysis {
        kind: RouteModuleKind::Generator,
        public_functions: functions.keys().cloned().collect(),
        page: None,
    })
}

fn require_async(
    path: &str,
    name: &'static str,
    function: &syn::ItemFn,
) -> Result<(), ModuleAnalysisError> {
    if function.sig.asyncness.is_none() {
        return Err(ModuleAnalysisError::InvalidSignature {
            path: path.to_owned(),
            name,
            detail: "must be async",
        });
    }
    Ok(())
}

fn public_functions<'a>(
    path: &str,
    items: &'a [Item],
) -> Result<BTreeMap<String, &'a syn::ItemFn>, ModuleAnalysisError> {
    let mut functions = BTreeMap::new();
    for item in items {
        let Item::Fn(function) = item else {
            continue;
        };
        if !matches!(function.vis, Visibility::Public(_)) {
            continue;
        }
        let name = function.sig.ident.to_string();
        if functions.insert(name.clone(), function).is_some() {
            return Err(ModuleAnalysisError::DuplicateExport {
                path: path.to_owned(),
                name,
            });
        }
    }
    Ok(functions)
}

fn parse_page_metadata(
    path: &str,
    attr: &syn::Attribute,
) -> Result<PageModuleMetadata, ModuleAnalysisError> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|error| ModuleAnalysisError::InvalidPageMetadata {
            path: path.to_owned(),
            detail: error.to_string(),
        })?;
    let mut values = BTreeMap::<String, String>::new();
    let mut lists = BTreeMap::<String, Vec<String>>::new();
    let mut revalidate_secs = None;

    for meta in args {
        match meta {
            Meta::NameValue(value) => {
                let key = value
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| invalid(path, "metadata keys must be identifiers"))?;
                if key == "revalidate_secs" {
                    if revalidate_secs.is_some() {
                        return Err(invalid(path, "duplicate key `revalidate_secs`"));
                    }
                    revalidate_secs = Some(integer_value(path, &value, &key)?);
                } else {
                    let string = string_value(path, &value, &key)?;
                    if values.insert(key.clone(), string).is_some() {
                        return Err(invalid(path, format!("duplicate key `{key}`")));
                    }
                }
            }
            Meta::List(list) => {
                let key = list
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| invalid(path, "metadata list keys must be identifiers"))?;
                if !matches!(key.as_str(), "features" | "data_sources" | "tags") {
                    return Err(invalid(path, format!("unsupported list `{key}`")));
                }
                if lists.contains_key(&key) {
                    return Err(invalid(path, format!("duplicate list `{key}`")));
                }
                let strings = list
                    .parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)
                    .map_err(|error| invalid(path, format!("invalid {key}: {error}")))?
                    .into_iter()
                    .map(|value| value.value())
                    .collect::<Vec<_>>();
                lists.insert(key, strings);
            }
            Meta::Path(_) => return Err(invalid(path, "bare ores_page flags are not supported")),
        }
    }

    let renderer = take_required(path, &mut values, "renderer")?;
    let delivery = take_required(path, &mut values, "delivery")?;
    let render = values
        .remove("render")
        .unwrap_or_else(|| "dynamic".to_owned());
    let client = values.remove("client");
    let on_demand = values.remove("on_demand");
    let title = values.remove("title");
    let summary = values.remove("summary");
    let auth = values.remove("auth").unwrap_or_else(|| "public".to_owned());
    let stability = values
        .remove("stability")
        .unwrap_or_else(|| "stable".to_owned());
    let database = values
        .remove("database")
        .unwrap_or_else(|| "none".to_owned());
    if let Some(extra) = values.keys().next() {
        return Err(invalid(path, format!("unsupported key `{extra}`")));
    }

    let features = lists.remove("features").unwrap_or_default();
    let data_sources = lists.remove("data_sources").unwrap_or_default();
    let tags = lists.remove("tags").unwrap_or_default();

    if !matches!(renderer.as_str(), "mash" | "leptos" | "dioxus") {
        return Err(invalid(path, "renderer must be mash, leptos, or dioxus"));
    }
    if !matches!(
        delivery.as_str(),
        "ssr_only" | "client_only" | "ssr_hydrate"
    ) {
        return Err(invalid(
            path,
            "delivery must be ssr_only, client_only, or ssr_hydrate",
        ));
    }
    if !matches!(
        render.as_str(),
        "dynamic" | "static_only" | "static_with_fallback"
    ) {
        return Err(invalid(
            path,
            "render must be dynamic, static_only, or static_with_fallback",
        ));
    }
    if !matches!(
        auth.as_str(),
        "public" | "optional_session" | "session" | "admin"
    ) {
        return Err(invalid(
            path,
            "auth must be public, optional_session, session, or admin",
        ));
    }
    if !matches!(stability.as_str(), "experimental" | "beta" | "stable") {
        return Err(invalid(
            path,
            "stability must be experimental, beta, or stable",
        ));
    }
    if !matches!(database.as_str(), "none" | "read_only") {
        return Err(invalid(path, "database must be none or read_only"));
    }
    if let Some(value) = revalidate_secs {
        if value == 0 || value > MAX_PAGE_REVALIDATE_SECS {
            return Err(invalid(
                path,
                format!(
                    "revalidate_secs must be in 1..={MAX_PAGE_REVALIDATE_SECS} so it round-trips exactly through the page manifest JSON contract"
                ),
            ));
        }
    }
    if revalidate_secs.is_some() && on_demand.is_some() {
        return Err(invalid(
            path,
            "revalidate_secs and on_demand are mutually exclusive",
        ));
    }
    if delivery != "ssr_only" && client.is_none() {
        return Err(invalid(
            path,
            "client_only and ssr_hydrate require client = \"...\"",
        ));
    }
    if title
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.len() > 120)
    {
        return Err(invalid(path, "title must be 1..=120 bytes when present"));
    }
    if summary
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.len() > 500)
    {
        return Err(invalid(path, "summary must be 1..=500 bytes when present"));
    }

    validate_slugs(path, "features", &features)?;
    validate_slugs(path, "tags", &tags)?;
    validate_data_sources(path, &data_sources)?;
    if data_sources.iter().any(|value| value.starts_with("orm:")) && database != "read_only" {
        return Err(invalid(
            path,
            "orm: data_sources require database = \"read_only\" on web page rendering",
        ));
    }

    Ok(PageModuleMetadata {
        renderer,
        delivery,
        render,
        client,
        revalidate_secs,
        on_demand,
        title,
        summary,
        auth,
        stability,
        database,
        features,
        data_sources,
        tags,
    })
}

fn validate_slugs(path: &str, name: &str, values: &[String]) -> Result<(), ModuleAnalysisError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty()
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/')
            })
        {
            return Err(invalid(
                path,
                format!("{name} entries must be non-empty machine-readable slugs: {value:?}"),
            ));
        }
        if !seen.insert(value) {
            return Err(invalid(path, format!("duplicate {name} entry {value:?}")));
        }
    }
    Ok(())
}

fn validate_data_sources(path: &str, values: &[String]) -> Result<(), ModuleAnalysisError> {
    let mut seen = BTreeSet::new();
    for value in values {
        let Some((kind, target)) = value.split_once(':') else {
            return Err(invalid(
                path,
                format!("data source {value:?} needs rpc: or orm: prefix"),
            ));
        };
        if !matches!(kind, "rpc" | "orm")
            || target.is_empty()
            || target.chars().any(char::is_whitespace)
        {
            return Err(invalid(
                path,
                format!("data source {value:?} must be rpc:<operation> or orm:<read-surface>"),
            ));
        }
        if !seen.insert(value) {
            return Err(invalid(path, format!("duplicate data source {value:?}")));
        }
    }
    Ok(())
}

fn take_required(
    path: &str,
    values: &mut BTreeMap<String, String>,
    key: &str,
) -> Result<String, ModuleAnalysisError> {
    values
        .remove(key)
        .ok_or_else(|| invalid(path, format!("missing required key `{key}`")))
}

fn string_value(
    path: &str,
    value: &MetaNameValue,
    name: &str,
) -> Result<String, ModuleAnalysisError> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &value.value
    else {
        return Err(invalid(path, format!("{name} must be a string literal")));
    };
    Ok(value.value())
}

fn integer_value(
    path: &str,
    value: &MetaNameValue,
    name: &str,
) -> Result<u64, ModuleAnalysisError> {
    let Expr::Lit(ExprLit {
        lit: Lit::Int(value),
        ..
    }) = &value.value
    else {
        return Err(invalid(path, format!("{name} must be an integer literal")));
    };
    value
        .base10_parse::<u64>()
        .map_err(|error| invalid(path, format!("invalid {name}: {error}")))
}

fn invalid(path: &str, detail: impl Into<String>) -> ModuleAnalysisError {
    ModuleAnalysisError::InvalidPageMetadata {
        path: path.to_owned(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_analysis_reads_docs_and_data_sources_without_executing_code() {
        let source = r#"
            #[ores_api_docs_macros::ores_page(
                renderer = "leptos",
                delivery = "ssr_hydrate",
                render = "static_with_fallback",
                revalidate_secs = 60,
                client = "client.rs",
                title = "Readiness dashboard",
                summary = "Shows readiness status.",
                auth = "session",
                stability = "beta",
                database = "read_only",
                features("readiness.overview", "evidence.summary"),
                data_sources("rpc:list_readiness", "orm:canonical_orm_core::ReadinessRead"),
                tags("customer", "compliance")
            )]
            pub async fn page() {}
        "#;
        let analysis = analyze_page_source("src/pages/readiness/page.rs", source).unwrap();
        let page = analysis.page.unwrap();
        assert_eq!(page.renderer, "leptos");
        assert_eq!(page.delivery, "ssr_hydrate");
        assert_eq!(page.revalidate_secs, Some(60));
        assert_eq!(page.features, ["readiness.overview", "evidence.summary"]);
        assert_eq!(page.database, "read_only");
    }

    #[test]
    fn revalidate_secs_obeys_the_page_manifest_safe_integer_domain() {
        for admitted in [1_u64, MAX_PAGE_REVALIDATE_SECS] {
            let source = format!(
                "#[ores_page(renderer = \"mash\", delivery = \"ssr_only\", revalidate_secs = {admitted})] pub async fn page() {{}}"
            );
            let analysis =
                analyze_page_source("src/pages/page.rs", &source).expect("boundary admitted");
            assert_eq!(
                analysis.page.expect("page metadata").revalidate_secs,
                Some(admitted)
            );
        }

        for rejected in [0_u64, MAX_PAGE_REVALIDATE_SECS + 1] {
            let source = format!(
                "#[ores_page(renderer = \"mash\", delivery = \"ssr_only\", revalidate_secs = {rejected})] pub async fn page() {{}}"
            );
            let error = analyze_page_source("src/pages/page.rs", &source)
                .expect_err("out-of-contract revalidation interval must fail closed");
            assert!(
                error.to_string().contains("revalidate_secs must be in"),
                "{error}"
            );
        }
    }

    #[test]
    fn orm_source_requires_read_only_database_declaration() {
        let source = r#"
            #[ores_page(
                renderer = "mash",
                delivery = "ssr_only",
                data_sources("orm:example_orm_core::ThingRead")
            )]
            pub async fn page() {}
        "#;
        assert!(matches!(
            analyze_page_source("src/pages/page.rs", source),
            Err(ModuleAnalysisError::InvalidPageMetadata { .. })
        ));
    }

    #[test]
    fn generate_static_params_is_rejected_from_page_rs() {
        let source = r#"
            #[ores_page(renderer = "mash", delivery = "ssr_only")]
            pub async fn page() {}
            pub async fn generate_static_params() {}
        "#;
        assert!(matches!(
            analyze_page_source("src/pages/page.rs", source),
            Err(ModuleAnalysisError::WrongModule { .. })
        ));
    }

    #[test]
    fn gen_requires_reserved_export_and_attribute() {
        let source = "#[ores_generate] pub async fn generate_static_params() {}";
        assert!(analyze_generator_source("src/pages/blog/gen.rs", source).is_ok());
    }

    #[test]
    fn sync_reserved_exports_fail_static_analysis() {
        let page = "#[ores_page(renderer = \"mash\", delivery = \"ssr_only\")] pub fn page() {}";
        assert!(matches!(
            analyze_page_source("src/pages/page.rs", page),
            Err(ModuleAnalysisError::InvalidSignature { name: "page", .. })
        ));
        let generator = "#[ores_generate] pub fn generate_static_params() {}";
        assert!(matches!(
            analyze_generator_source("src/pages/blog/gen.rs", generator),
            Err(ModuleAnalysisError::InvalidSignature {
                name: "generate_static_params",
                ..
            })
        ));
    }
}
