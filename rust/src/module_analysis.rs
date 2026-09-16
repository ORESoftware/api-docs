//! Static source analysis for reserved filesystem-route modules.
//!
//! The analyzer parses Rust syntax but never executes application code. It
//! validates special-file/reserved-export structure first; generated compile
//! glue then lets rustc verify exact types and framework-specific code.

use std::collections::BTreeMap;
use syn::{
    punctuated::Punctuated, Expr, ExprLit, Item, Lit, Meta, MetaNameValue, Token, Visibility,
};
use thiserror::Error;

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
    #[error("{path}: invalid #[ores_page] metadata: {detail}")]
    InvalidPageMetadata { path: String, detail: String },
}

pub fn analyze_page_source(path: &str, source: &str) -> Result<RouteModuleAnalysis, ModuleAnalysisError> {
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
    let attr = page
        .attrs
        .iter()
        .find(|attr| attr.path().segments.last().is_some_and(|seg| seg.ident == "ores_page"))
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
    let generate = functions
        .get("generate_static_params")
        .ok_or_else(|| ModuleAnalysisError::MissingExport {
            path: path.to_owned(),
            name: "generate_static_params",
        })?;
    if !generate
        .attrs
        .iter()
        .any(|attr| attr.path().segments.last().is_some_and(|seg| seg.ident == "ores_generate"))
    {
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

fn public_functions<'a>(
    path: &str,
    items: &'a [Item],
) -> Result<BTreeMap<String, &'a syn::ItemFn>, ModuleAnalysisError> {
    let mut functions = BTreeMap::new();
    for item in items {
        let Item::Fn(function) = item else { continue };
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
    let mut revalidate_secs = None;
    for meta in args {
        let Meta::NameValue(value) = meta else {
            return Err(invalid(path, "arguments must use key = value"));
        };
        let key = value
            .path
            .get_ident()
            .map(ToString::to_string)
            .ok_or_else(|| invalid(path, "metadata keys must be identifiers"))?;
        if key == "revalidate_secs" {
            revalidate_secs = Some(integer_value(path, &value, &key)?);
        } else {
            let value = string_value(path, &value, &key)?;
            if values.insert(key.clone(), value).is_some() {
                return Err(invalid(path, format!("duplicate key `{key}`")));
            }
        }
    }
    let renderer = take_required(path, &mut values, "renderer")?;
    let delivery = take_required(path, &mut values, "delivery")?;
    let render = values.remove("render").unwrap_or_else(|| "dynamic".to_owned());
    let client = values.remove("client");
    let on_demand = values.remove("on_demand");
    if let Some(extra) = values.keys().next() {
        return Err(invalid(path, format!("unsupported key `{extra}`")));
    }
    if !matches!(renderer.as_str(), "mash" | "leptos" | "dioxus") {
        return Err(invalid(path, "renderer must be mash, leptos, or dioxus"));
    }
    if !matches!(delivery.as_str(), "ssr_only" | "client_only" | "ssr_hydrate") {
        return Err(invalid(
            path,
            "delivery must be ssr_only, client_only, or ssr_hydrate",
        ));
    }
    if !matches!(render.as_str(), "dynamic" | "static_only" | "static_with_fallback") {
        return Err(invalid(
            path,
            "render must be dynamic, static_only, or static_with_fallback",
        ));
    }
    if revalidate_secs.is_some() && on_demand.is_some() {
        return Err(invalid(path, "revalidate_secs and on_demand are mutually exclusive"));
    }
    if delivery != "ssr_only" && client.is_none() {
        return Err(invalid(
            path,
            "client_only and ssr_hydrate require client = \"...\"",
        ));
    }
    Ok(PageModuleMetadata {
        renderer,
        delivery,
        render,
        client,
        revalidate_secs,
        on_demand,
    })
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
    let Expr::Lit(ExprLit { lit: Lit::Str(value), .. }) = &value.value else {
        return Err(invalid(path, format!("{name} must be a string literal")));
    };
    Ok(value.value())
}

fn integer_value(
    path: &str,
    value: &MetaNameValue,
    name: &str,
) -> Result<u64, ModuleAnalysisError> {
    let Expr::Lit(ExprLit { lit: Lit::Int(value), .. }) = &value.value else {
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
    fn page_analysis_reads_reserved_attribute_without_executing_code() {
        let source = r#"
            #[ores_api_docs_macros::ores_page(
                renderer = "leptos",
                delivery = "ssr_hydrate",
                render = "static_with_fallback",
                revalidate_secs = 60,
                client = "client.rs"
            )]
            pub fn page() {}
        "#;
        let analysis = analyze_page_source("src/pages/blog/page.rs", source).unwrap();
        let page = analysis.page.unwrap();
        assert_eq!(page.renderer, "leptos");
        assert_eq!(page.delivery, "ssr_hydrate");
        assert_eq!(page.revalidate_secs, Some(60));
    }

    #[test]
    fn generate_static_params_is_rejected_from_page_rs() {
        let source = r#"
            #[ores_page(renderer = "mash", delivery = "ssr_only")]
            pub fn page() {}
            pub fn generate_static_params() {}
        "#;
        assert!(matches!(
            analyze_page_source("src/pages/page.rs", source),
            Err(ModuleAnalysisError::WrongModule { .. })
        ));
    }

    #[test]
    fn gen_requires_reserved_export_and_attribute() {
        let source = "#[ores_generate] pub fn generate_static_params() {}";
        assert!(analyze_generator_source("src/pages/blog/gen.rs", source).is_ok());
    }
}
