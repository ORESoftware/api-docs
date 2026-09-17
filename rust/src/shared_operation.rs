//! Static pairing between filesystem HTTP adapters and shared typed operations.
//!
//! New RPC-enabled `route.rs` modules should put transport-independent business
//! logic in one `#[ores_operation(...)]` async function. Reserved Axum verb
//! exports use `#[ores_route(operation = ...)]` to bind to that operation.
//! Generated HTTP and RPC adapters then share one deterministic `invoke_*`
//! boundary instead of RPC synthesizing a second HTTP request.

use std::collections::{BTreeMap, BTreeSet};

use syn::{
    punctuated::Punctuated, Expr, ExprLit, Item, Lit, LitStr, Meta, ReturnType, Token, Type,
    Visibility,
};
use thiserror::Error;

use crate::route_source::HTTP_ROUTE_EXPORTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcExecutionModel {
    /// Preferred model: HTTP and RPC adapters invoke the same generated
    /// operation wrapper, which then calls the authored typed operation.
    SharedOperation,
    /// Migration-only compatibility for legacy `#[ores_rpc]` HTTP handlers.
    HttpProjectionLegacy,
}

impl RpcExecutionModel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SharedOperation => "shared_operation",
            Self::HttpProjectionLegacy => "http_projection_legacy",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedOperationSource {
    pub rust_name: String,
    pub invoke_name: String,
    pub key: String,
    pub codecs: Vec<String>,
    pub default_codec: String,
    pub audiences: Vec<String>,
    pub scope: String,
    pub parameter_types: Vec<String>,
    pub return_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpOperationAdapterSource {
    pub rust_name: String,
    pub method: String,
    pub operation: String,
    pub parameter_types: Vec<String>,
    pub return_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedOperationRouteSource {
    pub operations: BTreeMap<String, SharedOperationSource>,
    pub adapters_by_method: BTreeMap<String, HttpOperationAdapterSource>,
}

impl SharedOperationRouteSource {
    #[must_use]
    pub fn operation_for_method(&self, method: &str) -> Option<&SharedOperationSource> {
        let adapter = self.adapters_by_method.get(method)?;
        self.operations.get(&adapter.operation)
    }
}

#[derive(Debug, Error)]
pub enum SharedOperationSourceError {
    #[error("Rust syntax error in {path}: {detail}")]
    Syntax { path: String, detail: String },
    #[error("{path}: invalid #[ores_operation] on {name}: {detail}")]
    InvalidOperationMetadata {
        path: String,
        name: String,
        detail: String,
    },
    #[error("{path}: invalid #[ores_route] on {name}: {detail}")]
    InvalidRouteMetadata {
        path: String,
        name: String,
        detail: String,
    },
    #[error("{path}: HTTP adapter {handler} references missing #[ores_operation] function {operation}")]
    MissingOperation {
        path: String,
        handler: String,
        operation: String,
    },
    #[error("{path}: #[ores_operation] function {operation} is not bound to an HTTP verb with #[ores_route]")]
    UnboundOperation { path: String, operation: String },
    #[error("{path}: #[ores_operation] function {operation} is bound by more than one HTTP verb")]
    DuplicateOperationBinding { path: String, operation: String },
}

pub fn analyze_shared_operation_route_source(
    path: &str,
    source: &str,
) -> Result<SharedOperationRouteSource, SharedOperationSourceError> {
    let file = syn::parse_file(source).map_err(|error| SharedOperationSourceError::Syntax {
        path: path.to_owned(),
        detail: error.to_string(),
    })?;

    let mut operations = BTreeMap::new();
    let mut adapters_by_method = BTreeMap::new();
    let mut bound_operations = BTreeSet::new();

    for item in &file.items {
        let Item::Fn(function) = item else {
            continue;
        };
        let name = function.sig.ident.to_string();

        if let Some(attr) = find_attr(function, "ores_operation") {
            if function.sig.asyncness.is_none() {
                return Err(invalid_operation(path, &name, "operation must be async"));
            }
            if HTTP_ROUTE_EXPORTS.iter().any(|(verb, _)| *verb == name) {
                return Err(invalid_operation(
                    path,
                    &name,
                    "shared operation must not use a reserved HTTP verb name",
                ));
            }
            let meta = parse_operation_attribute(path, &name, attr)?;
            let operation = SharedOperationSource {
                rust_name: name.clone(),
                invoke_name: format!("__ores_invoke_{name}"),
                key: meta.key,
                codecs: meta.codecs,
                default_codec: meta.default_codec,
                audiences: meta.audiences,
                scope: meta.scope,
                parameter_types: parameter_types(function),
                return_type: return_type(function),
            };
            if operations.insert(name.clone(), operation).is_some() {
                return Err(invalid_operation(path, &name, "duplicate operation function"));
            }
        }
    }

    for item in &file.items {
        let Item::Fn(function) = item else {
            continue;
        };
        let name = function.sig.ident.to_string();
        let Some((_, method)) = HTTP_ROUTE_EXPORTS
            .iter()
            .find(|(candidate, _)| *candidate == name)
        else {
            continue;
        };
        let Some(attr) = find_attr(function, "ores_route") else {
            continue;
        };
        if !matches!(function.vis, Visibility::Public(_)) {
            return Err(invalid_route(path, &name, "HTTP adapter must be pub"));
        }
        if function.sig.asyncness.is_none() {
            return Err(invalid_route(path, &name, "HTTP adapter must be async"));
        }
        let operation = parse_route_operation(path, &name, attr)?;
        if !operations.contains_key(&operation) {
            return Err(SharedOperationSourceError::MissingOperation {
                path: path.to_owned(),
                handler: name,
                operation,
            });
        }
        if !bound_operations.insert(operation.clone()) {
            return Err(SharedOperationSourceError::DuplicateOperationBinding {
                path: path.to_owned(),
                operation,
            });
        }
        adapters_by_method.insert(
            (*method).to_owned(),
            HttpOperationAdapterSource {
                rust_name: name,
                method: (*method).to_owned(),
                operation,
                parameter_types: parameter_types(function),
                return_type: return_type(function),
            },
        );
    }

    for operation in operations.keys() {
        if !bound_operations.contains(operation) {
            return Err(SharedOperationSourceError::UnboundOperation {
                path: path.to_owned(),
                operation: operation.clone(),
            });
        }
    }

    Ok(SharedOperationRouteSource {
        operations,
        adapters_by_method,
    })
}

#[derive(Debug)]
struct OperationMeta {
    key: String,
    codecs: Vec<String>,
    default_codec: String,
    audiences: Vec<String>,
    scope: String,
}

fn parse_operation_attribute(
    path: &str,
    name: &str,
    attr: &syn::Attribute,
) -> Result<OperationMeta, SharedOperationSourceError> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|error| invalid_operation(path, name, error.to_string()))?;
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
                    .ok_or_else(|| invalid_operation(path, name, "metadata keys must be identifiers"))?;
                let value = string_expr(&value.value).ok_or_else(|| {
                    invalid_operation(path, name, format!("{field} must be a string literal"))
                })?;
                match field.as_str() {
                    "key" => set_once(path, name, &field, &mut key, value)?,
                    "default_codec" => {
                        set_once(path, name, &field, &mut default_codec, value)?
                    }
                    "scope" => set_once(path, name, &field, &mut scope, value)?,
                    _ => {
                        return Err(invalid_operation(
                            path,
                            name,
                            format!("unsupported ores_operation key {field:?}"),
                        ))
                    }
                }
            }
            Meta::List(list) => {
                let field = list
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| invalid_operation(path, name, "metadata lists must be identifiers"))?;
                let values = list
                    .parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)
                    .map_err(|error| invalid_operation(path, name, error.to_string()))?
                    .into_iter()
                    .map(|value| value.value())
                    .collect::<Vec<_>>();
                match field.as_str() {
                    "codecs" => set_once(path, name, &field, &mut codecs, values)?,
                    "audiences" => set_once(path, name, &field, &mut audiences, values)?,
                    _ => {
                        return Err(invalid_operation(
                            path,
                            name,
                            format!("unsupported ores_operation list {field:?}"),
                        ))
                    }
                }
            }
            Meta::Path(_) => {
                return Err(invalid_operation(
                    path,
                    name,
                    "bare ores_operation flags are not supported",
                ))
            }
        }
    }

    let key = key.ok_or_else(|| invalid_operation(path, name, "key is required"))?;
    if !valid_rpc_key(&key) {
        return Err(invalid_operation(
            path,
            name,
            "key must be a stable dotted lowercase object key",
        ));
    }
    let codecs = codecs.unwrap_or_else(|| vec!["json".to_owned()]);
    validate_values(path, name, "codecs", &codecs, &["json", "protobuf", "messagepack"])?;
    let default_codec = default_codec.unwrap_or_else(|| codecs[0].clone());
    if !codecs.iter().any(|codec| codec == &default_codec) {
        return Err(invalid_operation(
            path,
            name,
            "default_codec must also appear in codecs(...)" ,
        ));
    }
    let audiences = audiences.unwrap_or_else(|| vec!["server".to_owned()]);
    validate_values(path, name, "audiences", &audiences, &["browser", "server"])?;
    let scope = scope.unwrap_or_else(|| "regular".to_owned());
    if !matches!(scope.as_str(), "regular" | "admin") {
        return Err(invalid_operation(path, name, "scope must be regular or admin"));
    }
    if scope == "admin" && audiences.iter().any(|audience| audience == "browser") {
        return Err(invalid_operation(
            path,
            name,
            "admin operations are server-only",
        ));
    }

    Ok(OperationMeta {
        key,
        codecs,
        default_codec,
        audiences,
        scope,
    })
}

fn parse_route_operation(
    path: &str,
    name: &str,
    attr: &syn::Attribute,
) -> Result<String, SharedOperationSourceError> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|error| invalid_route(path, name, error.to_string()))?;
    if args.len() != 1 {
        return Err(invalid_route(
            path,
            name,
            "ores_route requires exactly operation = <function>",
        ));
    }
    let Meta::NameValue(value) = &args[0] else {
        return Err(invalid_route(
            path,
            name,
            "ores_route requires operation = <function>",
        ));
    };
    if !value.path.is_ident("operation") {
        return Err(invalid_route(path, name, "only operation = ... is supported"));
    }
    match &value.value {
        Expr::Path(expr) if expr.path.segments.len() == 1 => {
            Ok(expr.path.segments[0].ident.to_string())
        }
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(invalid_route(
            path,
            name,
            "operation must be one local function identifier",
        )),
    }
}

fn find_attr<'a>(function: &'a syn::ItemFn, name: &str) -> Option<&'a syn::Attribute> {
    function.attrs.iter().find(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == name)
    })
}

fn parameter_types(function: &syn::ItemFn) -> Vec<String> {
    function
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Receiver(_) => None,
            syn::FnArg::Typed(typed) => Some(type_source(&typed.ty)),
        })
        .collect()
}

fn return_type(function: &syn::ItemFn) -> Option<String> {
    match &function.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(type_source(ty)),
    }
}

fn string_expr(expr: &Expr) -> Option<String> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = expr
    else {
        return None;
    };
    Some(value.value())
}

fn set_once<T>(
    path: &str,
    name: &str,
    field: &str,
    slot: &mut Option<T>,
    value: T,
) -> Result<(), SharedOperationSourceError> {
    if slot.is_some() {
        return Err(invalid_operation(
            path,
            name,
            format!("duplicate {field}"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn validate_values(
    path: &str,
    name: &str,
    field: &str,
    values: &[String],
    allowed: &[&str],
) -> Result<(), SharedOperationSourceError> {
    if values.is_empty() {
        return Err(invalid_operation(path, name, format!("{field} must not be empty")));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !allowed.contains(&value.as_str()) {
            return Err(invalid_operation(
                path,
                name,
                format!("unsupported {field} value {value:?}"),
            ));
        }
        if !seen.insert(value) {
            return Err(invalid_operation(
                path,
                name,
                format!("duplicate {field} value {value:?}"),
            ));
        }
    }
    Ok(())
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

fn invalid_operation(
    path: &str,
    name: &str,
    detail: impl Into<String>,
) -> SharedOperationSourceError {
    SharedOperationSourceError::InvalidOperationMetadata {
        path: path.to_owned(),
        name: name.to_owned(),
        detail: detail.into(),
    }
}

fn invalid_route(
    path: &str,
    name: &str,
    detail: impl Into<String>,
) -> SharedOperationSourceError {
    SharedOperationSourceError::InvalidRouteMetadata {
        path: path.to_owned(),
        name: name.to_owned(),
        detail: detail.into(),
    }
}

fn type_source(ty: &Type) -> String {
    format!("{ty:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_and_rpc_bind_to_one_shared_operation() {
        let source = r#"
            #[ores_operation(
                key = "fiducia_cloud.users.find_user_by_id",
                codecs("json", "protobuf", "messagepack"),
                default_codec = "protobuf",
                audiences("browser", "server")
            )]
            async fn find_user_by_id(ctx: OperationContext, input: FindUserInput)
                -> Result<FindUserOutput, FindUserError>
            { todo!() }

            #[ores_route(operation = find_user_by_id)]
            pub async fn get(
                State(state): State<AppState>,
                Path(path): Path<FindUserPath>
            ) -> HttpResult { todo!() }
        "#;
        let analysis = analyze_shared_operation_route_source(
            "src/routes/v1/users/[user_id]/route.rs",
            source,
        )
        .expect("shared operation route");
        let operation = analysis.operation_for_method("GET").expect("operation");
        assert_eq!(operation.rust_name, "find_user_by_id");
        assert_eq!(operation.invoke_name, "__ores_invoke_find_user_by_id");
        assert_eq!(operation.default_codec, "protobuf");
        assert_eq!(operation.audiences, vec!["browser", "server"]);
    }

    #[test]
    fn missing_operation_target_fails_closed() {
        let source = r#"
            #[ores_route(operation = find_user_by_id)]
            pub async fn get() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("missing operation must fail");
        assert!(matches!(error, SharedOperationSourceError::MissingOperation { .. }));
    }

    #[test]
    fn unbound_operation_fails_closed() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.find_user_by_id")]
            async fn find_user_by_id(ctx: OperationContext, input: FindUserInput) -> Output {
                todo!()
            }

            pub async fn get() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("unbound operation must fail");
        assert!(matches!(error, SharedOperationSourceError::UnboundOperation { .. }));
    }

    #[test]
    fn same_operation_cannot_back_two_verbs() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.upsert_user")]
            async fn upsert_user(ctx: OperationContext, input: UserInput) -> UserOutput {
                todo!()
            }
            #[ores_route(operation = upsert_user)]
            pub async fn post() {}
            #[ores_route(operation = upsert_user)]
            pub async fn put() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("duplicate binding must fail");
        assert!(matches!(
            error,
            SharedOperationSourceError::DuplicateOperationBinding { .. }
        ));
    }

    #[test]
    fn admin_operation_cannot_target_browser() {
        let source = r#"
            #[ores_operation(
                key = "fiducia_cloud.admin.users.disable_user",
                audiences("browser", "server"),
                scope = "admin"
            )]
            async fn disable_user(ctx: OperationContext, input: DisableUserInput) -> Output {
                todo!()
            }
            #[ores_route(operation = disable_user)]
            pub async fn post() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/disable/route.rs", source)
            .expect_err("admin browser exposure must fail");
        assert!(format!("{error}").contains("server-only"));
    }
}
