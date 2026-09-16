#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprLit, ItemFn, Lit, Meta, MetaNameValue, Token, Visibility,
};

#[proc_macro_attribute]
pub fn ores_page(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_page(&args, &item) {
        Ok(()) => quote!(#item).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ores_generate(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    if !args.is_empty() {
        return syn::Error::new_spanned(args.first().expect("non-empty"), "ores_generate takes no arguments")
            .to_compile_error()
            .into();
    }
    if item.sig.ident != "generate_static_params" {
        return syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_generate] must annotate pub fn generate_static_params",
        )
        .to_compile_error()
        .into();
    }
    if !matches!(item.vis, Visibility::Public(_)) {
        return syn::Error::new_spanned(&item.vis, "generate_static_params must be pub")
            .to_compile_error()
            .into();
    }
    quote!(#item).into()
}

fn validate_page(args: &Punctuated<Meta, Token![,]>, item: &ItemFn) -> syn::Result<()> {
    if item.sig.ident != "page" {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_page] must annotate pub fn page",
        ));
    }
    if !matches!(item.vis, Visibility::Public(_)) {
        return Err(syn::Error::new_spanned(&item.vis, "page must be pub"));
    }

    let mut renderer = None;
    let mut delivery = None;
    let mut render = None;
    let mut revalidate_secs = None;
    let mut on_demand = None;
    let mut client = None;

    for meta in args {
        let Meta::NameValue(value) = meta else {
            return Err(syn::Error::new_spanned(meta, "ores_page arguments must be key = value"));
        };
        let key = value
            .path
            .get_ident()
            .map(ToString::to_string)
            .ok_or_else(|| syn::Error::new_spanned(&value.path, "ores_page keys must be identifiers"))?;
        match key.as_str() {
            "renderer" => renderer = Some(string_value(value, "renderer")?),
            "delivery" => delivery = Some(string_value(value, "delivery")?),
            "render" => render = Some(string_value(value, "render")?),
            "revalidate_secs" => revalidate_secs = Some(integer_value(value, "revalidate_secs")?),
            "on_demand" => on_demand = Some(string_value(value, "on_demand")?),
            "client" => client = Some(string_value(value, "client")?),
            _ => {
                return Err(syn::Error::new_spanned(
                    &value.path,
                    format!("unsupported ores_page key `{key}`"),
                ))
            }
        }
    }

    let renderer = renderer.ok_or_else(|| syn::Error::new_spanned(item, "ores_page requires renderer"))?;
    let delivery = delivery.ok_or_else(|| syn::Error::new_spanned(item, "ores_page requires delivery"))?;
    let render = render.unwrap_or_else(|| "dynamic".to_owned());

    if !matches!(renderer.as_str(), "mash" | "leptos" | "dioxus") {
        return Err(syn::Error::new_spanned(item, "renderer must be mash, leptos, or dioxus"));
    }
    if !matches!(delivery.as_str(), "ssr_only" | "client_only" | "ssr_hydrate") {
        return Err(syn::Error::new_spanned(
            item,
            "delivery must be ssr_only, client_only, or ssr_hydrate",
        ));
    }
    if !matches!(render.as_str(), "dynamic" | "static_only" | "static_with_fallback") {
        return Err(syn::Error::new_spanned(
            item,
            "render must be dynamic, static_only, or static_with_fallback",
        ));
    }
    if revalidate_secs.is_some() && on_demand.is_some() {
        return Err(syn::Error::new_spanned(
            item,
            "revalidate_secs and on_demand are mutually exclusive",
        ));
    }
    if delivery != "ssr_only" && client.is_none() {
        return Err(syn::Error::new_spanned(
            item,
            "client_only and ssr_hydrate pages must declare client = \"...\"",
        ));
    }
    if delivery == "ssr_only" && client.is_some() && renderer != "mash" {
        return Err(syn::Error::new_spanned(
            item,
            "non-MASH ssr_only pages should not declare a browser client entry",
        ));
    }
    Ok(())
}

fn string_value(value: &MetaNameValue, name: &str) -> syn::Result<String> {
    let Expr::Lit(ExprLit { lit: Lit::Str(value), .. }) = &value.value else {
        return Err(syn::Error::new_spanned(&value.value, format!("{name} must be a string literal")));
    };
    Ok(value.value())
}

fn integer_value(value: &MetaNameValue, name: &str) -> syn::Result<u64> {
    let Expr::Lit(ExprLit { lit: Lit::Int(value), .. }) = &value.value else {
        return Err(syn::Error::new_spanned(&value.value, format!("{name} must be an integer literal")));
    };
    value.base10_parse::<u64>()
}
