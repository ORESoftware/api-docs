#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    Expr, ExprLit, ItemFn, Lit, LitStr, Meta, MetaNameValue, Token, Visibility,
};

#[derive(Debug)]
struct ParsedPage {
    renderer: String,
    delivery: String,
    render: String,
    revalidate_secs: Option<u64>,
    on_demand: Option<String>,
    client: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    auth: String,
    stability: String,
    database: String,
    features: Vec<String>,
    data_sources: Vec<String>,
    tags: Vec<String>,
}

#[derive(Debug)]
struct ParsedRpc {
    key: String,
    codecs: Vec<String>,
    default_codec: String,
    audiences: Vec<String>,
    scope: String,
}

#[proc_macro_attribute]
pub fn ores_page(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_page(&args, &item) {
        Ok(meta) => {
            let renderer = match meta.renderer.as_str() {
                "mash" => quote!(::ores_api_docs_client::PageRenderer::Mash),
                "leptos" => quote!(::ores_api_docs_client::PageRenderer::Leptos),
                "dioxus" => quote!(::ores_api_docs_client::PageRenderer::Dioxus),
                _ => unreachable!("validated renderer"),
            };
            let delivery = match meta.delivery.as_str() {
                "ssr_only" => quote!(::ores_api_docs_client::PageDelivery::SsrOnly),
                "client_only" => quote!(::ores_api_docs_client::PageDelivery::ClientOnly),
                "ssr_hydrate" => quote!(::ores_api_docs_client::PageDelivery::SsrAndHydrate),
                _ => unreachable!("validated delivery"),
            };
            let render_mode = match meta.render.as_str() {
                "dynamic" => quote!(::ores_api_docs_client::PageRenderMode::Dynamic),
                "static_only" => quote!(::ores_api_docs_client::PageRenderMode::StaticOnly),
                "static_with_fallback" => {
                    quote!(::ores_api_docs_client::PageRenderMode::StaticWithFallback)
                }
                _ => unreachable!("validated render mode"),
            };
            let revalidate = if let Some(seconds) = meta.revalidate_secs {
                quote!(::ores_api_docs_client::RevalidationPolicy::AfterSeconds(#seconds))
            } else if let Some(tag) = meta.on_demand.as_deref() {
                let tag = LitStr::new(tag, Span::call_site());
                quote!(::ores_api_docs_client::RevalidationPolicy::OnDemand(#tag))
            } else {
                quote!(::ores_api_docs_client::RevalidationPolicy::Never)
            };

            let title = option_lit(meta.title.as_deref());
            let summary = option_lit(meta.summary.as_deref());
            let auth = LitStr::new(&meta.auth, Span::call_site());
            let stability = LitStr::new(&meta.stability, Span::call_site());
            let database = LitStr::new(&meta.database, Span::call_site());
            let features = string_lits(&meta.features);
            let data_sources = string_lits(&meta.data_sources);
            let tags = string_lits(&meta.tags);

            quote! {
                #item

                #[doc(hidden)]
                pub const __ORES_PAGE_CONFIG: ::ores_api_docs_client::PageConfig =
                    ::ores_api_docs_client::PageConfig {
                        renderer: #renderer,
                        delivery: #delivery,
                        render_mode: #render_mode,
                        revalidate: #revalidate,
                    };

                #[doc(hidden)]
                pub const __ORES_PAGE_METADATA: ::ores_api_docs_client::PageMetadata =
                    ::ores_api_docs_client::PageMetadata {
                        title: #title,
                        summary: #summary,
                        auth: #auth,
                        stability: #stability,
                        database: #database,
                        features: &[#(#features),*],
                        data_sources: &[#(#data_sources),*],
                        tags: &[#(#tags),*],
                    };

                #[doc(hidden)]
                pub fn __ores_page_boxed(
                    ctx: ::ores_api_docs_client::PageContext,
                ) -> ::ores_api_docs_client::PageFuture {
                    Box::pin(page(ctx))
                }
            }
            .into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ores_generate(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    if !args.is_empty() {
        return syn::Error::new_spanned(
            args.first().expect("non-empty"),
            "ores_generate takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    if item.sig.ident != "generate_static_params" {
        return syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_generate] must annotate pub async fn generate_static_params",
        )
        .to_compile_error()
        .into();
    }
    if !matches!(item.vis, Visibility::Public(_)) {
        return syn::Error::new_spanned(&item.vis, "generate_static_params must be pub")
            .to_compile_error()
            .into();
    }
    if item.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            &item.sig.fn_token,
            "generate_static_params must be async",
        )
        .to_compile_error()
        .into();
    }
    quote! {
        #item

        #[doc(hidden)]
        pub fn __ores_generate_static_params_boxed(
            ctx: ::ores_api_docs_client::PrerenderContext,
        ) -> ::ores_api_docs_client::GenerateStaticParamsFuture {
            Box::pin(generate_static_params(ctx))
        }
    }
    .into()
}

/// Compile-time metadata for an HTTP verb exported from a filesystem `route.rs`.
///
/// Path, method, extractor/body/result types are intentionally not repeated in
/// this attribute: `api-docs` derives them from the filesystem route, function
/// signature, and peer TypeSpec/JSON-Schema contract. The attribute carries only
/// semantic RPC metadata that cannot be inferred safely.
#[proc_macro_attribute]
pub fn ores_rpc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_rpc(&args, &item) {
        Ok(meta) => {
            // Force every parsed field to participate in compile-time validation
            // while leaving the handler ABI untouched for Axum.
            let _ = (
                meta.key,
                meta.codecs,
                meta.default_codec,
                meta.audiences,
                meta.scope,
            );
            quote!(#item).into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

fn validate_rpc(
    args: &Punctuated<Meta, Token![,]>,
    item: &ItemFn,
) -> syn::Result<ParsedRpc> {
    let name = item.sig.ident.to_string();
    if !matches!(
        name.as_str(),
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
    ) {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_rpc] must annotate a reserved route.rs HTTP verb export",
        ));
    }
    if !matches!(item.vis, Visibility::Public(_)) {
        return Err(syn::Error::new_spanned(
            &item.vis,
            "ores_rpc HTTP verb must be pub",
        ));
    }
    if item.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &item.sig.fn_token,
            "ores_rpc HTTP verb must be async",
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
                            "ores_rpc metadata keys must be identifiers",
                        )
                    })?;
                let parsed = string_value(value, &field)?;
                match field.as_str() {
                    "key" => rpc_set_once(&mut key, parsed, value, &field)?,
                    "default_codec" => {
                        rpc_set_once(&mut default_codec, parsed, value, &field)?
                    }
                    "scope" => rpc_set_once(&mut scope, parsed, value, &field)?,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &value.path,
                            format!("unsupported ores_rpc key `{field}`"),
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
                            "ores_rpc metadata lists must be identifiers",
                        )
                    })?;
                let values = list
                    .parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)?
                    .into_iter()
                    .map(|value| value.value())
                    .collect::<Vec<_>>();
                match field.as_str() {
                    "codecs" => rpc_set_once(&mut codecs, values, list, &field)?,
                    "audiences" => rpc_set_once(&mut audiences, values, list, &field)?,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &list.path,
                            format!("unsupported ores_rpc list `{field}`"),
                        ))
                    }
                }
            }
            Meta::Path(path) => {
                return Err(syn::Error::new_spanned(
                    path,
                    "bare ores_rpc flags are not supported",
                ));
            }
        }
    }

    let key = key.ok_or_else(|| syn::Error::new_spanned(item, "ores_rpc requires key"))?;
    if !valid_rpc_key(&key) {
        return Err(syn::Error::new_spanned(
            item,
            "ores_rpc key must be a stable dotted lowercase object key",
        ));
    }

    let codecs = codecs.unwrap_or_else(|| vec!["json".to_owned()]);
    validate_rpc_values(item, "codecs", &codecs, &["json", "protobuf", "messagepack"])?;
    let default_codec = default_codec.unwrap_or_else(|| codecs[0].clone());
    if !codecs.iter().any(|codec| codec == &default_codec) {
        return Err(syn::Error::new_spanned(
            item,
            "ores_rpc default_codec must also appear in codecs(...)"
        ));
    }

    let audiences = audiences.unwrap_or_else(|| vec!["server".to_owned()]);
    validate_rpc_values(item, "audiences", &audiences, &["browser", "server"])?;
    let scope = scope.unwrap_or_else(|| "regular".to_owned());
    if !matches!(scope.as_str(), "regular" | "admin") {
        return Err(syn::Error::new_spanned(
            item,
            "ores_rpc scope must be regular or admin",
        ));
    }
    if scope == "admin" && audiences.iter().any(|audience| audience == "browser") {
        return Err(syn::Error::new_spanned(
            item,
            "admin ores_rpc operations are server-only",
        ));
    }

    Ok(ParsedRpc {
        key,
        codecs,
        default_codec,
        audiences,
        scope,
    })
}

fn rpc_set_once<T>(
    slot: &mut Option<T>,
    value: T,
    span: impl quote::ToTokens,
    key: &str,
) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(
            span,
            format!("duplicate ores_rpc key `{key}`"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn validate_rpc_values(
    item: &ItemFn,
    name: &str,
    values: &[String],
    allowed: &[&str],
) -> syn::Result<()> {
    if values.is_empty() {
        return Err(syn::Error::new_spanned(
            item,
            format!("ores_rpc {name}(...) must not be empty"),
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !allowed.contains(&value.as_str()) {
            return Err(syn::Error::new_spanned(
                item,
                format!("unsupported ores_rpc {name} value {value:?}"),
            ));
        }
        if !seen.insert(value) {
            return Err(syn::Error::new_spanned(
                item,
                format!("duplicate ores_rpc {name} value {value:?}"),
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

fn validate_page(
    args: &Punctuated<Meta, Token![,]>,
    item: &ItemFn,
) -> syn::Result<ParsedPage> {
    if item.sig.ident != "page" {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            "#[ores_page] must annotate pub async fn page",
        ));
    }
    if !matches!(item.vis, Visibility::Public(_)) {
        return Err(syn::Error::new_spanned(&item.vis, "page must be pub"));
    }
    if item.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &item.sig.fn_token,
            "page must be async so SSR can use sibling APIs or read-only ORM calls",
        ));
    }

    let mut renderer = None;
    let mut delivery = None;
    let mut render = None;
    let mut revalidate_secs = None;
    let mut on_demand = None;
    let mut client = None;
    let mut title = None;
    let mut summary = None;
    let mut auth = None;
    let mut stability = None;
    let mut database = None;
    let mut features = None;
    let mut data_sources = None;
    let mut tags = None;

    for meta in args {
        match meta {
            Meta::NameValue(value) => {
                let key = value
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        syn::Error::new_spanned(&value.path, "ores_page keys must be identifiers")
                    })?;
                match key.as_str() {
                    "renderer" => set_once(&mut renderer, string_value(value, "renderer")?, value, &key)?,
                    "delivery" => set_once(&mut delivery, string_value(value, "delivery")?, value, &key)?,
                    "render" => set_once(&mut render, string_value(value, "render")?, value, &key)?,
                    "revalidate_secs" => set_once(
                        &mut revalidate_secs,
                        integer_value(value, "revalidate_secs")?,
                        value,
                        &key,
                    )?,
                    "on_demand" => set_once(&mut on_demand, string_value(value, "on_demand")?, value, &key)?,
                    "client" => set_once(&mut client, string_value(value, "client")?, value, &key)?,
                    "title" => set_once(&mut title, string_value(value, "title")?, value, &key)?,
                    "summary" => set_once(&mut summary, string_value(value, "summary")?, value, &key)?,
                    "auth" => set_once(&mut auth, string_value(value, "auth")?, value, &key)?,
                    "stability" => set_once(&mut stability, string_value(value, "stability")?, value, &key)?,
                    "database" => set_once(&mut database, string_value(value, "database")?, value, &key)?,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &value.path,
                            format!("unsupported ores_page key `{key}`"),
                        ))
                    }
                }
            }
            Meta::List(list) => {
                let key = list
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        syn::Error::new_spanned(&list.path, "ores_page list keys must be identifiers")
                    })?;
                let values = list
                    .parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)?
                    .into_iter()
                    .map(|value| value.value())
                    .collect::<Vec<_>>();
                match key.as_str() {
                    "features" => set_once(&mut features, values, list, &key)?,
                    "data_sources" => set_once(&mut data_sources, values, list, &key)?,
                    "tags" => set_once(&mut tags, values, list, &key)?,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            &list.path,
                            format!("unsupported ores_page list `{key}`"),
                        ))
                    }
                }
            }
            Meta::Path(path) => {
                return Err(syn::Error::new_spanned(
                    path,
                    "bare ores_page flags are not supported",
                ));
            }
        }
    }

    let renderer =
        renderer.ok_or_else(|| syn::Error::new_spanned(item, "ores_page requires renderer"))?;
    let delivery =
        delivery.ok_or_else(|| syn::Error::new_spanned(item, "ores_page requires delivery"))?;
    let render = render.unwrap_or_else(|| "dynamic".to_owned());
    let auth = auth.unwrap_or_else(|| "public".to_owned());
    let stability = stability.unwrap_or_else(|| "stable".to_owned());
    let database = database.unwrap_or_else(|| "none".to_owned());
    let features = features.unwrap_or_default();
    let data_sources = data_sources.unwrap_or_default();
    let tags = tags.unwrap_or_default();

    if !matches!(renderer.as_str(), "mash" | "leptos" | "dioxus") {
        return Err(syn::Error::new_spanned(
            item,
            "renderer must be mash, leptos, or dioxus",
        ));
    }
    if !matches!(delivery.as_str(), "ssr_only" | "client_only" | "ssr_hydrate") {
        return Err(syn::Error::new_spanned(
            item,
            "delivery must be ssr_only, client_only, or ssr_hydrate",
        ));
    }
    if !matches!(
        render.as_str(),
        "dynamic" | "static_only" | "static_with_fallback"
    ) {
        return Err(syn::Error::new_spanned(
            item,
            "render must be dynamic, static_only, or static_with_fallback",
        ));
    }
    if !matches!(auth.as_str(), "public" | "optional_session" | "session" | "admin") {
        return Err(syn::Error::new_spanned(
            item,
            "auth must be public, optional_session, session, or admin",
        ));
    }
    if !matches!(stability.as_str(), "experimental" | "beta" | "stable") {
        return Err(syn::Error::new_spanned(
            item,
            "stability must be experimental, beta, or stable",
        ));
    }
    if !matches!(database.as_str(), "none" | "read_only") {
        return Err(syn::Error::new_spanned(
            item,
            "database must be none or read_only",
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
    if title.as_ref().is_some_and(|value| value.trim().is_empty() || value.len() > 120) {
        return Err(syn::Error::new_spanned(item, "title must be 1..=120 bytes"));
    }
    if summary
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.len() > 500)
    {
        return Err(syn::Error::new_spanned(item, "summary must be 1..=500 bytes"));
    }

    validate_slugs(item, "features", &features)?;
    validate_slugs(item, "tags", &tags)?;
    validate_data_sources(item, &data_sources)?;
    if data_sources.iter().any(|value| value.starts_with("orm:")) && database != "read_only" {
        return Err(syn::Error::new_spanned(
            item,
            "orm: data_sources require database = \"read_only\" on a page renderer",
        ));
    }

    Ok(ParsedPage {
        renderer,
        delivery,
        render,
        revalidate_secs,
        on_demand,
        client,
        title,
        summary,
        auth,
        stability,
        database,
        features,
        data_sources,
        tags,
    })
}

fn set_once<T>(slot: &mut Option<T>, value: T, span: impl quote::ToTokens, key: &str) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(span, format!("duplicate ores_page key `{key}`")));
    }
    *slot = Some(value);
    Ok(())
}

fn validate_slugs(item: &ItemFn, name: &str, values: &[String]) -> syn::Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/'))
        {
            return Err(syn::Error::new_spanned(
                item,
                format!("{name} entries must be machine-readable slugs; invalid {value:?}"),
            ));
        }
        if !seen.insert(value) {
            return Err(syn::Error::new_spanned(
                item,
                format!("duplicate {name} entry {value:?}"),
            ));
        }
    }
    Ok(())
}

fn validate_data_sources(item: &ItemFn, values: &[String]) -> syn::Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        let Some((kind, target)) = value.split_once(':') else {
            return Err(syn::Error::new_spanned(
                item,
                format!("data source {value:?} needs rpc: or orm: prefix"),
            ));
        };
        if !matches!(kind, "rpc" | "orm") || target.is_empty() || target.chars().any(char::is_whitespace) {
            return Err(syn::Error::new_spanned(
                item,
                format!("data source {value:?} must be rpc:<operation> or orm:<read-surface>"),
            ));
        }
        if !seen.insert(value) {
            return Err(syn::Error::new_spanned(
                item,
                format!("duplicate data source {value:?}"),
            ));
        }
    }
    Ok(())
}

fn option_lit(value: Option<&str>) -> proc_macro2::TokenStream {
    value.map_or_else(
        || quote!(None),
        |value| {
            let value = LitStr::new(value, Span::call_site());
            quote!(Some(#value))
        },
    )
}

fn string_lits(values: &[String]) -> Vec<LitStr> {
    values
        .iter()
        .map(|value| LitStr::new(value, Span::call_site()))
        .collect()
}

fn string_value(value: &MetaNameValue, name: &str) -> syn::Result<String> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &value.value
    else {
        return Err(syn::Error::new_spanned(
            &value.value,
            format!("{name} must be a string literal"),
        ));
    };
    Ok(value.value())
}

fn integer_value(value: &MetaNameValue, name: &str) -> syn::Result<u64> {
    let Expr::Lit(ExprLit {
        lit: Lit::Int(value),
        ..
    }) = &value.value
    else {
        return Err(syn::Error::new_spanned(
            &value.value,
            format!("{name} must be an integer literal"),
        ));
    };
    value.base10_parse::<u64>()
}
