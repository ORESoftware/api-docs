//! Static analysis for handwritten `src/routes/**/route.rs` modules.
//!
//! A route directory owns exactly one canonical HTTP path. The leaf `route.rs`
//! may export multiple HTTP verbs, Next.js-style, but helpers stay private so
//! the public surface is deterministic and machine-generatable.

use std::collections::BTreeSet;

use syn::{Item, ReturnType, Type, Visibility};
use thiserror::Error;

pub const HTTP_ROUTE_EXPORTS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("patch", "PATCH"),
    ("delete", "DELETE"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRouteHandlerSource {
    pub rust_name: String,
    pub method: String,
    pub parameter_types: Vec<String>,
    pub return_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRouteModuleSource {
    pub handlers: Vec<HttpRouteHandlerSource>,
}

impl HttpRouteModuleSource {
    #[must_use]
    pub fn methods(&self) -> BTreeSet<&str> {
        self.handlers
            .iter()
            .map(|handler| handler.method.as_str())
            .collect()
    }
}

#[derive(Debug, Error)]
pub enum HttpRouteSourceError {
    #[error("Rust syntax error in {path}: {detail}")]
    Syntax { path: String, detail: String },
    #[error("{path}: route.rs must export at least one HTTP verb function")]
    NoHttpVerbs { path: String },
    #[error("{path}: public route export {name:?} is unsupported; public functions must be get/post/put/patch/delete/head/options")]
    UnsupportedPublicExport { path: String, name: String },
    #[error("{path}: public HTTP verb {name} must be async")]
    VerbMustBeAsync { path: String, name: String },
    #[error("{path}: duplicate HTTP method {method}")]
    DuplicateMethod { path: String, method: String },
}

pub fn analyze_http_route_source(
    path: &str,
    source: &str,
) -> Result<HttpRouteModuleSource, HttpRouteSourceError> {
    let file = syn::parse_file(source).map_err(|error| HttpRouteSourceError::Syntax {
        path: path.to_owned(),
        detail: error.to_string(),
    })?;

    let mut handlers = Vec::new();
    let mut seen_methods = BTreeSet::new();

    for item in file.items {
        let Item::Fn(function) = item else {
            continue;
        };
        if !matches!(function.vis, Visibility::Public(_)) {
            continue;
        }

        let rust_name = function.sig.ident.to_string();
        let Some((_, method)) = HTTP_ROUTE_EXPORTS
            .iter()
            .find(|(candidate, _)| *candidate == rust_name)
        else {
            return Err(HttpRouteSourceError::UnsupportedPublicExport {
                path: path.to_owned(),
                name: rust_name,
            });
        };

        if function.sig.asyncness.is_none() {
            return Err(HttpRouteSourceError::VerbMustBeAsync {
                path: path.to_owned(),
                name: rust_name,
            });
        }
        if !seen_methods.insert(*method) {
            return Err(HttpRouteSourceError::DuplicateMethod {
                path: path.to_owned(),
                method: (*method).to_owned(),
            });
        }

        let parameter_types = function
            .sig
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                syn::FnArg::Receiver(_) => None,
                syn::FnArg::Typed(typed) => Some(type_source(&typed.ty)),
            })
            .collect();
        let return_type = match &function.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(type_source(ty)),
        };

        handlers.push(HttpRouteHandlerSource {
            rust_name,
            method: (*method).to_owned(),
            parameter_types,
            return_type,
        });
    }

    if handlers.is_empty() {
        return Err(HttpRouteSourceError::NoHttpVerbs {
            path: path.to_owned(),
        });
    }
    handlers.sort_by(|left, right| left.method.cmp(&right.method));
    Ok(HttpRouteModuleSource { handlers })
}

fn type_source(ty: &Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_route_file_can_export_multiple_verbs() {
        let source = r#"
            use axum::{Json, extract::Path};
            pub async fn get(Path(id): Path<String>) -> Json<String> { Json(id) }
            pub async fn post(Json(body): Json<String>) -> Json<String> { Json(body) }
            fn helper() {}
        "#;
        let analysis = analyze_http_route_source("src/routes/users/[id]/route.rs", source)
            .expect("valid route module");
        assert_eq!(analysis.methods(), BTreeSet::from(["GET", "POST"]));
        assert_eq!(analysis.handlers.len(), 2);
    }

    #[test]
    fn public_non_verb_helpers_are_rejected() {
        let source = "pub async fn get() {}\npub fn helper() {}\n";
        let error = analyze_http_route_source("src/routes/x/route.rs", source)
            .expect_err("public helper must fail");
        assert!(matches!(
            error,
            HttpRouteSourceError::UnsupportedPublicExport { .. }
        ));
    }
}
