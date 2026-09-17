//! Direct `/v1/rpc` dispatch to generated shared-operation adapters.
//!
//! Unlike the legacy filesystem projector, this runtime never constructs an
//! `http::Request` and never re-enters an Axum REST router. Each generated
//! handler decodes one `RpcV1Call` into the operation input, calls the same
//! `__ores_invoke_*` function used by the HTTP adapter, and encodes the typed
//! output into an `RpcV1Receipt`.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use axum::Router;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    rpc_v1_router, OptionalJson, RouteMap, RpcV1Call, RpcV1Dispatcher, RpcV1HttpContext,
    RpcV1Receipt,
};

pub type RpcV1SharedOperationFuture =
    Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>>;

pub type RpcV1SharedOperationHandler = Arc<
    dyn Fn(RpcV1HttpContext, RpcV1Call) -> RpcV1SharedOperationFuture + Send + Sync + 'static,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpcV1SharedOperationBinding {
    /// Stable wire identity, normally the route entry's dotted `rpc_key`.
    pub operation: &'static str,
    pub source: &'static str,
    pub operation_fn: &'static str,
    pub invoker_fn: &'static str,
}

impl RpcV1SharedOperationBinding {
    #[must_use]
    pub const fn new(
        operation: &'static str,
        source: &'static str,
        operation_fn: &'static str,
        invoker_fn: &'static str,
    ) -> Self {
        Self {
            operation,
            source,
            operation_fn,
            invoker_fn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RpcV1SharedOperationRegistryError {
    #[error("shared-operation RPC binding from {source} has an empty operation key")]
    EmptyOperation { source: &'static str },
    #[error("shared-operation RPC binding {operation:?} is duplicated")]
    DuplicateOperation { operation: &'static str },
    #[error("shared-operation RPC binding {operation:?} from {source} is absent from the route map")]
    UnknownOperation {
        operation: &'static str,
        source: &'static str,
    },
    #[error("shared-operation RPC binding {operation:?} does not admit HTTP transport for /v1/rpc")]
    HttpTransportNotAllowed { operation: &'static str },
    #[error("shared-operation RPC binding {operation:?} has invalid operation/invoker identity")]
    InvalidInvokerIdentity { operation: &'static str },
    #[error("shared-operation RPC handler missing for {operation:?}")]
    MissingHandler { operation: &'static str },
    #[error("shared-operation RPC handler exists without a binding for {operation:?}")]
    UnexpectedHandler { operation: String },
}

#[derive(Clone)]
pub struct RpcV1SharedOperationRegistry {
    handlers: Arc<BTreeMap<String, RpcV1SharedOperationHandler>>,
}

impl RpcV1SharedOperationRegistry {
    pub fn new(
        route_map: &RouteMap,
        bindings: &'static [RpcV1SharedOperationBinding],
        handlers: BTreeMap<String, RpcV1SharedOperationHandler>,
    ) -> Result<Self, RpcV1SharedOperationRegistryError> {
        let mut expected = BTreeSet::new();
        for binding in bindings {
            if binding.operation.trim().is_empty() {
                return Err(RpcV1SharedOperationRegistryError::EmptyOperation {
                    source: binding.source,
                });
            }
            if !expected.insert(binding.operation) {
                return Err(RpcV1SharedOperationRegistryError::DuplicateOperation {
                    operation: binding.operation,
                });
            }
            let Some(route) = route_map.lookup_rpc(binding.operation) else {
                return Err(RpcV1SharedOperationRegistryError::UnknownOperation {
                    operation: binding.operation,
                    source: binding.source,
                });
            };
            if !route.transports.iter().any(|transport| transport == "http") {
                return Err(RpcV1SharedOperationRegistryError::HttpTransportNotAllowed {
                    operation: binding.operation,
                });
            }
            let expected_invoker = format!("__ores_invoke_{}", binding.operation_fn);
            if binding.operation_fn.trim().is_empty() || binding.invoker_fn != expected_invoker {
                return Err(RpcV1SharedOperationRegistryError::InvalidInvokerIdentity {
                    operation: binding.operation,
                });
            }
            if !handlers.contains_key(binding.operation) {
                return Err(RpcV1SharedOperationRegistryError::MissingHandler {
                    operation: binding.operation,
                });
            }
        }
        for operation in handlers.keys() {
            if !expected.contains(operation.as_str()) {
                return Err(RpcV1SharedOperationRegistryError::UnexpectedHandler {
                    operation: operation.clone(),
                });
            }
        }
        Ok(Self {
            handlers: Arc::new(handlers),
        })
    }
}

impl RpcV1Dispatcher for RpcV1SharedOperationRegistry {
    fn dispatch(
        &self,
        context: RpcV1HttpContext,
        call: RpcV1Call,
    ) -> RpcV1SharedOperationFuture {
        let handler = self.handlers.get(&call.key).cloned();
        Box::pin(async move {
            let Some(handler) = handler else {
                return missing_handler_receipt(call);
            };
            handler(context, call).await
        })
    }
}

pub fn shared_operation_rpc_v1_router(
    route_map: RouteMap,
    bindings: &'static [RpcV1SharedOperationBinding],
    handlers: BTreeMap<String, RpcV1SharedOperationHandler>,
) -> Result<Router, RpcV1SharedOperationRegistryError> {
    let registry = RpcV1SharedOperationRegistry::new(&route_map, bindings, handlers)?;
    Ok(rpc_v1_router(route_map, registry))
}

fn missing_handler_receipt(call: RpcV1Call) -> RpcV1Receipt {
    let mut error = Map::new();
    error.insert(
        "code".into(),
        Value::String("rpc_operation_not_in_build".into()),
    );
    error.insert(
        "message".into(),
        Value::String("RPC operation is not included in this build slice".into()),
    );
    let mut receipt = RpcV1Receipt::failure(call.id, call.key, 501, error);
    receipt.trace_id = call.trace_id;
    receipt.span_id = call.span_id;
    receipt.body = OptionalJson::absent();
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routes() -> RouteMap {
        RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo",
              "map":{
                "find_user_by_id":{
                  "path":"/v1/users/{id}",
                  "methods":["GET"],
                  "rpc_key":"demo.users.find_user",
                  "transports":["http"]
                }
              }
            }"#,
        )
        .expect("route map")
    }

    static BINDINGS: &[RpcV1SharedOperationBinding] = &[RpcV1SharedOperationBinding::new(
        "demo.users.find_user",
        "src/routes/v1/users/[id]/route.rs",
        "find_user",
        "__ores_invoke_find_user",
    )];

    #[test]
    fn stable_rpc_key_can_differ_from_legacy_map_key() {
        let routes = routes();
        let route = routes
            .lookup_rpc("demo.users.find_user")
            .expect("rpc key lookup");
        assert_eq!(route.path, "/v1/users/{id}");
    }

    #[test]
    fn registry_requires_exact_handler_inventory() {
        let error = RpcV1SharedOperationRegistry::new(&routes(), BINDINGS, BTreeMap::new())
            .err()
            .expect("missing handler must fail");
        assert!(matches!(
            error,
            RpcV1SharedOperationRegistryError::MissingHandler { .. }
        ));
    }

    #[test]
    fn invoker_name_is_bound_to_operation_function() {
        static BAD: &[RpcV1SharedOperationBinding] = &[RpcV1SharedOperationBinding::new(
            "demo.users.find_user",
            "src/routes/v1/users/[id]/route.rs",
            "find_user",
            "__ores_invoke_something_else",
        )];
        let mut handlers = BTreeMap::new();
        let handler: RpcV1SharedOperationHandler = Arc::new(|_, call| {
            Box::pin(async move {
                RpcV1Receipt::success(call.id, call.key, OptionalJson::absent())
            })
        });
        handlers.insert("demo.users.find_user".to_owned(), handler);
        let error = RpcV1SharedOperationRegistry::new(&routes(), BAD, handlers)
            .err()
            .expect("bad invoker must fail");
        assert!(matches!(
            error,
            RpcV1SharedOperationRegistryError::InvalidInvokerIdentity { .. }
        ));
    }
}
