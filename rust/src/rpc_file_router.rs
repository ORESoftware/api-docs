//! Executable filesystem-route registry for the RPC v1 Axum transport.
//!
//! `src/routes/**/route.rs` remains subordinate to the reviewed RouteMap, but a
//! Lambda-capable route file can now export an actual server handler instead of
//! only metadata. The same static binding table can be mounted in one monolith
//! or narrowed to a single operation for a small function build.

use std::{collections::BTreeSet, future::Future, pin::Pin};

use axum::Router;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    rpc_v1_router, RouteMap, RpcV1Call, RpcV1Dispatcher, RpcV1HttpContext, RpcV1Receipt,
};

pub type RpcV1RouteFuture = Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>>;
pub type RpcV1RouteHandler = fn(RpcV1HttpContext, RpcV1Call) -> RpcV1RouteFuture;

#[derive(Clone, Copy)]
pub struct RpcV1RouteBinding {
    pub operation: &'static str,
    pub source: &'static str,
    pub handler: RpcV1RouteHandler,
}

impl RpcV1RouteBinding {
    #[must_use]
    pub const fn new(
        operation: &'static str,
        source: &'static str,
        handler: RpcV1RouteHandler,
    ) -> Self {
        Self {
            operation,
            source,
            handler,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RpcV1RouteRegistryError {
    #[error("filesystem RPC binding from {source} has an empty operation key")]
    EmptyOperation { source: &'static str },
    #[error("filesystem RPC operation {operation:?} is bound more than once")]
    DuplicateOperation { operation: &'static str },
    #[error("filesystem RPC operation {operation:?} from {source} is absent from the route map")]
    UnknownOperation {
        operation: &'static str,
        source: &'static str,
    },
    #[error("filesystem RPC operation {operation:?} from {source} does not admit HTTP transport")]
    HttpTransportNotAllowed {
        operation: &'static str,
        source: &'static str,
    },
}

/// Clone-cheap dispatcher backed by compile-generated static function pointers.
///
/// The registry intentionally permits a strict subset of the supplied RouteMap:
/// that is how the exact same route files support a full standalone server and
/// a one-route Lambda/function binary. Calls to map entries that are not present
/// in this registry fail closed at dispatch time.
#[derive(Clone, Copy)]
pub struct RpcV1RouteRegistry {
    bindings: &'static [RpcV1RouteBinding],
}

impl RpcV1RouteRegistry {
    pub fn new(
        route_map: &RouteMap,
        bindings: &'static [RpcV1RouteBinding],
    ) -> Result<Self, RpcV1RouteRegistryError> {
        let mut seen = BTreeSet::new();
        for binding in bindings {
            if binding.operation.trim().is_empty() {
                return Err(RpcV1RouteRegistryError::EmptyOperation {
                    source: binding.source,
                });
            }
            if !seen.insert(binding.operation) {
                return Err(RpcV1RouteRegistryError::DuplicateOperation {
                    operation: binding.operation,
                });
            }
            let Some(route) = route_map.lookup(binding.operation) else {
                return Err(RpcV1RouteRegistryError::UnknownOperation {
                    operation: binding.operation,
                    source: binding.source,
                });
            };
            if !route.transports.iter().any(|transport| transport == "http") {
                return Err(RpcV1RouteRegistryError::HttpTransportNotAllowed {
                    operation: binding.operation,
                    source: binding.source,
                });
            }
        }
        Ok(Self { bindings })
    }

    #[must_use]
    pub fn bindings(&self) -> &'static [RpcV1RouteBinding] {
        self.bindings
    }
}

impl RpcV1Dispatcher for RpcV1RouteRegistry {
    fn dispatch(
        &self,
        context: RpcV1HttpContext,
        call: RpcV1Call,
    ) -> RpcV1RouteFuture {
        if let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| binding.operation == call.key)
        {
            return (binding.handler)(context, call);
        }

        Box::pin(async move {
            let mut error = Map::new();
            error.insert(
                "code".into(),
                Value::String("rpc_route_not_in_build".to_owned()),
            );
            error.insert(
                "message".into(),
                Value::String(
                    "RPC operation is valid for the service but is not included in this build slice"
                        .to_owned(),
                ),
            );
            RpcV1Receipt::failure(call.id, call.key, 501, error)
        })
    }
}

/// Mount executable filesystem RPC handlers at `/rpc/v1`.
///
/// `bindings` may contain every operation for a monolith or one/few operations
/// for a Lambda/function build. RouteMap validation remains authoritative in
/// both cases.
pub fn filesystem_rpc_v1_router(
    route_map: RouteMap,
    bindings: &'static [RpcV1RouteBinding],
) -> Result<Router, RpcV1RouteRegistryError> {
    let dispatcher = RpcV1RouteRegistry::new(&route_map, bindings)?;
    Ok(rpc_v1_router(route_map, dispatcher))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OptionalJson;

    fn health(_context: RpcV1HttpContext, call: RpcV1Call) -> RpcV1RouteFuture {
        Box::pin(async move {
            RpcV1Receipt::success(
                call.id,
                call.key,
                OptionalJson::present(Value::String("ok".into())),
            )
        })
    }

    #[test]
    fn validates_static_bindings_against_route_map() {
        static BINDINGS: &[RpcV1RouteBinding] = &[RpcV1RouteBinding::new(
            "healthz",
            "src/routes/healthz/route.rs",
            health,
        )];
        let map =
            RouteMap::from_json_str(include_str!("../../examples/canonical-api.route-map.json"))
                .expect("canonical route map");
        let registry = RpcV1RouteRegistry::new(&map, BINDINGS).expect("registry");
        assert_eq!(registry.bindings()[0].operation, "healthz");
    }

    #[test]
    fn rejects_unknown_operation_before_mount() {
        static BINDINGS: &[RpcV1RouteBinding] = &[RpcV1RouteBinding::new(
            "missing-operation",
            "src/routes/missing/route.rs",
            health,
        )];
        let map =
            RouteMap::from_json_str(include_str!("../../examples/canonical-api.route-map.json"))
                .expect("canonical route map");
        let error = RpcV1RouteRegistry::new(&map, BINDINGS).expect_err("unknown key must fail");
        assert!(matches!(
            error,
            RpcV1RouteRegistryError::UnknownOperation { .. }
        ));
    }
}
