#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprLit, ItemFn, Lit, LitStr, Meta, MetaNameValue, Token, Visibility,
};

#[derive(Debug)]
struct ParsedOperation {
    key: String,
    codecs: Vec<String>,
    default_codec: String,
    audiences: Vec<String>,
    scope: String,
}

#[proc_macro_attribute]
pub fn ores_operation(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_operation(&args, &item) {
        Ok(meta) => {
            let _ = (
                meta.key,
                meta.codecs,
                meta.default_codec,
                meta.audiences,
                meta.scope,
            );
            quote::quote!(#item).into()
        }
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
            quote::quote!(#item).into()
        }
        Err(error) => error.to_compile_error().into(),
    }
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
                    .ok_or_else(|| {
                        syn::Error::new_spanned(
                            &value.path,
                            "ores_operation metadata keys must be identifiers",
                        )
                    })?;
                let parsed = string_value(value, &field)?;
                match field.as_str() {
                    "key" => set_once(&mut key, parsed, value, &field)?,
                    "default_codec" => set_once(&mut default_codec, parsed, value, &field)?,
                    "scope" => set_once(&mut scope, parsed, value, &field)?,
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

    let key = key.ok_or_else(|| syn::Error::new_spanned(item, "ores_operation requires key"))?;
    if !valid_rpc_key(&key) {
        return Err(syn::Error::new_spanned(
            item,
            "ores_operation key must be a stable dotted lowercase object key",
        ));
    }
    let codecs = codecs.unwrap_or_else(|| vec!["json".to_owned()]);
    validate_values(item, "codecs", &codecs, &["json", "protobuf", "messagepack"])?;
    let default_codec = default_codec.unwrap_or_else(|| codecs[0].clone());
    if !codecs.iter().any(|codec| codec == &default_codec) {
        return Err(syn::Error::new_spanned(
            item,
            "ores_operation default_codec must also appear in codecs(...)"
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

    Ok(ParsedOperation {
        key,
        codecs,
        default_codec,
        audiences,
        scope,
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
        Expr::Path(expr) if expr.path.segments.len() == 1 => {
            Ok(expr.path.segments[0].ident.to_string())
        }
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(syn::Error::new_spanned(
            &value.value,
            "ores_route operation must be one local function identifier",
        )),
    }
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
