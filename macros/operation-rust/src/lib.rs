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
        let (success_ty, failure_ty) = result_types(&item.sig.output)?;

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

        return Ok(quote! {
            #item

            // This non-generic where-clause is checked when the item is
            // compiled. A handwritten handler therefore cannot return a body or
            // error type different from the generated client/backend contract.
            #[doc(hidden)]
            fn #assert_name()
            where
                #spec_ty: ::ores_api_docs::OperationSpec<
                    ResponseBody = #success_ty,
                    Error = #failure_ty,
                >,
            {
            }

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

            /// Generated shared operation boundary. HTTP and RPC adapters must
            /// call this function; it executes the same policy hook before the
            /// authored semantic operation. The associated output/error types
            /// force the server implementation to stay aligned with the
            /// intermediary RPC contract used to generate clients.
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
                ::ores_api_docs::invoke_typed_context_operation(
                    &#descriptor_name,
                    #context_name,
                    #operation_name,
                )
                .await
            }
        });
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
    if context_spec.is_some() {
        result_types(&item.sig.output)?;
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

    if let Some(context_spec) = context_spec {
        let declared_spec = spec.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                item,
                "canonical ores_operation requires spec = GeneratedOperationSpec",
            )
        })?;
        if type_source(declared_spec) != type_source(&context_spec) {
            return Err(syn::Error::new_spanned(
                &first.ty,
                format!(
                    "ores_operation spec {} disagrees with TypedOperationContext operation type {}",
                    type_source(declared_spec),
                    type_source(&context_spec)
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
    if args.len() != 1 {
        return Err(syn::Error::new_spanned(
            item,
            "ores_route requires exactly operation = <local function>",
        ));
    }
    let Meta::NameValue(value) = &args[0] else {
        return Err(syn::Error::new_spanned(
            &args[0],
            "ores_route requires operation = <local function>",
        ));
    };
    if !value.path.is_ident("operation") {
        return Err(syn::Error::new_spanned(
            &value.path,
            "ores_route supports only operation = ...",
        ));
    }
    match &value.value {
        Expr::Path(expr) if !expr.path.segments.is_empty() => Ok(expr.path.to_token_stream().to_string()),
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(syn::Error::new_spanned(
            &value.value,
            "ores_route operation must be a function path such as handlers::find_user",
        )),
    }
}

fn result_types(output: &ReturnType) -> syn::Result<(Type, Type)> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "canonical ores_operation must return Result<Success, Error>",
        ));
    };
    let Type::Path(path) = ty.as_ref() else {
        return Err(syn::Error::new_spanned(
            ty,
            "canonical ores_operation must return Result<Success, Error>",
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "missing result type"));
    };
    if segment.ident != "Result" {
        return Err(syn::Error::new_spanned(
            ty,
            "canonical ores_operation must return Result<Success, Error>",
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
