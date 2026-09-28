//! Static pairing between filesystem HTTP adapters and shared typed operations.
//!
//! New RPC-enabled routes bind transport-independent business logic to one
//! `#[ores_operation(...)]` function. Reserved Axum verb exports use
//! `#[ores_route(operation = ...)]` to bind to that operation. Generated HTTP
//! and RPC adapters then share one deterministic `invoke_*` boundary instead of
//! RPC synthesizing a second HTTP request.

use std::collections::{BTreeMap, BTreeSet};

use syn::{
    punctuated::Punctuated, Expr, ExprLit, Item, Lit, LitStr, Meta, ReturnType, Token, Type,
    Visibility,
};
use thiserror::Error;

use crate::{route_source::HTTP_ROUTE_EXPORTS, HttpResponseFraming};

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
    /// Generated intermediary operation-spec type used by both backend and SDK
    /// generation. Canonical context-centric handlers require this value.
    pub spec: Option<String>,
    pub key: String,
    pub codecs: Vec<String>,
    pub default_codec: String,
    pub audiences: Vec<String>,
    pub scope: String,
    pub stream: String,
    pub parameter_types: Vec<String>,
    pub return_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpOperationAdapterSource {
    pub rust_name: String,
    pub method: String,
    pub operation: String,
    /// Optional authored path projection from `#[ores_route(path = ...)]`.
    pub path: Option<String>,
    /// How the HTTP/Lambda HTTP projection frames the response body.
    ///
    /// This is independent from the semantic RPC stream mode. `single` is the
    /// compatibility default for unary/client-stream response cardinality;
    /// response-streaming operations are rejected later unless they author an
    /// explicit streaming framing.
    pub response_framing: HttpResponseFraming,
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

    #[must_use]
    pub fn adapter_for_method(&self, method: &str) -> Option<&HttpOperationAdapterSource> {
        self.adapters_by_method.get(method)
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
    #[error(
        "{path}: HTTP adapter {handler} references missing #[ores_operation] function {operation}"
    )]
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
                spec: meta.spec,
                key: meta.key,
                codecs: meta.codecs,
                default_codec: meta.default_codec,
                audiences: meta.audiences,
                scope: meta.scope,
                stream: meta.stream,
                parameter_types: parameter_types(function),
                return_type: return_type(function),
            };
            if operations.insert(name.clone(), operation).is_some() {
                return Err(invalid_operation(
                    path,
                    &name,
                    "duplicate operation function",
                ));
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
        let route = parse_route_metadata(path, &name, attr)?;
        if !operations.contains_key(&route.operation) {
            return Err(SharedOperationSourceError::MissingOperation {
                path: path.to_owned(),
                handler: name,
                operation: route.operation,
            });
        }
        if !bound_operations.insert(route.operation.clone()) {
            return Err(SharedOperationSourceError::DuplicateOperationBinding {
                path: path.to_owned(),
                operation: route.operation,
            });
        }
        adapters_by_method.insert(
            (*method).to_owned(),
            HttpOperationAdapterSource {
                rust_name: name,
                method: (*method).to_owned(),
                operation: route.operation,
                path: route.path,
                response_framing: route.response_framing,
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
    spec: Option<String>,
    key: String,
    codecs: Vec<String>,
    default_codec: String,
    audiences: Vec<String>,
    scope: String,
    stream: String,
}

#[derive(Debug)]
struct RouteMeta {
    operation: String,
    path: Option<String>,
    response_framing: HttpResponseFraming,
}

fn parse_operation_attribute(
    path: &str,
    name: &str,
    attr: &syn::Attribute,
) -> Result<OperationMeta, SharedOperationSourceError> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|error| invalid_operation(path, name, error.to_string()))?;
    let mut spec = None;
    let mut key = None;
    let mut default_codec = None;
    let mut scope = None;
    let mut stream = None;
    let mut codecs = None;
    let mut audiences = None;

    for meta in args {
        match meta {
            Meta::NameValue(value) => {
                let field = value
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        invalid_operation(path, name, "metadata keys must be identifiers")
                    })?;
                match field.as_str() {
                    "spec" => {
                        let value = path_expr(&value.value).ok_or_else(|| {
                            invalid_operation(path, name, "spec must be a Rust type path")
                        })?;
                        set_once(path, name, &field, &mut spec, value)?;
                    }
                    "key" | "default_codec" | "scope" | "stream" => {
                        let parsed = string_expr(&value.value).ok_or_else(|| {
                            invalid_operation(
                                path,
                                name,
                                format!("{field} must be a string literal"),
                            )
                        })?;
                        match field.as_str() {
                            "key" => set_once(path, name, &field, &mut key, parsed)?,
                            "default_codec" => {
                                set_once(path, name, &field, &mut default_codec, parsed)?
                            }
                            "scope" => set_once(path, name, &field, &mut scope, parsed)?,
                            "stream" => set_once(path, name, &field, &mut stream, parsed)?,
                            _ => unreachable!(),
                        }
                    }
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
                    .ok_or_else(|| {
                        invalid_operation(path, name, "metadata lists must be identifiers")
                    })?;
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
    validate_values(
        path,
        name,
        "codecs",
        &codecs,
        &["json", "protobuf", "messagepack"],
    )?;
    let default_codec = default_codec.unwrap_or_else(|| codecs[0].clone());
    if !codecs.iter().any(|codec| codec == &default_codec) {
        return Err(invalid_operation(
            path,
            name,
            "default_codec must also appear in codecs(...)",
        ));
    }
    let audiences = audiences.unwrap_or_else(|| vec!["server".to_owned()]);
    validate_values(path, name, "audiences", &audiences, &["browser", "server"])?;
    let scope = scope.unwrap_or_else(|| "regular".to_owned());
    if !matches!(scope.as_str(), "regular" | "admin") {
        return Err(invalid_operation(
            path,
            name,
            "scope must be regular or admin",
        ));
    }
    if scope == "admin" && audiences.iter().any(|audience| audience == "browser") {
        return Err(invalid_operation(
            path,
            name,
            "admin operations are server-only",
        ));
    }
    let stream = stream.unwrap_or_else(|| "unary".to_owned());
    validate_values(
        path,
        name,
        "stream",
        std::slice::from_ref(&stream),
        &["unary", "server_stream", "client_stream", "bidi"],
    )?;
    let key_name = key.rsplit('.').next().unwrap_or(key.as_str());
    let has_stream_suffix = name.ends_with("_stream") || key_name.ends_with("_stream");
    let is_streaming = stream != "unary";
    if has_stream_suffix && !is_streaming {
        return Err(invalid_operation(
            path,
            name,
            "operation name ending in _stream requires explicit non-unary stream metadata",
        ));
    }
    if is_streaming && !has_stream_suffix {
        return Err(invalid_operation(
            path,
            name,
            "non-unary stream operation names must end in _stream",
        ));
    }

    Ok(OperationMeta {
        spec,
        key,
        codecs,
        default_codec,
        audiences,
        scope,
        stream,
    })
}

fn parse_route_metadata(
    path: &str,
    name: &str,
    attr: &syn::Attribute,
) -> Result<RouteMeta, SharedOperationSourceError> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|error| invalid_route(path, name, error.to_string()))?;
    let mut operation = None;
    let mut route_path = None;
    let mut response_framing = None;

    for meta in args {
        let Meta::NameValue(value) = meta else {
            return Err(invalid_route(
                path,
                name,
                "ores_route arguments must be name/value metadata",
            ));
        };
        if value.path.is_ident("operation") {
            let parsed = parse_route_operation_value(path, name, &value.value)?;
            set_route_once(path, name, "operation", &mut operation, parsed)?;
        } else if value.path.is_ident("path") {
            let parsed = string_expr(&value.value).ok_or_else(|| {
                invalid_route(
                    path,
                    name,
                    "path must be a string literal such as \"/v1/users/{id}\"",
                )
            })?;
            validate_route_path(&parsed).map_err(|detail| invalid_route(path, name, detail))?;
            set_route_once(path, name, "path", &mut route_path, parsed)?;
        } else if value.path.is_ident("framing") {
            let parsed = string_expr(&value.value)
                .ok_or_else(|| invalid_route(path, name, "framing must be a string literal"))?;
            let framing = parse_response_framing(&parsed)
                .map_err(|detail| invalid_route(path, name, detail))?;
            set_route_once(path, name, "framing", &mut response_framing, framing)?;
        } else {
            return Err(invalid_route(
                path,
                name,
                "ores_route supports only operation = ..., path = \"...\", and framing = \"...\"",
            ));
        }
    }

    let operation = operation
        .ok_or_else(|| invalid_route(path, name, "ores_route requires operation = <function>"))?;
    Ok(RouteMeta {
        operation,
        path: route_path,
        response_framing: response_framing.unwrap_or(HttpResponseFraming::Single),
    })
}

fn parse_route_operation_value(
    path: &str,
    name: &str,
    value: &Expr,
) -> Result<String, SharedOperationSourceError> {
    match value {
        Expr::Path(expr) if !expr.path.segments.is_empty() => Ok(expr
            .path
            .segments
            .last()
            .expect("non-empty path")
            .ident
            .to_string()),
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => Ok(value
            .value()
            .rsplit("::")
            .next()
            .unwrap_or_default()
            .to_owned()),
        _ => Err(invalid_route(
            path,
            name,
            "operation must be a function path such as handlers::find_user",
        )),
    }
}

fn parse_response_framing(value: &str) -> Result<HttpResponseFraming, String> {
    match value {
        "single" => Ok(HttpResponseFraming::Single),
        "sse" => Ok(HttpResponseFraming::Sse),
        "ndjson" => Ok(HttpResponseFraming::Ndjson),
        "json_seq" => Ok(HttpResponseFraming::JsonSeq),
        "length_delimited" => Ok(HttpResponseFraming::LengthDelimited),
        "raw_chunks" => Ok(HttpResponseFraming::RawChunks),
        other => Err(format!(
            "unsupported ores_route framing {other:?}; expected single, sse, ndjson, json_seq, length_delimited, or raw_chunks"
        )),
    }
}

fn validate_route_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') {
        return Err(format!("ores_route path {path:?} must start with `/`"));
    }
    if path
        .chars()
        .any(|character| character.is_whitespace() || character == '?' || character == '#')
    {
        return Err(format!(
            "ores_route path {path:?} must be a path template only: no whitespace, query or fragment"
        ));
    }
    if path == "/" {
        return Ok(());
    }
    let segments = path[1..].split('/').collect::<Vec<_>>();
    let mut names = BTreeSet::new();
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            return Err(format!(
                "ores_route path {path:?} has an empty segment (doubled or trailing `/`)"
            ));
        }
        let capture = segment
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'));
        match capture {
            Some(inner) => {
                let (capture_name, is_rest) = match inner.strip_prefix('*') {
                    Some(name) => (name, true),
                    None => (inner, false),
                };
                if syn::parse_str::<syn::Ident>(capture_name).is_err() {
                    return Err(format!(
                        "ores_route path {path:?}: capture `{segment}` must name an identifier"
                    ));
                }
                if is_rest && index + 1 != segments.len() {
                    return Err(format!(
                        "ores_route path {path:?}: catch-all `{segment}` must be the last segment"
                    ));
                }
                if !names.insert(capture_name.to_owned()) {
                    return Err(format!(
                        "ores_route path {path:?} captures `{capture_name}` twice"
                    ));
                }
            }
            None if segment.contains('{') || segment.contains('}') => {
                return Err(format!(
                    "ores_route path {path:?}: `{segment}` mixes a literal with a capture; a capture must be a whole segment"
                ));
            }
            None => {}
        }
    }
    Ok(())
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

fn path_expr(expr: &Expr) -> Option<String> {
    let Expr::Path(value) = expr else {
        return None;
    };
    Some(
        value
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
    )
}

fn set_once<T>(
    path: &str,
    name: &str,
    field: &str,
    slot: &mut Option<T>,
    value: T,
) -> Result<(), SharedOperationSourceError> {
    if slot.is_some() {
        return Err(invalid_operation(path, name, format!("duplicate {field}")));
    }
    *slot = Some(value);
    Ok(())
}

fn set_route_once<T>(
    path: &str,
    name: &str,
    field: &str,
    slot: &mut Option<T>,
    value: T,
) -> Result<(), SharedOperationSourceError> {
    if slot.is_some() {
        return Err(invalid_route(path, name, format!("duplicate {field}")));
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
        return Err(invalid_operation(
            path,
            name,
            format!("{field} must not be empty"),
        ));
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

fn invalid_route(path: &str, name: &str, detail: impl Into<String>) -> SharedOperationSourceError {
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
                spec = FindUserOperation,
                key = "fiducia_cloud.users.find_user_by_id",
                codecs("json", "protobuf", "messagepack"),
                default_codec = "protobuf",
                audiences("browser", "server")
            )]
            async fn find_user_by_id(ctx: TypedOperationContext<AppState, FindUserOperation>)
                -> Result<FindUserOutput, FindUserError>
            { todo!() }

            #[ores_route(
                operation = find_user_by_id,
                path = "/v1/users/{user_id}",
                framing = "single"
            )]
            pub async fn get(
                State(state): State<AppState>,
                Path(path): Path<FindUserPath>
            ) -> HttpResult { todo!() }
        "#;
        let analysis =
            analyze_shared_operation_route_source("src/routes/v1/users/[user_id]/route.rs", source)
                .expect("shared operation route");
        let operation = analysis.operation_for_method("GET").expect("operation");
        assert_eq!(operation.rust_name, "find_user_by_id");
        assert_eq!(operation.invoke_name, "__ores_invoke_find_user_by_id");
        assert_eq!(operation.spec.as_deref(), Some("FindUserOperation"));
        assert_eq!(operation.default_codec, "protobuf");
        assert_eq!(operation.audiences, vec!["browser", "server"]);
        let adapter = analysis.adapter_for_method("GET").expect("adapter");
        assert_eq!(adapter.path.as_deref(), Some("/v1/users/{user_id}"));
        assert_eq!(adapter.response_framing, HttpResponseFraming::Single);
    }

    #[test]
    fn streaming_response_framing_is_preserved_by_source_analysis() {
        let source = r#"
            #[ores_operation(
                key = "demo.users.watch_users_stream",
                stream = "server_stream"
            )]
            async fn watch_users_stream(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = watch_users_stream, framing = "ndjson")]
            pub async fn get() {}
        "#;
        let analysis =
            analyze_shared_operation_route_source("src/routes/users/stream/route.rs", source)
                .expect("stream operation source");
        let adapter = analysis.adapter_for_method("GET").expect("adapter");
        assert_eq!(adapter.response_framing, HttpResponseFraming::Ndjson);
    }

    #[test]
    fn route_framing_vocabulary_is_closed() {
        let source = r#"
            #[ores_operation(key = "demo.users.find_user")]
            async fn find_user(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = find_user, framing = "chunked")]
            pub async fn get() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("unknown framing must fail");
        assert!(format!("{error}").contains("unsupported ores_route framing"));
    }

    #[test]
    fn route_path_metadata_is_validated_by_static_analysis() {
        let source = r#"
            #[ores_operation(key = "demo.users.find_user")]
            async fn find_user(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = find_user, path = "v1/users")]
            pub async fn get() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("relative route path must fail");
        assert!(format!("{error}").contains("must start with `/`"));
    }

    #[test]
    fn missing_operation_target_fails_closed() {
        let source = r#"
            #[ores_route(operation = find_user_by_id)]
            pub async fn get() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("missing operation must fail");
        assert!(matches!(
            error,
            SharedOperationSourceError::MissingOperation { .. }
        ));
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
        assert!(matches!(
            error,
            SharedOperationSourceError::UnboundOperation { .. }
        ));
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
    fn stream_suffix_requires_non_unary_metadata() {
        let source = r#"
            #[ores_operation(key = "demo.users.watch_users_stream")]
            async fn watch_users_stream(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = watch_users_stream)]
            pub async fn get() {}
        "#;
        let error =
            analyze_shared_operation_route_source("src/routes/users/stream/route.rs", source)
                .expect_err("stream suffix without stream metadata must fail");
        assert!(format!("{error}").contains("requires explicit non-unary stream metadata"));
    }

    #[test]
    fn non_unary_stream_requires_stream_suffix() {
        let source = r#"
            #[ores_operation(
                key = "demo.users.watch_users",
                stream = "server_stream"
            )]
            async fn watch_users(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = watch_users)]
            pub async fn get() {}
        "#;
        let error =
            analyze_shared_operation_route_source("src/routes/users/stream/route.rs", source)
                .expect_err("non-unary stream without suffix must fail");
        assert!(format!("{error}").contains("must end in _stream"));
    }

    #[test]
    fn server_stream_metadata_is_preserved_by_source_analysis() {
        let source = r#"
            #[ores_operation(
                key = "demo.users.watch_users_stream",
                stream = "server_stream"
            )]
            async fn watch_users_stream(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = watch_users_stream, framing = "sse")]
            pub async fn get() {}
        "#;
        let analysis =
            analyze_shared_operation_route_source("src/routes/users/stream/route.rs", source)
                .expect("stream operation source");
        let operation = analysis
            .operation_for_method("GET")
            .expect("stream operation");
        assert_eq!(operation.stream, "server_stream");
        assert_eq!(
            analysis
                .adapter_for_method("GET")
                .expect("adapter")
                .response_framing,
            HttpResponseFraming::Sse
        );
    }

    #[test]
    fn duplicate_route_projection_fields_fail_closed() {
        let source = r#"
            #[ores_operation(key = "demo.users.find_user")]
            async fn find_user(ctx: OperationContext, input: Input) -> Output { todo!() }

            #[ores_route(operation = find_user, framing = "single", framing = "sse")]
            pub async fn get() {}
        "#;
        let error = analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect_err("duplicate framing must fail");
        assert!(format!("{error}").contains("duplicate framing"));
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
        let error =
            analyze_shared_operation_route_source("src/routes/users/disable/route.rs", source)
                .expect_err("admin browser exposure must fail");
        assert!(format!("{error}").contains("server-only"));
    }
}
