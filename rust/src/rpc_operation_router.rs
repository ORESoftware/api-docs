//! Hardened direct RPC dispatch for filesystem API routes.
//!
//! `src/routes/**/route.rs` remains the implementation authority. Generated
//! code registers each exported HTTP verb function with Axum for ordinary REST
//! traffic and also stores a direct callable reference to that exact handler for
//! RPC. RPC operation selection happens before Axum extraction and never passes
//! through the API server's general-purpose router.
//!
//! This separation is intentional:
//! - browser `page.rs` routes are never eligible RPC targets;
//! - `/v1/rpc` and the legacy `/rpc/v1` alias are reserved transport endpoints;
//! - an RPC operation cannot fall through into another REST route;
//! - an RPC operation cannot recurse into the RPC transport endpoint;
//! - generated code can apply one handler-level middleware layer to the exact
//!   same handler value used by REST and RPC, preserving auth/rate-limit/trace
//!   semantics without using the HTTP router as the RPC dispatcher.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use axum::{
    body::Body,
    handler::Handler,
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request},
    response::Response,
    Router,
};
use http_body_util::BodyExt;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    is_runtime_owned_request_header, rpc_v1_router, OptionalJson, RouteMap, RpcV1Call,
    RpcV1Dispatcher, RpcV1HttpContext, RpcV1Receipt, RpcV1RouteBinding, Transport,
};

pub type RpcV1OperationFuture = Pin<Box<dyn Future<Output = Response> + Send + 'static>>;
pub type RpcV1OperationHandler =
    Arc<dyn Fn(Request<Body>) -> RpcV1OperationFuture + Send + Sync + 'static>;

const RPC_V1_CANONICAL_PATH: &str = "/v1/rpc";
const RPC_V1_LEGACY_PATH: &str = "/rpc/v1";

/// Erase one concrete Axum handler into the small callable shape used by the RPC
/// registry. This invokes `Handler::call` directly; there is no `Router` lookup,
/// loopback socket, or internal HTTP route traversal.
pub fn axum_rpc_operation_handler<H, T, S>(handler: H, state: S) -> RpcV1OperationHandler
where
    H: Handler<T, S>,
    T: 'static,
    S: Clone + Send + Sync + 'static,
{
    Arc::new(move |request| {
        let future = handler.clone().call(request, state.clone());
        Box::pin(future)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RpcV1OperationRegistryError {
    #[error("filesystem RPC binding from {route_source} has an empty operation key")]
    EmptyOperation { route_source: &'static str },
    #[error("filesystem RPC operation {operation:?} is bound more than once")]
    DuplicateOperation { operation: &'static str },
    #[error(
        "filesystem RPC operation {operation:?} from {route_source} is absent from the route map"
    )]
    UnknownOperation {
        operation: &'static str,
        route_source: &'static str,
    },
    #[error(
        "filesystem RPC operation {operation:?} from {route_source} does not admit HTTP transport"
    )]
    HttpTransportNotAllowed {
        operation: &'static str,
        route_source: &'static str,
    },
    #[error(
        "filesystem binding {operation:?} from {route_source} declares {method} {path}, but api-docs declares methods {actual_methods:?} at {actual_path}"
    )]
    ContractMismatch {
        operation: &'static str,
        route_source: &'static str,
        method: &'static str,
        path: &'static str,
        actual_methods: Vec<String>,
        actual_path: String,
    },
    #[error(
        "filesystem RPC operation {operation:?} from {route_source} targets reserved RPC transport path {path}"
    )]
    RecursiveRpcTarget {
        operation: &'static str,
        route_source: &'static str,
        path: &'static str,
    },
    #[error("filesystem RPC operation {operation:?} has no generated direct handler")]
    MissingOperationHandler { operation: &'static str },
    #[error("generated direct RPC handler {operation:?} has no matching filesystem binding")]
    UnexpectedOperationHandler { operation: &'static str },
}

/// Registry of exact operation-key -> direct Axum handler references.
#[derive(Clone)]
pub struct RpcV1OperationRegistry {
    routes: Arc<RouteMap>,
    bindings: &'static [RpcV1RouteBinding],
    handlers: Arc<BTreeMap<&'static str, RpcV1OperationHandler>>,
}

impl RpcV1OperationRegistry {
    pub fn new(
        route_map: RouteMap,
        bindings: &'static [RpcV1RouteBinding],
        handlers: BTreeMap<&'static str, RpcV1OperationHandler>,
    ) -> Result<Self, RpcV1OperationRegistryError> {
        validate_bindings(&route_map, bindings, &handlers)?;
        Ok(Self {
            routes: Arc::new(route_map),
            bindings,
            handlers: Arc::new(handlers),
        })
    }

    #[must_use]
    pub fn bindings(&self) -> &'static [RpcV1RouteBinding] {
        self.bindings
    }

    #[must_use]
    pub fn route_map(&self) -> &RouteMap {
        self.routes.as_ref()
    }

    /// Dispatch one decoded RPC call directly to the exact `route.rs` handler.
    /// The synthetic request exists only so the normal Axum extractors can parse
    /// path/query/header/body values; no Axum router performs operation selection.
    pub async fn dispatch_call(
        &self,
        call: RpcV1Call,
        trusted_ingress_headers: HeaderMap,
        transport: Transport,
    ) -> RpcV1Receipt {
        let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| binding.operation == call.key)
        else {
            return failure(
                call,
                501,
                "rpc_route_not_in_build",
                "RPC operation is valid for the service but is not included in this build slice",
                transport,
            );
        };

        let Some(route) = self.routes.lookup(&call.key) else {
            return failure(
                call,
                500,
                "rpc_route_registry_drift",
                "filesystem RPC binding no longer exists in the route map",
                transport,
            );
        };
        if !route
            .transports
            .iter()
            .any(|allowed| allowed == transport.as_str())
        {
            return failure(
                call,
                400,
                "transport_not_allowed",
                "operation does not declare the selected RPC transport",
                transport,
            );
        }
        if call.transport.is_some_and(|declared| declared != transport) {
            return failure(
                call,
                400,
                "transport_mismatch",
                "RPC envelope transport does not match the active adapter",
                transport,
            );
        }

        let Some(handler) = self.handlers.get(binding.operation) else {
            return failure(
                call,
                500,
                "rpc_operation_handler_missing",
                "generated direct RPC handler is missing",
                transport,
            );
        };

        let request = match request_from_call(binding, &call, &trusted_ingress_headers) {
            Ok(request) => request,
            Err(message) => {
                return failure(call, 400, "rpc_http_projection_failed", &message, transport)
            }
        };

        let response = handler(request).await;
        receipt_from_response(call, response, transport).await
    }
}

impl RpcV1Dispatcher for RpcV1OperationRegistry {
    fn dispatch(&self, context: RpcV1HttpContext, call: RpcV1Call) -> RpcV1OperationFuture {
        let dispatcher = self.clone();
        let ingress = context.request_headers().clone();
        Box::pin(async move {
            let receipt = dispatcher
                .dispatch_call(call, ingress, Transport::Http)
                .await;
            let status = receipt.status.unwrap_or(if receipt.ok { 200 } else { 500 });
            let body = receipt.encode().unwrap_or_default();
            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("valid direct RPC response")
        })
    }
}

/// Adapter required by `rpc_v1_router`. This is deliberately separate from the
/// application operation future because `RpcV1Dispatcher` returns receipts.
#[derive(Clone)]
struct ReceiptDispatcher(RpcV1OperationRegistry);

impl RpcV1Dispatcher for ReceiptDispatcher {
    fn dispatch(
        &self,
        context: RpcV1HttpContext,
        call: RpcV1Call,
    ) -> Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>> {
        let dispatcher = self.0.clone();
        let ingress = context.request_headers().clone();
        Box::pin(async move {
            dispatcher
                .dispatch_call(call, ingress, Transport::Http)
                .await
        })
    }
}

/// Build the HTTP RPC transport from exact direct operation handlers. The
/// returned router contains only the RPC transport endpoint.
pub fn filesystem_operation_rpc_v1_router(
    route_map: RouteMap,
    bindings: &'static [RpcV1RouteBinding],
    handlers: BTreeMap<&'static str, RpcV1OperationHandler>,
) -> Result<Router, RpcV1OperationRegistryError> {
    let dispatcher = RpcV1OperationRegistry::new(route_map.clone(), bindings, handlers)?;
    Ok(rpc_v1_router(route_map, ReceiptDispatcher(dispatcher)))
}

fn validate_bindings(
    route_map: &RouteMap,
    bindings: &'static [RpcV1RouteBinding],
    handlers: &BTreeMap<&'static str, RpcV1OperationHandler>,
) -> Result<(), RpcV1OperationRegistryError> {
    let mut seen = BTreeSet::new();
    for binding in bindings {
        if binding.operation.trim().is_empty() {
            return Err(RpcV1OperationRegistryError::EmptyOperation {
                route_source: binding.source,
            });
        }
        if !seen.insert(binding.operation) {
            return Err(RpcV1OperationRegistryError::DuplicateOperation {
                operation: binding.operation,
            });
        }
        if is_reserved_rpc_path(binding.path) {
            return Err(RpcV1OperationRegistryError::RecursiveRpcTarget {
                operation: binding.operation,
                route_source: binding.source,
                path: binding.path,
            });
        }
        let Some(route) = route_map.lookup(binding.operation) else {
            return Err(RpcV1OperationRegistryError::UnknownOperation {
                operation: binding.operation,
                route_source: binding.source,
            });
        };
        if !route.transports.iter().any(|transport| transport == "http") {
            return Err(RpcV1OperationRegistryError::HttpTransportNotAllowed {
                operation: binding.operation,
                route_source: binding.source,
            });
        }
        if route.path != binding.path
            || route.methods.len() != 1
            || !route.methods[0].eq_ignore_ascii_case(binding.method)
        {
            return Err(RpcV1OperationRegistryError::ContractMismatch {
                operation: binding.operation,
                route_source: binding.source,
                method: binding.method,
                path: binding.path,
                actual_methods: route.methods.clone(),
                actual_path: route.path.clone(),
            });
        }
        if !handlers.contains_key(binding.operation) {
            return Err(RpcV1OperationRegistryError::MissingOperationHandler {
                operation: binding.operation,
            });
        }
    }

    for operation in handlers.keys().copied() {
        if !seen.contains(operation) {
            return Err(RpcV1OperationRegistryError::UnexpectedOperationHandler { operation });
        }
    }
    Ok(())
}

fn is_reserved_rpc_path(path: &str) -> bool {
    matches!(path, RPC_V1_CANONICAL_PATH | RPC_V1_LEGACY_PATH)
}

fn request_from_call(
    binding: &RpcV1RouteBinding,
    call: &RpcV1Call,
    trusted_ingress_headers: &HeaderMap,
) -> Result<Request<Body>, String> {
    let path = expand_binding_path(binding.path, call.path.as_ref())?;
    let query = encode_query(call.query.as_ref())?;
    let uri = if query.is_empty() {
        path
    } else {
        format!("{path}?{query}")
    };
    let method = Method::from_bytes(binding.method.as_bytes())
        .map_err(|error| format!("invalid generated HTTP method: {error}"))?;
    let body = if let Some(value) = call.body.value() {
        serde_json::to_vec(value).map_err(|error| format!("encode RPC body as JSON: {error}"))?
    } else {
        Vec::new()
    };

    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::from(body))
        .map_err(|error| format!("build direct RPC extractor request: {error}"))?;
    copy_trusted_ingress_headers(trusted_ingress_headers, request.headers_mut());
    copy_application_headers(call.headers.as_ref(), request.headers_mut())?;
    if call.body.is_present() && !request.headers().contains_key(header::CONTENT_TYPE) {
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    Ok(request)
}

fn expand_binding_path(
    template: &str,
    params: Option<&Map<String, Value>>,
) -> Result<String, String> {
    let empty = Map::new();
    let params = params.unwrap_or(&empty);
    let mut used = BTreeSet::new();
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'{' {
            out.push(bytes[index] as char);
            index += 1;
            continue;
        }
        let close = template[index + 1..]
            .find('}')
            .map(|relative| index + 1 + relative)
            .ok_or_else(|| format!("unclosed path placeholder in {template:?}"))?;
        let mut name = &template[index + 1..close];
        let catch_all = name.starts_with('*');
        if catch_all {
            name = &name[1..];
        }
        let optional = name.ends_with('?');
        if optional {
            name = &name[..name.len() - 1];
        }
        let value = params.get(name);
        if value.is_none() && optional {
            if out.ends_with('/') {
                out.pop();
            }
            index = close + 1;
            continue;
        }
        let value = value.ok_or_else(|| format!("missing RPC path parameter {name:?}"))?;
        let scalar = scalar_string(value)
            .ok_or_else(|| format!("RPC path parameter {name:?} must be scalar"))?;
        used.insert(name.to_owned());
        if catch_all {
            out.push_str(
                &scalar
                    .split('/')
                    .map(percent_encode)
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        } else {
            out.push_str(&percent_encode(&scalar));
        }
        index = close + 1;
    }

    let extras = params
        .keys()
        .filter(|key| !used.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    if !extras.is_empty() {
        return Err(format!("unexpected RPC path parameters: {extras:?}"));
    }
    Ok(out)
}

fn encode_query(query: Option<&Map<String, Value>>) -> Result<String, String> {
    let Some(query) = query else {
        return Ok(String::new());
    };
    let mut keys = query.keys().collect::<Vec<_>>();
    keys.sort();
    let mut parts = Vec::new();
    for key in keys {
        match &query[key] {
            Value::Array(values) => {
                for item in values {
                    let scalar = scalar_string(item).ok_or_else(|| {
                        format!("RPC query array {key:?} may contain only scalar values")
                    })?;
                    parts.push(format!(
                        "{}={}",
                        percent_encode(key),
                        percent_encode(&scalar)
                    ));
                }
            }
            Value::Null => {}
            value => {
                let scalar = scalar_string(value)
                    .ok_or_else(|| format!("RPC query value {key:?} must be scalar"))?;
                parts.push(format!(
                    "{}={}",
                    percent_encode(key),
                    percent_encode(&scalar)
                ));
            }
        }
    }
    Ok(parts.join("&"))
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn copy_application_headers(
    headers: Option<&Map<String, Value>>,
    target: &mut HeaderMap,
) -> Result<(), String> {
    let Some(headers) = headers else {
        return Ok(());
    };
    for (name, value) in headers {
        if is_runtime_owned_request_header(name) {
            return Err(format!(
                "RPC application headers may not set runtime-owned header {name:?}"
            ));
        }
        let value = scalar_string(value)
            .ok_or_else(|| format!("RPC header {name:?} must be a scalar value"))?;
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| format!("invalid RPC header name: {error}"))?;
        let value = HeaderValue::from_str(&value)
            .map_err(|error| format!("invalid RPC header value: {error}"))?;
        target.insert(name, value);
    }
    Ok(())
}

fn copy_trusted_ingress_headers(source: &HeaderMap, target: &mut HeaderMap) {
    for name in [
        "cf-connecting-ip",
        "x-real-ip",
        "x-request-id",
        "traceparent",
        "tracestate",
    ] {
        if let Some(value) = source.get(name) {
            if let Ok(name) = HeaderName::from_bytes(name.as_bytes()) {
                target.insert(name, value.clone());
            }
        }
    }
}

async fn receipt_from_response(
    call: RpcV1Call,
    response: Response,
    transport: Transport,
) -> RpcV1Receipt {
    let status = response.status();
    let bytes = match response.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            return failure(
                call,
                502,
                "rpc_http_body_read_failed",
                &error.to_string(),
                transport,
            )
        }
    };

    if status.is_success() || status.is_redirection() {
        let body = if bytes.is_empty() {
            OptionalJson::absent()
        } else {
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) => OptionalJson::present(value),
                Err(_) => {
                    return failure(
                        call,
                        502,
                        "rpc_http_non_json_success",
                        "RPC-capable HTTP handlers must return JSON or an empty body",
                        transport,
                    )
                }
            }
        };
        let mut receipt = RpcV1Receipt::success(call.id, call.key, body);
        receipt.status = Some(status.as_u16());
        receipt.transport = Some(transport);
        receipt.trace_id = call.trace_id;
        receipt.span_id = call.span_id;
        return receipt;
    }

    let mut error = match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(object)) => object,
        Ok(value) => {
            let mut object = Map::new();
            object.insert("code".into(), Value::String("http_error".into()));
            object.insert("body".into(), value);
            object
        }
        Err(_) => {
            let mut object = Map::new();
            object.insert("code".into(), Value::String("http_error".into()));
            object.insert(
                "message".into(),
                Value::String("HTTP handler returned a non-JSON error body".into()),
            );
            object
        }
    };
    error
        .entry("status")
        .or_insert(Value::from(status.as_u16()));
    let mut receipt = RpcV1Receipt::failure(call.id, call.key, status.as_u16(), error);
    receipt.transport = Some(transport);
    receipt.trace_id = call.trace_id;
    receipt.span_id = call.span_id;
    receipt
}

fn failure(
    call: RpcV1Call,
    status: u16,
    code: &str,
    message: &str,
    transport: Transport,
) -> RpcV1Receipt {
    let mut error = Map::new();
    error.insert("code".into(), Value::String(code.to_owned()));
    error.insert("message".into(), Value::String(message.to_owned()));
    let mut receipt = RpcV1Receipt::failure(call.id, call.key, status, error);
    receipt.transport = Some(transport);
    receipt.trace_id = call.trace_id;
    receipt.span_id = call.span_id;
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Path, Json};
    use serde_json::json;

    async fn get_item(Path(id): Path<String>) -> Json<Value> {
        Json(json!({"id": id, "handler": "get_item"}))
    }

    async fn other() -> Json<Value> {
        Json(json!({"handler": "other"}))
    }

    #[tokio::test]
    async fn operation_registry_dispatches_only_selected_handler() {
        let map = RouteMap::from_json_str(
            r#"{
                "schema_version":"1.0.0",
                "service":"test",
                "map":{
                    "get_item":{
                        "path":"/v1/items/{id}",
                        "methods":["GET"],
                        "transports":["http"]
                    },
                    "other":{
                        "path":"/v1/other",
                        "methods":["GET"],
                        "transports":["http"]
                    }
                }
            }"#,
        )
        .expect("route map");
        static BINDINGS: &[RpcV1RouteBinding] = &[
            RpcV1RouteBinding::new(
                "get_item",
                "GET",
                "/v1/items/{id}",
                "src/routes/v1/items/[id]/route.rs",
            ),
            RpcV1RouteBinding::new(
                "other",
                "GET",
                "/v1/other",
                "src/routes/v1/other/route.rs",
            ),
        ];
        let mut handlers = BTreeMap::new();
        handlers.insert("get_item", axum_rpc_operation_handler(get_item, ()));
        handlers.insert("other", axum_rpc_operation_handler(other, ()));
        let registry = RpcV1OperationRegistry::new(map, BINDINGS, handlers).expect("registry");

        let mut call = RpcV1Call::new("call-1", "get_item");
        call.path = Some(Map::from_iter([("id".into(), Value::String("abc".into()))]));
        let receipt = registry
            .dispatch_call(call, HeaderMap::new(), Transport::Http)
            .await;
        assert!(receipt.ok);
        assert_eq!(
            receipt.body.value(),
            Some(&json!({"id": "abc", "handler": "get_item"}))
        );
    }

    #[test]
    fn rejects_rpc_transport_endpoint_as_application_operation() {
        let map = RouteMap::from_json_str(
            r#"{
                "schema_version":"1.0.0",
                "service":"test",
                "map":{
                    "rpc_itself":{
                        "path":"/v1/rpc",
                        "methods":["POST"],
                        "transports":["http"]
                    }
                }
            }"#,
        )
        .expect("route map");
        static BINDINGS: &[RpcV1RouteBinding] = &[RpcV1RouteBinding::new(
            "rpc_itself",
            "POST",
            "/v1/rpc",
            "src/routes/v1/rpc/route.rs",
        )];
        let mut handlers = BTreeMap::new();
        handlers.insert("rpc_itself", axum_rpc_operation_handler(other, ()));
        let error = RpcV1OperationRegistry::new(map, BINDINGS, handlers)
            .err()
            .expect("recursive target must fail");
        assert!(matches!(
            error,
            RpcV1OperationRegistryError::RecursiveRpcTarget { .. }
        ));
    }
}
