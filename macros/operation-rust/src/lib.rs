#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprLit, FnArg, GenericArgument, ItemFn, Lit, LitStr, Meta, MetaNameValue, Pat,
    PathArguments, ReturnType, Token, Type, Visibility,
};

#[derive(Debug)]
struct ParsedOperation {
    spec: Option<Type>,
    key: String,
    codecs: Vec<String>,
    default_codec: String,
    audiences: Vec<String>,
    scope: String,
    stream: String,
}

#[proc_macro_attribute]
pub fn ores_operation(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_operation(&args, &item).and_then(|meta| expand_operation(meta, item)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ores_route(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_route(&args, &item) {
        Ok(operation) => {
            let _ = operation;
            quote!(#item).into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_operation(meta: ParsedOperation, item: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    let operation_name = item.sig.ident.clone();
    let invoke_name = format_ident!("__ores_invoke_{}", operation_name);

    // Canonical form: one typed context argument. The generated invoker becomes
    // the mandatory shared policy boundary and its return type is taken from the
    // generated OperationSpec, not duplicated from handwritten metadata.
    if item.sig.inputs.len() == 1 {
        let spec_ty = meta.spec.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                &item.sig.ident,
                "canonical ores_operation requires spec = GeneratedOperationSpec",
            )
        })?;
        let FnArg::Typed(context) = item.sig.inputs.first().expect("one input") else {
            unreachable!("receivers rejected during validation")
        };
        let Pat::Ident(context_pat) = context.pat.as_ref() else {
            unreachable!("simple identifiers required during validation")
        };
        let context_name = context_pat.ident.clone();
        let context_ty = context.ty.as_ref();

        let descriptor_name = format_ident!(
            "__ORES_OPERATION_DESCRIPTOR_{}",
            operation_name.to_string().to_ascii_uppercase()
        );
        let assert_name = format_ident!("__ores_assert_spec_{}", operation_name);
        let key = LitStr::new(&meta.key, operation_name.span());
        let default_codec = LitStr::new(&meta.default_codec, operation_name.span());
        let scope = LitStr::new(&meta.scope, operation_name.span());
        let stream_mode = match meta.stream.as_str() {
            "unary" => quote!(::ores_api_docs::RpcStreamMode::Unary),
            "server_stream" => quote!(::ores_api_docs::RpcStreamMode::ServerStream),
            "client_stream" => quote!(::ores_api_docs::RpcStreamMode::ClientStream),
            "bidi" => quote!(::ores_api_docs::RpcStreamMode::Bidi),
            _ => unreachable!("stream mode validated before expansion"),
        };
        let codecs = meta
            .codecs
            .iter()
            .map(|value| LitStr::new(value, operation_name.span()))
            .collect::<Vec<_>>();
        let audiences = meta
            .audiences
            .iter()
            .map(|value| LitStr::new(value, operation_name.span()))
            .collect::<Vec<_>>();

        let descriptor = quote! {
            #[doc(hidden)]
            static #descriptor_name: ::ores_api_docs::OperationDescriptor =
                ::ores_api_docs::OperationDescriptor {
                    key: #key,
                    codecs: &[#(#codecs),*],
                    default_codec: #default_codec,
                    audiences: &[#(#audiences),*],
                    scope: #scope,
                    stream: #stream_mode,
                };
        };

        return match meta.stream.as_str() {
            "unary" => {
                let (success_ty, failure_ty) = result_types(&item.sig.output)?;
                Ok(quote! {
                    #item

                    // This non-generic where-clause is checked when the item is
                    // compiled. A handwritten unary handler therefore cannot
                    // return a body or error type different from the generated
                    // client/backend contract.
                    #[doc(hidden)]
                    fn #assert_name()
                    where
                        #spec_ty: ::ores_api_docs::OperationSpec<
                            ResponseBody = #success_ty,
                            Error = #failure_ty,
                        >,
                    {
                    }

                    #descriptor

                    /// Generated shared unary operation boundary. HTTP and RPC
                    /// adapters must call this function; it executes the same
                    /// policy hook before the authored semantic operation.
                    #[doc(hidden)]
                    pub(crate) async fn #invoke_name(
                        #context_name: #context_ty,
                    ) -> ::core::result::Result<
                        <#spec_ty as ::ores_api_docs::OperationSpec>::ResponseBody,
                        ::ores_api_docs::OperationInvokeError<
                            <#spec_ty as ::ores_api_docs::OperationSpec>::Error
                        >,
                    > {
                        #assert_name();
                        ::ores_api_docs::typed_operation_context::invoke_typed_context_operation(
                            &#descriptor_name,
                            #context_name,
                            #operation_name,
                        )
                        .await
                    }
                })
            }
            "server_stream" => Ok(quote! {
                #item

                #descriptor

                /// Generated shared server-stream boundary. The authored async
                /// handler must return exactly the operation-typed stream shape;
                /// the helper's Future bound makes metadata/type disagreement a
                /// Rust compile error rather than a generator/runtime trap.
                #[doc(hidden)]
                pub(crate) async fn #invoke_name(
                    #context_name: #context_ty,
                ) -> ::core::result::Result<
                    ::ores_api_docs::operation_server_stream::ServerStreamResult<#spec_ty>,
                    ::ores_api_docs::OperationInvokeError<
                        <#spec_ty as ::ores_api_docs::OperationSpec>::Error
                    >,
                > {
                    ::ores_api_docs::typed_operation_context::invoke_typed_context_server_stream_operation(
                        &#descriptor_name,
                        #context_name,
                        #operation_name,
                    )
                    .await
                }
            }),
            "client_stream" | "bidi" => Err(syn::Error::new_spanned(
                &item.sig.output,
                format!(
                    "#[ores_operation(stream = {:?})] does not yet have a canonical authored Rust handler ABI; refusing to compile instead of treating it as unary",
                    meta.stream
                ),
            )),
            _ => unreachable!("stream mode validated before expansion"),
        };
    }

    // Migration-only compatibility: preserve the existing two-argument shape
    // exactly while product routes move to TypedOperationContext<State, Spec>.
    let mut invoke_sig = item.sig.clone();
    invoke_sig.ident = invoke_name;
    let mut arguments = Vec::new();
    for input in &item.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "ores_operation does not accept self receivers",
            ));
        };
        let Pat::Ident(ident) = typed.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "ores_operation parameters must use simple identifier patterns",
            ));
        };
        arguments.push(ident.ident.clone());
    }

    Ok(quote! {
        #item

        /// Migration compatibility shared boundary. New routes should use the
        /// one-argument TypedOperationContext form so policy is generated here.
        #[doc(hidden)]
        pub(crate) #invoke_sig {
            #operation_name(#(#arguments),*).await
        }
    })
}

fn validate_operation(
    args: &Punctuated<Meta, Token![,]>,
    item: &ItemFn,
) -> syn::Result<ParsedOperation> {
    if item.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &item.sig.fn_token,
            "#[ores_operation] requires an async function",
        ));
    }
    let name = item.sig.ident.to_string();
    if matches!(
        name.as_str(),
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
    ) {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_operation] belongs on the shared inner operation, not a reserved HTTP verb",
        ));
    }
    if !matches!(item.sig.inputs.len(), 1 | 2) {
        return Err(syn::Error::new_spanned(
            &item.sig.inputs,
            "ores_operation requires canonical one-argument TypedOperationContext or migration two-argument OperationContext + input",
        ));
    }
    for input in &item.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "ores_operation does not accept self receivers",
            ));
        };
        if !matches!(typed.pat.as_ref(), Pat::Ident(_)) {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "ores_operation parameters must use simple identifier patterns",
            ));
        }
    }

    let FnArg::Typed(first) = item.sig.inputs.first().expect("input checked") else {
        unreachable!("receiver rejected")
    };
    let context_spec = if item.sig.inputs.len() == 1 {
        Some(typed_context_spec(first.ty.as_ref())?)
    } else {
        if !type_ends_with(first.ty.as_ref(), "OperationContext") {
            return Err(syn::Error::new_spanned(
                &first.ty,
                "migration ores_operation first argument must be OperationContext<...>",
            ));
        }
        None
    };

    if matches!(item.sig.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            "ores_operation must declare a typed result",
        ));
    }

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
                        syn::Error::new_spanned(
                            &value.path,
                            "ores_operation metadata keys must be identifiers",
                        )
                    })?;
                match field.as_str() {
                    "spec" => {
                        let parsed = type_value(value, &field)?;
                        set_once(&mut spec, parsed, value, &field)?;
                    }
                    "key" => {
                        let parsed = string_value(value, &field)?;
                        set_once(&mut key, parsed, value, &field)?;
                    }
                    "default_codec" => {
                        let parsed = string_value(value, &field)?;
                        set_once(&mut default_codec, parsed, value, &field)?;
                    }
                    "scope" => {
                        let parsed = string_value(value, &field)?;
                        set_once(&mut scope, parsed, value, &field)?;
                    }
                    "stream" => {
                        let parsed = string_value(value, &field)?;
                        set_once(&mut stream, parsed, value, &field)?;
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &value.path,
                            format!("unsupported ores_operation key `{field}`"),
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
                        syn::Error::new_spanned(
                            &list.path,
                            "ores_operation metadata lists must be identifiers",
                        )
                    })?;
                let values = list
                    .parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)?
                    .into_iter()
                    .map(|value| value.value())
                    .collect::<Vec<_>>();
                match field.as_str() {
                    "codecs" => set_once(&mut codecs, values, list, &field)?,
                    "audiences" => set_once(&mut audiences, values, list, &field)?,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &list.path,
                            format!("unsupported ores_operation list `{field}`"),
                        ))
                    }
                }
            }
            Meta::Path(path) => {
                return Err(syn::Error::new_spanned(
                    path,
                    "bare ores_operation flags are not supported",
                ))
            }
        }
    }

    if let Some(context_spec) = context_spec.as_ref() {
        let declared_spec = spec.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                item,
                "canonical ores_operation requires spec = GeneratedOperationSpec",
            )
        })?;
        if type_source(declared_spec) != type_source(context_spec) {
            return Err(syn::Error::new_spanned(
                &first.ty,
                format!(
                    "ores_operation spec {} disagrees with TypedOperationContext operation type {}",
                    type_source(declared_spec),
                    type_source(context_spec)
                ),
            ));
        }
    }

    let key = key.ok_or_else(|| syn::Error::new_spanned(item, "ores_operation requires key"))?;
    if !valid_rpc_key(&key) {
        return Err(syn::Error::new_spanned(
            item,
            "ores_operation key must be a stable dotted lowercase object key",
        ));
    }
    let codecs = codecs.unwrap_or_else(|| vec!["json".to_owned()]);
    validate_values(
        item,
        "codecs",
        &codecs,
        &["json", "protobuf", "messagepack"],
    )?;
    let default_codec = default_codec.unwrap_or_else(|| codecs[0].clone());
    if !codecs.iter().any(|codec| codec == &default_codec) {
        return Err(syn::Error::new_spanned(
            item,
            "ores_operation default_codec must also appear in codecs(...)",
        ));
    }
    let audiences = audiences.unwrap_or_else(|| vec!["server".to_owned()]);
    validate_values(item, "audiences", &audiences, &["browser", "server"])?;
    let scope = scope.unwrap_or_else(|| "regular".to_owned());
    if !matches!(scope.as_str(), "regular" | "admin") {
        return Err(syn::Error::new_spanned(
            item,
            "ores_operation scope must be regular or admin",
        ));
    }
    if scope == "admin" && audiences.iter().any(|audience| audience == "browser") {
        return Err(syn::Error::new_spanned(
            item,
            "admin ores_operation functions are server-only",
        ));
    }
    let stream = stream.unwrap_or_else(|| "unary".to_owned());
    validate_values(
        item,
        "stream",
        std::slice::from_ref(&stream),
        &["unary", "server_stream", "client_stream", "bidi"],
    )?;
    let key_name = key.rsplit('.').next().unwrap_or(key.as_str());
    let has_stream_suffix = name.ends_with("_stream") || key_name.ends_with("_stream");
    let is_streaming = stream != "unary";
    if has_stream_suffix && !is_streaming {
        return Err(syn::Error::new_spanned(
            item,
            "ores_operation names ending in _stream require explicit non-unary stream metadata",
        ));
    }
    if is_streaming && !has_stream_suffix {
        return Err(syn::Error::new_spanned(
            item,
            "non-unary ores_operation names must end in _stream",
        ));
    }

    if context_spec.is_some() {
        match stream.as_str() {
            "unary" => {
                result_types(&item.sig.output)?;
            }
            "server_stream" => {
                reject_unary_result_for_server_stream(&item.sig.output)?;
            }
            "client_stream" | "bidi" => {
                return Err(syn::Error::new_spanned(
                    &item.sig.output,
                    format!(
                        "#[ores_operation(stream = {stream:?})] does not yet have a canonical authored Rust handler ABI; refusing to compile instead of treating it as unary"
                    ),
                ));
            }
            _ => unreachable!("stream mode validated above"),
        }
    }

    Ok(ParsedOperation {
        spec,
        key,
        codecs,
        default_codec,
        audiences,
        scope,
        stream,
    })
}

fn validate_route(args: &Punctuated<Meta, Token![,]>, item: &ItemFn) -> syn::Result<String> {
    let name = item.sig.ident.to_string();
    if !matches!(
        name.as_str(),
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
    ) {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_route] must annotate a reserved route.rs HTTP verb export",
        ));
    }
    if !matches!(item.vis, Visibility::Public(_)) {
        return Err(syn::Error::new_spanned(
            &item.vis,
            "ores_route HTTP adapter must be pub",
        ));
    }
    if item.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &item.sig.fn_token,
            "ores_route HTTP adapter must be async",
        ));
    }
    // `operation = <fn>` is required. `path = "/…"` is optional HTTP-projection
    // metadata: the template this adapter is mounted at. It lets a generator
    // read the projection from the adapter itself, instead of requiring the
    // `Router::route(...)` registration to live in the same file -- services
    // that centralize routing keep doing so. The verb is still the function
    // name, so it is not repeated here.
    let mut operation = None::<String>;
    let mut path = None::<String>;
    for meta in args {
        let Meta::NameValue(value) = meta else {
            return Err(syn::Error::new_spanned(
                meta,
                "ores_route arguments are `operation = <local function>` and optionally `path = \"/...\"`",
            ));
        };
        if value.path.is_ident("operation") {
            if operation.is_some() {
                return Err(syn::Error::new_spanned(&value.path, "ores_route `operation` is given twice"));
            }
            operation = Some(match &value.value {
                Expr::Path(expr) if !expr.path.segments.is_empty() => {
                    expr.path.to_token_stream().to_string()
                }
                Expr::Lit(ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) => value.value(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        &value.value,
                        "ores_route operation must be a function path such as handlers::find_user",
                    ))
                }
            });
        } else if value.path.is_ident("path") {
            if path.is_some() {
                return Err(syn::Error::new_spanned(&value.path, "ores_route `path` is given twice"));
            }
            let Expr::Lit(ExprLit {
                lit: Lit::Str(literal),
                ..
            }) = &value.value
            else {
                return Err(syn::Error::new_spanned(
                    &value.value,
                    "ores_route path must be a string literal such as \"/v1/users/{id}\"",
                ));
            };
            validate_route_path(&literal.value())
                .map_err(|message| syn::Error::new_spanned(literal, message))?;
            path = Some(literal.value());
        } else {
            return Err(syn::Error::new_spanned(
                &value.path,
                "ores_route supports only `operation = ...` and `path = \"...\"`",
            ));
        }
    }
    let _ = path;
    operation.ok_or_else(|| {
        syn::Error::new_spanned(item, "ores_route requires operation = <local function>")
    })
}

/// A route template in the Axum 0.8 syntax the services already author:
/// literal segments, `{name}` for one segment, `{*name}` for the rest of the
/// path (last segment only). Rejected here so a typo is a compile error in the
/// adapter, not a route that silently never matches.
fn validate_route_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') {
        return Err(format!("ores_route path {path:?} must start with `/`"));
    }
    if path.chars().any(|character| character.is_whitespace() || character == '?' || character == '#') {
        return Err(format!(
            "ores_route path {path:?} must be a path template only: no whitespace, query or fragment"
        ));
    }
    if path == "/" {
        return Ok(());
    }
    let segments = path[1..].split('/').collect::<Vec<_>>();
    let mut names = std::collections::BTreeSet::new();
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            return Err(format!(
                "ores_route path {path:?} has an empty segment (doubled or trailing `/`)"
            ));
        }
        let capture = segment.strip_prefix('{').and_then(|rest| rest.strip_suffix('}'));
        match capture {
            Some(inner) => {
                let (name, is_rest) = match inner.strip_prefix('*') {
                    Some(name) => (name, true),
                    None => (inner, false),
                };
                if syn::parse_str::<syn::Ident>(name).is_err() {
                    return Err(format!(
                        "ores_route path {path:?}: capture `{segment}` must name an identifier"
                    ));
                }
                if is_rest && index + 1 != segments.len() {
                    return Err(format!(
                        "ores_route path {path:?}: catch-all `{segment}` must be the last segment"
                    ));
                }
                if !names.insert(name.to_owned()) {
                    return Err(format!("ores_route path {path:?} captures `{name}` twice"));
                }
            }
            None if segment.contains('{') || segment.contains('}') => {
                return Err(format!(
                    "ores_route path {path:?}: `{segment}` mixes a literal with a capture; \
                     a capture must be a whole segment"
                ));
            }
            None => {}
        }
    }
    Ok(())
}

fn result_types(output: &ReturnType) -> syn::Result<(Type, Type)> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "canonical unary ores_operation must return Result<Success, Error>",
        ));
    };
    let Type::Path(path) = ty.as_ref() else {
        return Err(syn::Error::new_spanned(
            ty,
            "canonical unary ores_operation must return Result<Success, Error>",
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "missing result type"));
    };
    if segment.ident != "Result" {
        return Err(syn::Error::new_spanned(
            ty,
            "canonical unary ores_operation must return Result<Success, Error>",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            &segment.arguments,
            "Result must declare success and error types",
        ));
    };
    let types = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if types.len() != 2 {
        return Err(syn::Error::new_spanned(
            arguments,
            "Result must declare exactly success and error types",
        ));
    }
    Ok((types[0].clone(), types[1].clone()))
}

fn reject_unary_result_for_server_stream(output: &ReturnType) -> syn::Result<()> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "#[ores_operation(stream = \"server_stream\")] requires an explicit server-stream return type such as ServerStreamResult<OperationSpec>",
        ));
    };
    if type_ends_with(ty.as_ref(), "Result") {
        return Err(syn::Error::new_spanned(
            ty,
            "#[ores_operation(stream = \"server_stream\")] cannot return unary Result<Success, Error>; return ServerStreamResult<OperationSpec> (or the equivalent OperationServerStream<ResponseBody, Error>) so cargo check can couple stream metadata to the Rust type",
        ));
    }
    Ok(())
}

fn typed_context_spec(ty: &Type) -> syn::Result<Type> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "canonical ores_operation argument must be TypedOperationContext<State, OperationSpec>",
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "missing context type"));
    };
    if segment.ident != "TypedOperationContext" {
        return Err(syn::Error::new_spanned(
            ty,
            "canonical ores_operation argument must be TypedOperationContext<State, OperationSpec>",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            &segment.arguments,
            "TypedOperationContext must declare State and OperationSpec types",
        ));
    };
    let types = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if types.len() != 2 {
        return Err(syn::Error::new_spanned(
            arguments,
            "TypedOperationContext must declare exactly State and OperationSpec types",
        ));
    }
    Ok(types[1].clone())
}

fn type_ends_with(ty: &Type, expected: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == expected)
}

fn type_value(value: &MetaNameValue, field: &str) -> syn::Result<Type> {
    syn::parse2::<Type>(value.value.to_token_stream()).map_err(|_| {
        syn::Error::new_spanned(
            &value.value,
            format!("{field} must be a Rust type path such as CreateUserOperation"),
        )
    })
}

fn type_source(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(' ', "")
}

fn string_value(value: &MetaNameValue, field: &str) -> syn::Result<String> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &value.value
    else {
        return Err(syn::Error::new_spanned(
            &value.value,
            format!("{field} must be a string literal"),
        ));
    };
    Ok(value.value())
}

fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    span: impl quote::ToTokens,
    field: &str,
) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(
            span,
            format!("duplicate metadata field `{field}`"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn validate_values(
    item: &ItemFn,
    name: &str,
    values: &[String],
    allowed: &[&str],
) -> syn::Result<()> {
    if values.is_empty() {
        return Err(syn::Error::new_spanned(
            item,
            format!("ores_operation {name}(...) must not be empty"),
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !allowed.contains(&value.as_str()) {
            return Err(syn::Error::new_spanned(
                item,
                format!("unsupported ores_operation {name} value {value:?}"),
            ));
        }
        if !seen.insert(value) {
            return Err(syn::Error::new_spanned(
                item,
                format!("duplicate ores_operation {name} value {value:?}"),
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

#[cfg(test)]
mod stream_metadata_tests {
    use super::*;
    use syn::parse::Parser;

    fn args(source: &str) -> Punctuated<Meta, Token![,]> {
        Punctuated::<Meta, Token![,]>::parse_terminated
            .parse_str(source)
            .expect("operation metadata")
    }

    fn operation(name: &str) -> ItemFn {
        syn::parse_str(&format!(
            "async fn {name}(ctx: OperationContext, input: Input) -> Output {{ todo!() }}"
        ))
        .expect("operation function")
    }

    fn canonical_operation(name: &str, output: &str) -> ItemFn {
        syn::parse_str(&format!(
            "async fn {name}(ctx: TypedOperationContext<State, WatchEvents>) -> {output} {{ todo!() }}"
        ))
        .expect("canonical operation function")
    }

    #[test]
    fn accepts_explicit_server_stream_with_stream_suffix() {
        let item = operation("watch_users_stream");
        let parsed = validate_operation(
            &args(
                r#"key = "demo.users.watch_users_stream", stream = "server_stream""#,
            ),
            &item,
        )
        .expect("valid stream metadata");
        assert_eq!(parsed.stream, "server_stream");
    }

    #[test]
    fn canonical_server_stream_rejects_unary_result_shape() {
        let item = canonical_operation("watch_events_stream", "Result<WatchEvent, WatchError>");
        let error = validate_operation(
            &args(
                r#"spec = WatchEvents, key = "demo.events.watch_events_stream", stream = "server_stream""#,
            ),
            &item,
        )
        .expect_err("server stream metadata must reject unary Result shape");
        assert!(error.to_string().contains("cannot return unary Result"));
    }

    #[test]
    fn canonical_server_stream_accepts_operation_typed_stream_shape() {
        let item = canonical_operation(
            "watch_events_stream",
            "ServerStreamResult<WatchEvents>",
        );
        validate_operation(
            &args(
                r#"spec = WatchEvents, key = "demo.events.watch_events_stream", stream = "server_stream""#,
            ),
            &item,
        )
        .expect("server stream return shape must be admitted");
    }

    #[test]
    fn canonical_client_stream_fails_closed_until_handler_abi_exists() {
        let item = canonical_operation("upload_events_stream", "SomeStreamType");
        let error = validate_operation(
            &args(
                r#"spec = WatchEvents, key = "demo.events.upload_events_stream", stream = "client_stream""#,
            ),
            &item,
        )
        .expect_err("client stream must not silently use unary ABI");
        assert!(error.to_string().contains("does not yet have a canonical authored Rust handler ABI"));
    }

    #[test]
    fn rejects_stream_suffix_that_defaults_to_unary() {
        let item = operation("watch_users_stream");
        let error = validate_operation(
            &args(r#"key = "demo.users.watch_users_stream""#),
            &item,
        )
        .expect_err("stream suffix must require explicit stream mode");
        assert!(error
            .to_string()
            .contains("require explicit non-unary stream metadata"));
    }

    #[test]
    fn rejects_non_unary_mode_without_stream_suffix() {
        let item = operation("watch_users");
        let error = validate_operation(
            &args(r#"key = "demo.users.watch_users", stream = "server_stream""#),
            &item,
        )
        .expect_err("non-unary mode must use stream suffix");
        assert!(error.to_string().contains("must end in _stream"));
    }
}

#[cfg(test)]
mod route_metadata_tests {
    use super::*;
    use syn::parse::Parser;

    fn args(source: &str) -> Punctuated<Meta, Token![,]> {
        Punctuated::<Meta, Token![,]>::parse_terminated
            .parse_str(source)
            .expect("route metadata")
    }

    fn adapter(name: &str) -> ItemFn {
        syn::parse_str(&format!(
            "pub async fn {name}(req: Request) -> Response {{ todo!() }}"
        ))
        .expect("route adapter")
    }

    fn error(source: &str) -> String {
        validate_route(&args(source), &adapter("get"))
            .expect_err("must be rejected")
            .to_string()
    }

    /// The pre-existing form must keep compiling unchanged.
    #[test]
    fn operation_alone_is_still_accepted() {
        let operation = validate_route(&args("operation = handlers::find_user"), &adapter("get"))
            .expect("valid");
        assert_eq!(operation.replace(' ', ""), "handlers::find_user");
    }

    #[test]
    fn path_is_optional_metadata_in_either_order() {
        for source in [
            r#"operation = handlers::find_user, path = "/v1/users/{id}""#,
            r#"path = "/v1/users/{id}", operation = handlers::find_user"#,
            r#"operation = handlers::get_file, path = "/v1/files/{*rest}""#,
            r#"operation = handlers::root, path = "/""#,
        ] {
            validate_route(&args(source), &adapter("get")).unwrap_or_else(|error| {
                panic!("{source} should be valid: {error}");
            });
        }
    }

    #[test]
    fn malformed_templates_are_compile_errors() {
        let cases = [
            (
                r#"operation = h::f, path = "v1/users""#,
                "must start with `/`",
            ),
            (r#"operation = h::f, path = "/v1/users/""#, "empty segment"),
            (r#"operation = h::f, path = "/v1//users""#, "empty segment"),
            (
                r#"operation = h::f, path = "/v1/users?x=1""#,
                "no whitespace, query or fragment",
            ),
            (
                r#"operation = h::f, path = "/v1/{*rest}/more""#,
                "must be the last segment",
            ),
            (
                r#"operation = h::f, path = "/v1/{id}/x/{id}""#,
                "captures `id` twice",
            ),
            (
                r#"operation = h::f, path = "/v1/user-{id}""#,
                "must be a whole segment",
            ),
            (
                r#"operation = h::f, path = "/v1/{}""#,
                "must name an identifier",
            ),
            (r#"operation = h::f, path = 7"#, "must be a string literal"),
        ];
        for (source, expected) in cases {
            let message = error(source);
            assert!(message.contains(expected), "{source}: {message}");
        }
    }

    #[test]
    fn unknown_repeated_and_missing_arguments_are_rejected() {
        assert!(error(r#"operation = h::f, method = "GET""#).contains("supports only"));
        assert!(error(r#"operation = h::f, operation = h::g"#).contains("given twice"));
        assert!(error(r#"operation = h::f, path = "/a", path = "/b""#).contains("given twice"));
        assert!(error(r#"path = "/a""#).contains("requires operation"));
    }
}
