//! Static analysis for handwritten `src/routes/**/route.rs` modules.
//!
//! A route directory owns exactly one canonical HTTP path. The leaf `route.rs`
//! may export multiple HTTP verbs, Next.js-style, but helpers stay private so
//! the public surface is deterministic and machine-generatable.

use std::collections::BTreeSet;

use syn::{
    punctuated::Punctuated, Expr, ExprLit, Item, Lit, LitStr, Meta, ReturnType, Token, Type,
    Visibility,
};
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
pub struct RpcRouteAttributeSource {
    pub key: String,
    pub codecs: Vec<String>,
    pub default_codec: String,
    pub audiences: Vec<String>,
    pub scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRouteHandlerSource {
    pub rust_name: String,
    pub method: String,
    pub parameter_types: Vec<String>,
    pub return_type: Option<String>,
    pub rpc: Option<RpcRouteAttributeSource>,
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
    #[error("{path}: invalid #[ores_rpc] metadata on {name}: {detail}")]
    InvalidRpcMetadata {
        path: String,
        name: String,
        detail: String,
    },
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
        let rpc = function
            .attrs
            .iter()
            .find(|attr| {
                attr.path()
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "ores_rpc")
            })
            .map(|attr| parse_rpc_attribute(path, &rust_name, attr))
            .transpose()?;

        handlers.push(HttpRouteHandlerSource {
            rust_name,
            method: (*method).to_owned(),
            parameter_types,
            return_type,
            rpc,
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

fn parse_rpc_attribute(
    path: &str,
    name: &str,
    attr: &syn::Attribute,
) -> Result<RpcRouteAttributeSource, HttpRouteSourceError> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|error| invalid_rpc(path, name, error.to_string()))?;
    let mut key = None;
    let mut default_codec = None;
    let mut scope = None;
    let mut codecs = None;
    let mut audiences = None;

    for meta in args {
        match meta {
            Meta::NameValue(value) => {
                let field = value
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| invalid_rpc(path, name, "metadata keys must be identifiers"))?;
                let string = string_meta_value(path, name, &value.value, &field)?;
                match field.as_str() {
                    "key" => set_rpc_once(path, name, &field, &mut key, string)?,
                    "default_codec" => {
                        set_rpc_once(path, name, &field, &mut default_codec, string)?
                    }
                    "scope" => set_rpc_once(path, name, &field, &mut scope, string)?,
                    _ => {
                        return Err(invalid_rpc(
                            path,
                            name,
                            format!("unsupported ores_rpc key {field:?}"),
                        ))
                    }
                }
            }
            Meta::List(list) => {
                let field = list
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| invalid_rpc(path, name, "metadata lists must be identifiers"))?;
                let values = list
                    .parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)
                    .map_err(|error| invalid_rpc(path, name, error.to_string()))?
                    .into_iter()
                    .map(|value| value.value())
                    .collect::<Vec<_>>();
                match field.as_str() {
                    "codecs" => set_rpc_once(path, name, &field, &mut codecs, values)?,
                    "audiences" => set_rpc_once(path, name, &field, &mut audiences, values)?,
                    _ => {
                        return Err(invalid_rpc(
                            path,
                            name,
                            format!("unsupported ores_rpc list {field:?}"),
                        ))
                    }
                }
            }
            Meta::Path(flag) => {
                return Err(invalid_rpc(
                    path,
                    name,
                    format!("bare ores_rpc flag {flag:?} is not supported"),
                ))
            }
        }
    }

    let key = key.ok_or_else(|| invalid_rpc(path, name, "ores_rpc requires key = \"...\""))?;
    if !valid_rpc_key(&key) {
        return Err(invalid_rpc(
            path,
            name,
            "key must be a stable dotted lowercase object key",
        ));
    }
    let codecs = codecs.unwrap_or_else(|| vec!["json".to_owned()]);
    if codecs.is_empty() {
        return Err(invalid_rpc(path, name, "codecs must not be empty"));
    }
    let mut seen = BTreeSet::new();
    for codec in &codecs {
        if !matches!(codec.as_str(), "json" | "protobuf" | "messagepack") {
            return Err(invalid_rpc(
                path,
                name,
                format!("unsupported payload codec {codec:?}"),
            ));
        }
        if !seen.insert(codec.as_str()) {
            return Err(invalid_rpc(
                path,
                name,
                format!("duplicate payload codec {codec:?}"),
            ));
        }
    }
    let default_codec = default_codec.unwrap_or_else(|| codecs[0].clone());
    if !codecs.iter().any(|codec| codec == &default_codec) {
        return Err(invalid_rpc(
            path,
            name,
            "default_codec must also appear in codecs(...)",
        ));
    }
    let audiences = audiences.unwrap_or_else(|| vec!["server".to_owned()]);
    if audiences.is_empty() {
        return Err(invalid_rpc(path, name, "audiences must not be empty"));
    }
    let mut seen = BTreeSet::new();
    for audience in &audiences {
        if !matches!(audience.as_str(), "browser" | "server") {
            return Err(invalid_rpc(
                path,
                name,
                format!("unsupported RPC audience {audience:?}"),
            ));
        }
        if !seen.insert(audience.as_str()) {
            return Err(invalid_rpc(
                path,
                name,
                format!("duplicate RPC audience {audience:?}"),
            ));
        }
    }
    let scope = scope.unwrap_or_else(|| "regular".to_owned());
    if !matches!(scope.as_str(), "regular" | "admin") {
        return Err(invalid_rpc(path, name, "scope must be regular or admin"));
    }
    if scope == "admin" && audiences.iter().any(|audience| audience == "browser") {
        return Err(invalid_rpc(
            path,
            name,
            "admin RPC operations are server-only and may not target browser clients",
        ));
    }

    Ok(RpcRouteAttributeSource {
        key,
        codecs,
        default_codec,
        audiences,
        scope,
    })
}

fn set_rpc_once<T>(
    path: &str,
    name: &str,
    field: &str,
    slot: &mut Option<T>,
    value: T,
) -> Result<(), HttpRouteSourceError> {
    if slot.is_some() {
        return Err(invalid_rpc(path, name, format!("duplicate {field}")));
    }
    *slot = Some(value);
    Ok(())
}

fn string_meta_value(
    path: &str,
    name: &str,
    expr: &Expr,
    field: &str,
) -> Result<String, HttpRouteSourceError> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = expr
    else {
        return Err(invalid_rpc(
            path,
            name,
            format!("{field} must be a string literal"),
        ));
    };
    Ok(value.value())
}

fn valid_rpc_key(key: &str) -> bool {
    let segments = key.split('.').collect::<Vec<_>>();
    segments.len() >= 2
        && segments.into_iter().all(|segment| {
            let mut chars = segment.chars();
            matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
                && chars.all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-'
                })
        })
}

fn invalid_rpc(path: &str, name: &str, detail: impl Into<String>) -> HttpRouteSourceError {
    HttpRouteSourceError::InvalidRpcMetadata {
        path: path.to_owned(),
        name: name.to_owned(),
        detail: detail.into(),
    }
}

fn type_source(ty: &Type) -> String {
    // This string is introspection/debug metadata only; rustc remains the type
    // authority when generated glue imports the actual handler function. Using
    // syn's Debug representation avoids a direct `quote` dependency and keeps
    // Cargo.lock stable for this static analyzer.
    format!("{ty:?}")
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
    fn rpc_metadata_is_typed_and_operation_local() {
        let source = r#"
            #[ores_rpc(
                key = "fiducia_cloud.users.find_user_by_id",
                codecs("json", "protobuf", "messagepack"),
                default_codec = "protobuf",
                audiences("browser", "server"),
                scope = "regular"
            )]
            pub async fn get() {}
        "#;
        let analysis = analyze_http_route_source("src/routes/v1/users/[user_id]/route.rs", source)
            .expect("valid route module");
        let rpc = analysis.handlers[0].rpc.as_ref().expect("rpc metadata");
        assert_eq!(rpc.key, "fiducia_cloud.users.find_user_by_id");
        assert_eq!(rpc.codecs, vec!["json", "protobuf", "messagepack"]);
        assert_eq!(rpc.default_codec, "protobuf");
        assert_eq!(rpc.audiences, vec!["browser", "server"]);
    }

    #[test]
    fn rpc_default_codec_must_be_supported() {
        let source = r#"
            #[ores_rpc(
                key = "fiducia_cloud.users.find_user_by_id",
                codecs("json"),
                default_codec = "protobuf"
            )]
            pub async fn get() {}
        "#;
        let error = analyze_http_route_source("src/routes/v1/users/[user_id]/route.rs", source)
            .expect_err("invalid default codec must fail");
        assert!(format!("{error}").contains("default_codec"));
    }

    #[test]
    fn admin_rpc_cannot_generate_browser_client() {
        let source = r#"
            #[ores_rpc(
                key = "fiducia_cloud.admin.users.disable_user",
                audiences("browser", "server"),
                scope = "admin"
            )]
            pub async fn post() {}
        "#;
        let error = analyze_http_route_source("src/routes/v1/users/[id]/disable/route.rs", source)
            .expect_err("admin browser surface must fail");
        assert!(format!("{error}").contains("server-only"));
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
