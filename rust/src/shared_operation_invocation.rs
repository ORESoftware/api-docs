//! Verify that an HTTP adapter declared with `#[ores_route(operation = ...)]`
//! actually calls the generated `__ores_invoke_<operation>` boundary.
//!
//! Attribute pairing alone is not enough: an adapter could declare the right
//! operation and still execute unrelated business logic. This verifier walks
//! the authored Rust AST and fails closed if the adapter bypasses the shared
//! invoker or directly calls the inner operation.

use std::collections::BTreeMap;

use syn::{visit::Visit, Expr, ExprCall, Item};

use crate::SharedOperationRouteSource;

pub fn verify_shared_operation_invocations(
    path: &str,
    source: &str,
    analysis: &SharedOperationRouteSource,
) -> Result<(), String> {
    let file = syn::parse_file(source)
        .map_err(|error| format!("Rust syntax error in {path}: {error}"))?;
    let mut functions = BTreeMap::new();
    for item in &file.items {
        if let Item::Fn(function) = item {
            functions.insert(function.sig.ident.to_string(), function);
        }
    }

    for adapter in analysis.adapters_by_method.values() {
        let function = functions.get(&adapter.rust_name).ok_or_else(|| {
            format!("{path}: HTTP adapter {:?} disappeared during invocation verification", adapter.rust_name)
        })?;
        let operation = analysis.operations.get(&adapter.operation).ok_or_else(|| {
            format!("{path}: missing shared operation {:?}", adapter.operation)
        })?;
        let mut visitor = InvocationVisitor {
            operation: operation.rust_name.as_str(),
            invoker: operation.invoke_name.as_str(),
            invoker_calls: 0,
            direct_operation_calls: 0,
        };
        visitor.visit_block(&function.block);

        if visitor.direct_operation_calls != 0 {
            return Err(format!(
                "{path}: HTTP adapter {} must not call shared operation {} directly; call {} so HTTP and RPC share operation policy",
                adapter.rust_name, operation.rust_name, operation.invoke_name
            ));
        }
        if visitor.invoker_calls != 1 {
            return Err(format!(
                "{path}: HTTP adapter {} must call {} exactly once, observed {} calls",
                adapter.rust_name, operation.invoke_name, visitor.invoker_calls
            ));
        }
    }
    Ok(())
}

struct InvocationVisitor<'a> {
    operation: &'a str,
    invoker: &'a str,
    invoker_calls: usize,
    direct_operation_calls: usize,
}

impl<'ast> Visit<'ast> for InvocationVisitor<'_> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some(name) = called_function_name(node.func.as_ref()) {
            if name == self.invoker {
                self.invoker_calls += 1;
            }
            if name == self.operation {
                self.direct_operation_calls += 1;
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn called_function_name(expr: &Expr) -> Option<&str> {
    let Expr::Path(path) = expr else {
        return None;
    };
    path.path.segments.last().map(|segment| segment.ident.to_string()).and_then(|name| {
        // The visitor only needs this value for the duration of the comparison,
        // but returning an owned String would complicate the generic visitor.
        // Instead use the identifier token text through a leaked tiny string;
        // this path is build-time analysis only and bounded by source call sites.
        Some(Box::leak(name.into_boxed_str()) as &str)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze_shared_operation_route_source;

    fn analyze(source: &str) -> SharedOperationRouteSource {
        analyze_shared_operation_route_source("src/routes/users/route.rs", source)
            .expect("shared operation analysis")
    }

    #[test]
    fn adapter_must_call_generated_invoker_exactly_once() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.find_user")]
            async fn find_user(ctx: OperationContext, input: FindUserInput) -> Output { todo!() }

            #[ores_route(operation = find_user)]
            pub async fn get() -> HttpResult {
                __ores_invoke_find_user(ctx, input).await.into()
            }
        "#;
        verify_shared_operation_invocations(
            "src/routes/users/route.rs",
            source,
            &analyze(source),
        )
        .expect("invoker use");
    }

    #[test]
    fn direct_inner_operation_call_is_rejected() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.find_user")]
            async fn find_user(ctx: OperationContext, input: FindUserInput) -> Output { todo!() }

            #[ores_route(operation = find_user)]
            pub async fn get() -> HttpResult {
                find_user(ctx, input).await.into()
            }
        "#;
        let error = verify_shared_operation_invocations(
            "src/routes/users/route.rs",
            source,
            &analyze(source),
        )
        .expect_err("direct call must fail");
        assert!(error.contains("must not call"));
    }

    #[test]
    fn declaration_without_invoker_call_is_rejected() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.find_user")]
            async fn find_user(ctx: OperationContext, input: FindUserInput) -> Output { todo!() }

            #[ores_route(operation = find_user)]
            pub async fn get() -> HttpResult {
                unrelated().await
            }
        "#;
        let error = verify_shared_operation_invocations(
            "src/routes/users/route.rs",
            source,
            &analyze(source),
        )
        .expect_err("missing invoker must fail");
        assert!(error.contains("exactly once"));
    }
}
