#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, Attribute, ItemFn, Meta, Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationMarker {
    Rpc,
    NoRpc,
}

impl PublicationMarker {
    const fn name(self) -> &'static str {
        match self {
            Self::Rpc => "ores_rpc",
            Self::NoRpc => "ores_no_rpc",
        }
    }

    const fn conflicting_name(self) -> &'static str {
        match self {
            Self::Rpc => "ores_no_rpc",
            Self::NoRpc => "ores_rpc",
        }
    }
}

/// Explicitly publish a semantic operation through the RPC surface.
///
/// `ores-stack` requires this marker for route-less `#[ores_operation]`
/// functions. Operations living beside an authored `route.rs` are RPC-published
/// by default and therefore do not need this marker.
#[proc_macro_attribute]
pub fn ores_rpc(args: TokenStream, input: TokenStream) -> TokenStream {
    expand_marker(PublicationMarker::Rpc, args, input)
}

/// Explicitly suppress generated public RPC publication for a semantic
/// operation. The operation remains available to other admitted transports and
/// to the guarded semantic Lambda boundary.
#[proc_macro_attribute]
pub fn ores_no_rpc(args: TokenStream, input: TokenStream) -> TokenStream {
    expand_marker(PublicationMarker::NoRpc, args, input)
}

fn expand_marker(marker: PublicationMarker, args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let item = parse_macro_input!(input as ItemFn);
    match validate_marker(marker, &args, &item) {
        Ok(()) => quote!(#item).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn validate_marker(
    marker: PublicationMarker,
    args: &Punctuated<Meta, Token![,]>,
    item: &ItemFn,
) -> syn::Result<()> {
    if !args.is_empty() {
        return Err(syn::Error::new_spanned(
            args,
            format!("#[{}] does not accept arguments", marker.name()),
        ));
    }
    if item.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &item.sig.fn_token,
            format!("#[{}] requires an async semantic operation", marker.name()),
        ));
    }
    if has_attr(&item.attrs, marker.conflicting_name()) {
        return Err(syn::Error::new_spanned(
            &item.sig.ident,
            format!(
                "#[{}] and #[{}] are mutually exclusive",
                marker.name(),
                marker.conflicting_name()
            ),
        ));
    }
    Ok(())
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn accepts_explicit_rpc_marker_on_async_function() {
        let item: ItemFn = parse_quote! {
            async fn rebuild_index() -> Result<(), Error> { todo!() }
        };
        let args = Punctuated::<Meta, Token![,]>::new();
        validate_marker(PublicationMarker::Rpc, &args, &item).expect("valid RPC marker");
    }

    #[test]
    fn rejects_arguments() {
        let item: ItemFn = parse_quote! {
            async fn rebuild_index() -> Result<(), Error> { todo!() }
        };
        let args: Punctuated<Meta, Token![,]> =
            vec![parse_quote!(path = "/v1/rpc")].into_iter().collect();
        let error = validate_marker(PublicationMarker::Rpc, &args, &item)
            .expect_err("RPC endpoint is fixed and marker takes no metadata");
        assert!(error.to_string().contains("does not accept arguments"));
    }

    #[test]
    fn rejects_conflicting_markers() {
        let item: ItemFn = parse_quote! {
            #[ores_no_rpc]
            async fn rebuild_index() -> Result<(), Error> { todo!() }
        };
        let args = Punctuated::<Meta, Token![,]>::new();
        let error = validate_marker(PublicationMarker::Rpc, &args, &item)
            .expect_err("publication decision must be unique");
        assert!(error.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn rejects_non_async_functions() {
        let item: ItemFn = parse_quote! {
            fn rebuild_index() -> Result<(), Error> { todo!() }
        };
        let args = Punctuated::<Meta, Token![,]>::new();
        let error = validate_marker(PublicationMarker::Rpc, &args, &item)
            .expect_err("RPC operation must be async");
        assert!(error.to_string().contains("requires an async"));
    }
}
