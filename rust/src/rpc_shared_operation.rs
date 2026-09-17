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
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    rpc_v1_router, OperationContext, OperationInvokeError, OperationRequestData, OperationSpec,
    OptionalJson, RouteMap, RpcPayloadCodec, RpcV1Call, RpcV1Dispatcher, RpcV1HttpContext,
    RpcV1Receipt, TypedOperationContext,
};

pub type RpcV1SharedOperationFuture = Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>>;

pub type RpcV1SharedOperationHandler =
    Arc<dyn Fn(RpcV1HttpContext, RpcV1Call) -> RpcV1SharedOperationFuture + Send + Sync + 'static>;

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
    #[error("shared-operation RPC binding from {route_source} has an empty operation key")]
    EmptyOperation { route_source: &'static str },
    #[error("shared-operation RPC binding {operation:?} is duplicated")]
    DuplicateOperation { operation: &'static str },
    #[error(
        "shared-operation RPC binding {operation:?} from {route_source} is absent from the route map"
    )]
    UnknownOperation {
        operation: &'static str,
        route_source: &'static str,
    },
    #[error(
        "shared-operation RPC binding {operation:?} does not admit HTTP transport for /v1/rpc"
    )]
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
                    route_source: binding.source,
                });
            }
            if !expected.insert(binding.operation) {
                return Err(RpcV1SharedOperationRegistryError::DuplicateOperation {
                    operation: binding.operation,
                });
            }
            let Some(route) = crate::rpc_key_lookup::lookup_rpc_route(route_map, binding.operation)
            else {
                return Err(RpcV1SharedOperationRegistryError::UnknownOperation {
                    operation: binding.operation,
                    route_source: binding.source,
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
    fn dispatch(&self, context: RpcV1HttpContext, call: RpcV1Call) -> RpcV1SharedOperationFuture {
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

/// Canonical generated adapter for a context-centric operation.
///
/// `ores-stack rpc sync` emits only the stable type/operation binding and calls
/// this helper. Request section decoding, construction of `TypedOperationContext`,
/// invocation of the shared policy boundary, and receipt encoding therefore
/// remain identical for every generated `rpc.rs` file.
pub async fn dispatch_typed_json_operation<S, O, Invoke, Fut>(
    state: S,
    http_context: RpcV1HttpContext,
    call: RpcV1Call,
    invoke: Invoke,
) -> RpcV1Receipt
where
    O: OperationSpec,
    O::Path: DeserializeOwned,
    O::Query: DeserializeOwned,
    O::RequestHeaders: DeserializeOwned,
    O::RequestBody: DeserializeOwned,
    O::ResponseBody: Serialize,
    O::Error: Serialize,
    Invoke: FnOnce(TypedOperationContext<S, O>) -> Fut,
    Fut: Future<Output = Result<O::ResponseBody, OperationInvokeError<O::Error>>>,
{
    let path = match decode_section::<O::Path>(
        &call,
        "path",
        call.path.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(receipt) => return *receipt,
    };
    let query = match decode_section::<O::Query>(
        &call,
        "query",
        call.query.clone().map(Value::Object).unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(receipt) => return *receipt,
    };
    let headers = match decode_section::<O::RequestHeaders>(
        &call,
        "headers",
        call.headers
            .clone()
            .map(Value::Object)
            .unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(receipt) => return *receipt,
    };
    let body_value = call.body.value().cloned().unwrap_or(Value::Null);
    let body = match decode_section::<O::RequestBody>(&call, "body", body_value.clone()) {
        Ok(value) => value,
        Err(receipt) => return *receipt,
    };

    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    request.insert_path::<O>(path);
    request.insert_query::<O>(query);
    request.insert_headers::<O>(headers);
    request.insert_body::<O>(body);
    request.set_semantic_input(serde_json::json!({
        "path": &call.path,
        "query": &call.query,
        "headers": &call.headers,
        "body": body_value,
    }));

    let context =
        TypedOperationContext::<S, O>::new(OperationContext::rpc(state, http_context), request);

    match invoke(context).await {
        Ok(output) => match serde_json::to_value(output) {
            Ok(value) => {
                let mut receipt = RpcV1Receipt::success(
                    call.id.clone(),
                    call.key.clone(),
                    OptionalJson::present(value),
                );
                receipt.status = Some(200);
                receipt.trace_id = call.trace_id.clone();
                receipt.span_id = call.span_id.clone();
                receipt
            }
            Err(error) => failure_receipt(&call, 500, "response_encode_failed", error.to_string()),
        },
        Err(error) => {
            let value = serde_json::to_value(error).unwrap_or_else(|encode_error| {
                serde_json::json!({
                    "code": "operation_error_encode_failed",
                    "message": encode_error.to_string(),
                })
            });
            let mut object = value
                .as_object()
                .cloned()
                .unwrap_or_else(|| Map::from_iter([("detail".to_owned(), value)]));
            object
                .entry("code".to_owned())
                .or_insert_with(|| Value::String("operation_error".to_owned()));
            let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), 500, object);
            receipt.trace_id = call.trace_id.clone();
            receipt.span_id = call.span_id.clone();
            receipt
        }
    }
}

fn decode_section<T>(
    call: &RpcV1Call,
    section: &'static str,
    value: Value,
) -> Result<T, Box<RpcV1Receipt>>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|error| {
        Box::new(failure_receipt(
            call,
            400,
            "request_decode_failed",
            format!("{section}: {error}"),
        ))
    })
}

fn failure_receipt(call: &RpcV1Call, status: u16, code: &str, message: String) -> RpcV1Receipt {
    let error = Map::from_iter([
        ("code".to_owned(), Value::String(code.to_owned())),
        ("message".to_owned(), Value::String(message)),
    ]);
    let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), status, error);
    receipt.trace_id = call.trace_id.clone();
    receipt.span_id = call.span_id.clone();
    receipt
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
        let route = crate::rpc_key_lookup::lookup_rpc_route(&routes, "demo.users.find_user")
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
            Box::pin(
                async move { RpcV1Receipt::success(call.id, call.key, OptionalJson::absent()) },
            )
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
