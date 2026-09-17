//! Handlers-authoritative shared-operation analysis.
//!
//! The original analyzer predates route-less RPC operations and therefore
//! requires every `#[ores_operation]` to have an `#[ores_route]` HTTP binding.
//! Keep its strict metadata/signature validation, but adapt that one migration
//! invariant here: handlers.rs owns the operation inventory and HTTP projection
//! is optional.

#[path = "shared_operation.rs"]
mod base;

pub use base::{
    HttpOperationAdapterSource, RpcExecutionModel, SharedOperationRouteSource,
    SharedOperationSource, SharedOperationSourceError,
};

use std::collections::BTreeSet;

use syn::{punctuated::Punctuated, Expr, ExprLit, Item, Lit, Meta, Token};

/// Analyze shared operations without requiring an HTTP projection for every
/// operation. Any authored `#[ores_route]` remains fully checked by the base
/// analyzer; only the obsolete unbound-operation requirement is neutralized.
pub fn analyze_shared_operation_route_source(
    path: &str,
    source: &str,
) -> Result<SharedOperationRouteSource, SharedOperationSourceError> {
    match base::analyze_shared_operation_route_source(path, source) {
        Ok(analysis) => return Ok(analysis),
        Err(SharedOperationSourceError::UnboundOperation { .. }) => {}
        Err(error) => return Err(error),
    }

    let file = syn::parse_file(source).map_err(|error| SharedOperationSourceError::Syntax {
        path: path.to_owned(),
        detail: error.to_string(),
    })?;
    let mut operations = BTreeSet::new();
    let mut bound = BTreeSet::new();
    let mut original_methods = BTreeSet::new();

    for item in &file.items {
        let Item::Fn(function) = item else {
            continue;
        };
        if has_attr(function, "ores_operation") {
            operations.insert(function.sig.ident.to_string());
        }
        if let Some(attr) = find_attr(function, "ores_route") {
            if let Some(operation) = route_operation(attr) {
                bound.insert(operation);
                original_methods.insert(function.sig.ident.to_string().to_ascii_uppercase());
            }
        }
    }

    let unbound = operations
        .difference(&bound)
        .cloned()
        .collect::<Vec<_>>();
    if unbound.is_empty() {
        return base::analyze_shared_operation_route_source(path, source);
    }

    // The base analyzer only needs evidence that each semantic operation is
    // bound. Duplicate Rust function names are valid syntax, and all synthetic
    // GET adapters are prepended so any authored GET adapter overwrites the
    // synthetic map entry later. We remove GET entirely afterward when the
    // original source had no authored GET projection.
    let synthetic = unbound
        .iter()
        .map(|operation| {
            format!(
                "#[ores_route(operation = {operation})]\npub async fn get() {{}}\n"
            )
        })
        .collect::<String>();
    let augmented = format!("{synthetic}\n{source}");
    let mut analysis = base::analyze_shared_operation_route_source(path, &augmented)?;

    analysis
        .adapters_by_method
        .retain(|method, _| original_methods.contains(method));
    Ok(analysis)
}

fn has_attr(function: &syn::ItemFn, name: &str) -> bool {
    find_attr(function, name).is_some()
}

fn find_attr<'a>(function: &'a syn::ItemFn, name: &str) -> Option<&'a syn::Attribute> {
    function.attrs.iter().find(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == name)
    })
}

fn route_operation(attr: &syn::Attribute) -> Option<String> {
    let args = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;
    if args.len() != 1 {
        return None;
    }
    let Meta::NameValue(value) = &args[0] else {
        return None;
    };
    if !value.path.is_ident("operation") {
        return None;
    }
    match &value.value {
        Expr::Path(expr) => expr
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Expr::Lit(ExprLit {
            lit: Lit::Str(value),
            ..
        }) => value
            .value()
            .rsplit("::")
            .next()
            .map(ToOwned::to_owned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_less_operation_is_valid_and_has_no_fake_http_adapter() {
        let source = r#"
            #[ores_operation(
                spec = RunJobOperation,
                key = "demo.jobs.run",
                codecs("json"),
                default_codec = "json",
                audiences("server"),
                scope = "regular"
            )]
            pub async fn run_job(
                ctx: TypedOperationContext<AppState, RunJobOperation>,
            ) -> Result<JobOutput, JobError> { todo!() }
        "#;
        let analysis = analyze_shared_operation_route_source("handlers.rs", source)
            .expect("route-less operation");
        assert!(analysis.operations.contains_key("run_job"));
        assert!(analysis.adapters_by_method.is_empty());
    }

    #[test]
    fn authored_http_projection_is_preserved() {
        let source = r#"
            #[ores_operation(
                spec = FindUserOperation,
                key = "demo.users.find",
                codecs("json"),
                default_codec = "json",
                audiences("browser", "server"),
                scope = "regular"
            )]
            pub async fn find_user(
                ctx: TypedOperationContext<AppState, FindUserOperation>,
            ) -> Result<User, UserError> { todo!() }

            #[ores_route(operation = find_user)]
            pub async fn get() -> HttpResult { todo!() }
        "#;
        let analysis = analyze_shared_operation_route_source("route.rs", source)
            .expect("HTTP projection");
        assert_eq!(
            analysis.operation_for_method("GET").map(|op| op.rust_name.as_str()),
            Some("find_user")
        );
    }
}
