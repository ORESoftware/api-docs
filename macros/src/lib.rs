#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::Parser, parse_macro_input, punctuated::Punctuated, Expr, ExprLit, ItemFn, Lit, Meta,
    Token,
};

/// Declare statically discoverable page behavior without executing the module.
///
/// Example:
/// `#[ores_page(renderer = "leptos", delivery = "ssr_hydrate", render = "static_fallback", revalidate_seconds = 300)]`
#[proc_macro_attribute]
pub fn ores_page(args: TokenStream, item: TokenStream) -> TokenStream {
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let args = match parser.parse(args) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let function = parse_macro_input!(item as ItemFn);
    if function.sig.ident != "page" {
        return syn::Error::new_spanned(
            &function.sig.ident,
            "#[ores_page] is reserved for the exported `page` function",
        )
        .into_compile_error()
        .into();
    }

    let mut renderer = None::<String>;
    let mut delivery = None::<String>;
    let mut render = None::<String>;
    let mut revalidate = None::<String>;
    let mut revalidate_seconds = None::<u64>;
    let mut revalidate_tag = None::<String>;

    for meta in args {
        let Meta::NameValue(value) = meta else {
            return syn::Error::new_spanned(meta, "ores_page arguments must be name = value")
                .into_compile_error()
                .into();
        };
        let Some(name) = value.path.get_ident().map(ToString::to_string) else {
            return syn::Error::new_spanned(value.path, "ores_page argument names must be identifiers")
                .into_compile_error()
                .into();
        };
        match name.as_str() {
            "renderer" => match string_literal(&value.value) {
                Ok(v) => renderer = Some(v),
                Err(e) => return e.into_compile_error().into(),
            },
            "delivery" => match string_literal(&value.value) {
                Ok(v) => delivery = Some(v),
                Err(e) => return e.into_compile_error().into(),
            },
            "render" => match string_literal(&value.value) {
                Ok(v) => render = Some(v),
                Err(e) => return e.into_compile_error().into(),
            },
            "revalidate" => match string_literal(&value.value) {
                Ok(v) => revalidate = Some(v),
                Err(e) => return e.into_compile_error().into(),
            },
            "revalidate_seconds" => match int_literal(&value.value) {
                Ok(v) => revalidate_seconds = Some(v),
                Err(e) => return e.into_compile_error().into(),
            },
            "revalidate_tag" => match string_literal(&value.value) {
                Ok(v) => revalidate_tag = Some(v),
                Err(e) => return e.into_compile_error().into(),
            },
            _ => {
                return syn::Error::new_spanned(
                    value.path,
                    format!("unknown #[ores_page] argument `{name}`"),
                )
                .into_compile_error()
                .into();
            }
        }
    }

    let renderer = match renderer.as_deref() {
        Some("mash") => quote!(::ores_api_docs_client::PageRenderer::Mash),
        Some("leptos") => quote!(::ores_api_docs_client::PageRenderer::Leptos),
        Some("dioxus") => quote!(::ores_api_docs_client::PageRenderer::Dioxus),
        Some(other) => return invalid_value("renderer", other, "mash|leptos|dioxus"),
        None => return missing("renderer"),
    };
    let delivery = match delivery.as_deref() {
        Some("ssr_only") => quote!(::ores_api_docs_client::PageDelivery::SsrOnly),
        Some("client_only") => quote!(::ores_api_docs_client::PageDelivery::ClientOnly),
        Some("ssr_hydrate") => quote!(::ores_api_docs_client::PageDelivery::SsrAndHydrate),
        Some(other) => return invalid_value("delivery", other, "ssr_only|client_only|ssr_hydrate"),
        None => return missing("delivery"),
    };
    let render = match render.as_deref().unwrap_or("dynamic") {
        "dynamic" => quote!(::ores_api_docs_client::PageRenderMode::Dynamic),
        "static_only" => quote!(::ores_api_docs_client::PageRenderMode::StaticOnly),
        "static_fallback" => quote!(::ores_api_docs_client::PageRenderMode::StaticWithFallback),
        other => return invalid_value("render", other, "dynamic|static_only|static_fallback"),
    };

    let revalidation_fields = usize::from(revalidate.is_some())
        + usize::from(revalidate_seconds.is_some())
        + usize::from(revalidate_tag.is_some());
    if revalidation_fields > 1 {
        return syn::Error::new_spanned(
            &function.sig.ident,
            "use only one of revalidate, revalidate_seconds, or revalidate_tag",
        )
        .into_compile_error()
        .into();
    }
    let revalidate = if let Some(seconds) = revalidate_seconds {
        quote!(::ores_api_docs_client::RevalidationPolicy::AfterSeconds(#seconds))
    } else if let Some(tag) = revalidate_tag {
        quote!(::ores_api_docs_client::RevalidationPolicy::OnDemand(#tag))
    } else {
        match revalidate.as_deref().unwrap_or("never") {
            "never" => quote!(::ores_api_docs_client::RevalidationPolicy::Never),
            other => return invalid_value("revalidate", other, "never"),
        }
    };

    quote! {
        #function

        #[doc(hidden)]
        pub const __ORES_PAGE_SPEC: ::ores_api_docs_client::PageSpec =
            ::ores_api_docs_client::PageSpec {
                renderer: #renderer,
                delivery: #delivery,
                render_mode: #render,
                revalidate: #revalidate,
            };
    }
    .into()
}

fn string_literal(expr: &Expr) -> Result<String, syn::Error> {
    match expr {
        Expr::Lit(ExprLit { lit: Lit::Str(value), .. }) => Ok(value.value()),
        _ => Err(syn::Error::new_spanned(expr, "expected a string literal")),
    }
}

fn int_literal(expr: &Expr) -> Result<u64, syn::Error> {
    match expr {
        Expr::Lit(ExprLit { lit: Lit::Int(value), .. }) => value.base10_parse::<u64>(),
        _ => Err(syn::Error::new_spanned(expr, "expected an integer literal")),
    }
}

fn missing(name: &str) -> TokenStream {
    syn::Error::new(proc_macro2::Span::call_site(), format!("missing #[ores_page] argument `{name}`"))
        .into_compile_error()
        .into()
}

fn invalid_value(name: &str, value: &str, allowed: &str) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        format!("invalid #[ores_page] {name}={value:?}; expected {allowed}"),
    )
    .into_compile_error()
    .into()
}
