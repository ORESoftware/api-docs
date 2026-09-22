#![forbid(unsafe_code)]

//! Compile-time validation for authored GraphQL resolver projections.
//!
//! GraphQL operation leaves live under `src/graphql/**/resolver.rs`.
//! A resolver binds by stable operation key to an existing semantic operation
//! owned by either `src/routes/rest/**/handlers.rs` or `src/rpc/**/funcs.rs`,
//! and names the exact generated `__ores_invoke_*` policy boundary it calls.
//! `ores-stack` performs the cross-file join and proves source, key, stream,
//! invoker and route-ingress identity agree.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Expr, ExprLit, FnArg, ItemFn, Lit, Meta, Pat, Token, Visibility, parse_macro_input,
    punctuated::Punctuated,
};

const GRAPHQL_V1_HTTP_PATH: &str = "/v1/graphql";

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphqlProjection {
    operation_key: String,
    invoke: String,
    kind: String,
    field: String,
    stream: String,
}

/// Marks one handwritten `src/graphql/**/resolver.rs` function as an explicit
/// GraphQL projection of an existing REST-associated or RPC-native semantic
/// operation.
///
/// ```ignore
/// #[ores_graphql(
///     operation_key = "users.get_user",
///     invoke = crate::routes::rest::users::get_user::handlers::__ores_invoke_get_user,
///     kind = "query",
///     field = "get_user",
///     stream = "unary"
/// )]
/// pub async fn resolve(...) -> ... { ... }
/// ```
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
                "#[ores_graphql] does not accept self receivers; keep resolver functions free-standing",
            ));
        };
        if !matches!(typed.pat.as_ref(), Pat::Ident(_)) {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "#[ores_graphql] parameters must use simple identifier patterns",
            ));
        }
    }

    let mut operation_key = None;
    let mut invoke = None;
    let mut kind = None;
    let mut field = None;
    let mut stream = None;
    for meta in args {
        let Meta::NameValue(value) = meta else {
            return Err(syn::Error::new_spanned(
                meta,
                "#[ores_graphql] arguments are operation_key = \"stable.key\", invoke = crate::...::__ores_invoke_fn, kind = \"query|mutation|subscription\", field = \"name\", stream = \"unary|server_stream\"",
            ));
        };
        if value.path.is_ident("operation_key") {
            set_once(
                &mut operation_key,
                string_value(&value.value, "operation_key")?,
                value,
                "operation_key",
            )?;
        } else if value.path.is_ident("invoke") {
            set_once(&mut invoke, invoke_value(&value.value)?, value, "invoke")?;
        } else if value.path.is_ident("kind") {
            set_once(
                &mut kind,
                string_value(&value.value, "kind")?,
                value,
                "kind",
            )?;
        } else if value.path.is_ident("field") {
            set_once(
                &mut field,
                string_value(&value.value, "field")?,
                value,
                "field",
            )?;
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
                "unsupported #[ores_graphql] key; HTTP ingress is fixed at src/routes/graphql/v1/route.rs -> /v1/graphql and REST paths are not GraphQL schema authority",
            ));
        }
    }

    let operation_key = operation_key.ok_or_else(|| {
        syn::Error::new_spanned(
            item,
            "#[ores_graphql] requires operation_key = \"stable.operation.key\"",
        )
    })?;
    if !valid_operation_key(&operation_key) {
        return Err(syn::Error::new_spanned(
            item,
            "#[ores_graphql] operation_key must be a stable dotted lowercase object key",
        ));
    }
    let invoke = invoke.ok_or_else(|| {
        syn::Error::new_spanned(
            item,
            "#[ores_graphql] requires invoke = crate::...::__ores_invoke_<operation>",
        )
    })?;
    let kind = kind
        .ok_or_else(|| syn::Error::new_spanned(item, "#[ores_graphql] requires kind"))?;
    if !matches!(kind.as_str(), "query" | "mutation" | "subscription") {
        return Err(syn::Error::new_spanned(
            item,
            "#[ores_graphql] kind must be query, mutation, or subscription",
        ));
    }
    let field = field
        .ok_or_else(|| syn::Error::new_spanned(item, "#[ores_graphql] requires field"))?;
    validate_graphql_name(&field).map_err(|message| syn::Error::new_spanned(item, message))?;
    let stream = stream.ok_or_else(|| {
        syn::Error::new_spanned(
            item,
            "#[ores_graphql] requires explicit stream metadata; ores-stack cross-checks it against the semantic operation",
        )
    })?;
    match (kind.as_str(), stream.as_str()) {
        ("query" | "mutation", "unary") | ("subscription", "server_stream") => {}
        ("subscription", _) => {
            return Err(syn::Error::new_spanned(
                item,
                "GraphQL subscriptions require stream = \"server_stream\"",
            ));
        }
        (_, _) => {
            return Err(syn::Error::new_spanned(
                item,
                "GraphQL queries and mutations require stream = \"unary\"",
            ));
        }
    }

    let _ = GRAPHQL_V1_HTTP_PATH;
    Ok(GraphqlProjection {
        operation_key,
        invoke,
        kind,
        field,
        stream,
    })
}

fn invoke_value(expr: &Expr) -> syn::Result<String> {
    let Expr::Path(value) = expr else {
        return Err(syn::Error::new_spanned(
            expr,
            "#[ores_graphql] invoke must be a Rust path such as crate::routes::rest::users::handlers::__ores_invoke_get_user or crate::rpc::users::funcs::__ores_invoke_get_user",
        ));
    };
    let Some(terminal) = value.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            expr,
            "#[ores_graphql] invoke must name a generated __ores_invoke_<operation> function",
        ));
    };
    let terminal = terminal.ident.to_string();
    if !terminal.starts_with("__ores_invoke_") || terminal == "__ores_invoke_" {
        return Err(syn::Error::new_spanned(
            expr,
            "#[ores_graphql] invoke must name a generated __ores_invoke_<operation> function",
        ));
    }
    Ok(value.path.to_token_stream().to_string().replace(' ', ""))
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

fn valid_operation_key(key: &str) -> bool {
    let parts = key.split('.').collect::<Vec<_>>();
    parts.len() >= 2
        && parts.into_iter().all(|part| {
            let mut chars = part.chars();
            matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
                && chars.all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-'
                })
        })
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
            parse_quote!(operation_key = "users.get_user"),
            parse_quote!(invoke = crate::routes::rest::users::handlers::__ores_invoke_get_user),
            parse_quote!(kind = "query"),
            parse_quote!(field = "get_user"),
            parse_quote!(stream = "unary"),
        ];
        let args: Punctuated<Meta, Token![,]> = items.into_iter().collect();
        let item: ItemFn = parse_quote!(pub async fn resolver(ctx: Context) -> Result<(), Error> { todo!() });
        let parsed = validate(&args, &item).expect("valid authored projection");
        assert_eq!(parsed.operation_key, "users.get_user");
        assert!(parsed.invoke.ends_with("__ores_invoke_get_user"));
    }

    #[test]
    fn accepts_deep_qualified_invoker_path() {
        let items: Vec<Meta> = vec![
            parse_quote!(operation_key = "ores_data_platform.capabilities.get"),
            parse_quote!(
                invoke = crate::routes::rest::v1::capabilities::handlers::__ores_invoke_capabilities
            ),
            parse_quote!(kind = "query"),
            parse_quote!(field = "capabilities"),
            parse_quote!(stream = "unary"),
        ];
        let args: Punctuated<Meta, Token![,]> = items.into_iter().collect();
        let item: ItemFn = parse_quote!(pub async fn capabilities(ctx: Context) -> Result<(), Error> { todo!() });
        let parsed = validate(&args, &item).expect("deep generated invoker path");
        assert_eq!(
            parsed.invoke,
            "crate::routes::rest::v1::capabilities::handlers::__ores_invoke_capabilities"
        );
    }

    #[test]
    fn rejects_non_generated_invoker_path() {
        let items: Vec<Meta> = vec![
            parse_quote!(operation_key = "users.get_user"),
            parse_quote!(invoke = crate::routes::rest::users::handlers::get_user),
            parse_quote!(kind = "query"),
            parse_quote!(field = "get_user"),
            parse_quote!(stream = "unary"),
        ];
        let args: Punctuated<Meta, Token![,]> = items.into_iter().collect();
        let item: ItemFn = parse_quote!(pub async fn resolver(ctx: Context) -> Result<(), Error> { todo!() });
        assert!(
            validate(&args, &item)
                .unwrap_err()
                .to_string()
                .contains("__ores_invoke_")
        );
    }

    #[test]
    fn subscription_requires_server_stream() {
        let items: Vec<Meta> = vec![
            parse_quote!(operation_key = "events.watch_stream"),
            parse_quote!(invoke = crate::routes::rest::events::handlers::__ores_invoke_watch_stream),
            parse_quote!(kind = "subscription"),
            parse_quote!(field = "watch_events"),
            parse_quote!(stream = "unary"),
        ];
        let args: Punctuated<Meta, Token![,]> = items.into_iter().collect();
        let item: ItemFn = parse_quote!(pub async fn resolver(ctx: Context) -> Result<(), Error> { todo!() });
        assert!(
            validate(&args, &item)
                .unwrap_err()
                .to_string()
                .contains("server_stream")
        );
    }

    #[test]
    fn operation_key_and_graphql_name_are_strict() {
        assert!(valid_operation_key("users.get_user"));
        assert!(!valid_operation_key("Users.GetUser"));
        assert!(validate_graphql_name("__schema").is_err());
        assert!(validate_graphql_name("2bad").is_err());
        assert!(validate_graphql_name("good_name2").is_ok());
    }
}
