#![forbid(unsafe_code)]

//! Compile-time validation for authored GraphQL resolver projections.
//!
//! This macro deliberately does not infer GraphQL from HTTP paths or generate a
//! resolver. `graphql.rs` remains authored code. The attribute supplies the
//! small, deterministic projection identity that `ores-stack` can cross-check
//! against the sibling `handlers.rs` semantic authority.

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprLit, FnArg, ItemFn, Lit, Meta, Pat, Token, Visibility,
};

const GRAPHQL_V1_HTTP_PATH: &str = "/v1/graphql";

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphqlProjection {
    operation: String,
    kind: String,
    field: String,
    stream: String,
}

/// Marks one handwritten `graphql.rs` resolver as an explicit projection of an
/// authored semantic operation.
///
/// Canonical form:
///
/// ```ignore
/// #[ores_graphql(
///     operation = handlers::get_user,
///     kind = "query",
///     field = "get_user",
///     stream = "unary"
/// )]
/// pub async fn get_user_graphql(...) -> ... { ... }
/// ```
///
/// Subscriptions must declare `stream = "server_stream"`; queries and mutations
/// must declare `stream = "unary"`. `ores-stack` performs the cross-file check
/// that this declaration agrees with `#[ores_operation]` in `handlers.rs`.
#[proc_macro_attribute]
pub fn ores_graphql(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate(&args, &item) {
        Ok(_) => quote!(#item).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn validate(
    args: &Punctuated<Meta, Token![,]>,
    item: &ItemFn,
) -> syn::Result<GraphqlProjection> {
    if item.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &item.sig.fn_token,
            "#[ores_graphql] requires an async resolver function",
        ));
    }
    if matches!(&item.vis, Visibility::Inherited) {
        return Err(syn::Error::new_spanned(
            &item.vis,
            "#[ores_graphql] resolver must be pub or pub(crate) so generated schema glue can reference it",
        ));
    }
    for input in &item.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "#[ores_graphql] does not accept self receivers; keep the authored resolver as a free function and let schema glue call it",
            ));
        };
        if !matches!(typed.pat.as_ref(), Pat::Ident(_)) {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "#[ores_graphql] parameters must use simple identifier patterns",
            ));
        }
    }

    let mut operation = None;
    let mut kind = None;
    let mut field = None;
    let mut stream = None;
    for meta in args {
        let Meta::NameValue(value) = meta else {
            return Err(syn::Error::new_spanned(
                meta,
                "#[ores_graphql] arguments are operation = handlers::fn, kind = \"query|mutation|subscription\", field = \"name\", stream = \"unary|server_stream\"",
            ));
        };
        if value.path.is_ident("operation") {
            set_once(
                &mut operation,
                operation_value(&value.value)?,
                value,
                "operation",
            )?;
        } else if value.path.is_ident("kind") {
            set_once(&mut kind, string_value(&value.value, "kind")?, value, "kind")?;
        } else if value.path.is_ident("field") {
            set_once(&mut field, string_value(&value.value, "field")?, value, "field")?;
        } else if value.path.is_ident("stream") {
            set_once(
                &mut stream,
                string_value(&value.value, "stream")?,
                value,
                "stream",
            )?;
        } else {
            return Err(syn::Error::new_spanned(
                &value.path,
                "unsupported #[ores_graphql] key; endpoint is fixed at /v1/graphql and may not be overridden",
            ));
        }
    }

    let operation = operation.ok_or_else(|| {
        syn::Error::new_spanned(item, "#[ores_graphql] requires operation = handlers::<operation>")
    })?;
    if !operation.starts_with("handlers :: ") && !operation.starts_with("handlers::") {
        return Err(syn::Error::new_spanned(
            item,
            "#[ores_graphql] operation must reference the sibling handlers module (for example handlers::get_user)",
        ));
    }
    let kind = kind.ok_or_else(|| syn::Error::new_spanned(item, "#[ores_graphql] requires kind"))?;
    if !matches!(kind.as_str(), "query" | "mutation" | "subscription") {
        return Err(syn::Error::new_spanned(
            item,
            "#[ores_graphql] kind must be query, mutation, or subscription",
        ));
    }
    let field = field.ok_or_else(|| syn::Error::new_spanned(item, "#[ores_graphql] requires field"))?;
    validate_graphql_name(&field).map_err(|message| syn::Error::new_spanned(item, message))?;
    let stream = stream.ok_or_else(|| {
        syn::Error::new_spanned(
            item,
            "#[ores_graphql] requires explicit stream metadata; it is cross-checked against handlers.rs",
        )
    })?;
    match (kind.as_str(), stream.as_str()) {
        ("query" | "mutation", "unary") | ("subscription", "server_stream") => {}
        ("subscription", _) => {
            return Err(syn::Error::new_spanned(
                item,
                "GraphQL subscriptions require stream = \"server_stream\"",
            ))
        }
        (_, _) => {
            return Err(syn::Error::new_spanned(
                item,
                "GraphQL queries and mutations require stream = \"unary\"",
            ))
        }
    }

    let _ = GRAPHQL_V1_HTTP_PATH;

    Ok(GraphqlProjection {
        operation,
        kind,
        field,
        stream,
    })
}

fn operation_value(expr: &Expr) -> syn::Result<String> {
    match expr {
        Expr::Path(value) if !value.path.segments.is_empty() => {
            Ok(value.path.to_token_stream().to_string())
        }
        _ => Err(syn::Error::new_spanned(
            expr,
            "#[ores_graphql] operation must be a Rust path such as handlers::get_user",
        )),
    }
}

fn string_value(expr: &Expr, field: &str) -> syn::Result<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(syn::Error::new_spanned(
            expr,
            format!("#[ores_graphql] {field} must be a string literal"),
        )),
    }
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
            format!("#[ores_graphql] {field} is given twice"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn validate_graphql_name(value: &str) -> Result<(), String> {
    if value.starts_with("__") {
        return Err("GraphQL fields beginning with `__` are reserved for introspection".to_owned());
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("GraphQL field must not be empty".to_owned());
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(format!(
            "GraphQL field {value:?} must start with ASCII letter or underscore"
        ));
    }
    if chars.any(|character| !(character == '_' || character.is_ascii_alphanumeric())) {
        return Err(format!(
            "GraphQL field {value:?} contains a character outside [_0-9A-Za-z]"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn accepts_explicit_unary_query() {
        let items: Vec<Meta> = vec![
            parse_quote!(operation = handlers::get_user),
            parse_quote!(kind = "query"),
            parse_quote!(field = "get_user"),
            parse_quote!(stream = "unary"),
        ];
        let args: Punctuated<Meta, Token![,]> = items.into_iter().collect();
        let item: ItemFn = parse_quote!(pub async fn resolver(ctx: Context) -> Result<(), Error> { todo!() });
        let parsed = validate(&args, &item).expect("valid authored projection");
        assert_eq!(parsed.kind, "query");
        assert_eq!(parsed.field, "get_user");
    }

    #[test]
    fn subscription_requires_server_stream() {
        let items: Vec<Meta> = vec![
            parse_quote!(operation = handlers::watch_events_stream),
            parse_quote!(kind = "subscription"),
            parse_quote!(field = "watch_events"),
            parse_quote!(stream = "unary"),
        ];
        let args: Punctuated<Meta, Token![,]> = items.into_iter().collect();
        let item: ItemFn = parse_quote!(pub async fn resolver(ctx: Context) -> Result<(), Error> { todo!() });
        assert!(validate(&args, &item).unwrap_err().to_string().contains("server_stream"));
    }

    #[test]
    fn rejects_reserved_or_invalid_names() {
        assert!(validate_graphql_name("__schema").is_err());
        assert!(validate_graphql_name("2bad").is_err());
        assert!(validate_graphql_name("bad-name").is_err());
        assert!(validate_graphql_name("good_name2").is_ok());
    }
}
