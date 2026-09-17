//! RPC projection for handwritten filesystem HTTP routes.
//!
//! `src/routes/**/route.rs` is the implementation authority. Build codegen
//! mounts its Next-style HTTP verb exports into an Axum router. RPC transports
//! do not require a second handwritten handler: the dispatcher translates an
//! RPC call into an in-process HTTP request against that same route service.
//! No loopback socket or network hop is involved.
//!
//! The service stored by [`RpcV1RouteRegistry`] must be the REST operation
//! router only: it must not contain the RPC transport endpoint. Shared
//! operation middleware belongs on that REST router before it is cloned into
//! the registry so plain HTTP and RPC-projected calls traverse the same stack.

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{Map, Value};
use thiserror::Error;
use tower::ServiceExt;

use crate::{
    is_runtime_owned_request_header, rpc_v1_router, OptionalJson, RouteMap, RpcV1Call,
    RpcV1Dispatcher, RpcV1HttpContext, RpcV1Receipt, Transport, RPC_V1_HTTP_PATH,
};

pub type RpcV1RouteFuture = Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>>;

/// Deterministic build-time projection of one HTTP verb in one `route.rs`.
///
/// One source file may contribute several bindings when it exports several
/// HTTP verbs. The operation is still unique: `(path, method) -> operation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpcV1RouteBinding {
    pub operation: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub source: &'static str,
}

impl RpcV1RouteBinding {
    #[must_use]
    pub const fn new(
        operation: &'static str,
        method: &'static str,
        path: &'static str,
        source: &'static str,
    ) -> Self {
        Self {
            operation,
            method,
            path,
            source,
        }
    }
}

/// Compatibility alias for older generated code. New route files do not
/// implement a second RPC handler; the HTTP route service is invoked in process.
pub type RpcV1RouteHandler = fn(RpcV1HttpContext, RpcV1Call) -> RpcV1RouteFuture;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RpcV1RouteRegistryError {
    #[error("filesystem RPC binding from {route_source} has an empty operation key")]
    EmptyOperation { route_source: &'static str },
    #[error("filesystem RPC operation {operation:?} is bound more than once")]
    DuplicateOperation { operation: &'static str },
    #[error(
        "filesystem RPC operation {operation:?} from {route_source} targets reserved RPC transport path {path:?}"
    )]
    ReservedRpcTransportPath {
        operation: &'static str,
        route_source: &'static str,
        path: &'static str,
    },
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
}

#[derive(Clone)]
pub struct RpcV1RouteRegistry {
    routes: Arc<RouteMap>,
    bindings: &'static [RpcV1RouteBinding],
    service: Router,
}

impl RpcV1RouteRegistry {
    /// Construct an RPC projection over a REST-only operation router.
    ///
    /// `service` must not contain [`RPC_V1_HTTP_PATH`] (or descendants). The
    /// route binding inventory also rejects that reserved namespace, which
    /// makes accidental RPC -> RPC recursion fail closed at startup. Apply all
    /// middleware that must be identical for REST and RPC operations to
    /// `service` before calling this constructor.
    pub fn new(
        route_map: RouteMap,
        bindings: &'static [RpcV1RouteBinding],
        service: Router,
    ) -> Result<Self, RpcV1RouteRegistryError> {
        let mut seen = BTreeSet::new();
        for binding in bindings {
            if binding.operation.trim().is_empty() {
                return Err(RpcV1RouteRegistryError::EmptyOperation {
                    route_source: binding.source,
                });
            }
            if !seen.insert(binding.operation) {
                return Err(RpcV1RouteRegistryError::DuplicateOperation {
                    operation: binding.operation,
                });
            }
            if is_reserved_rpc_transport_path(binding.path) {
                return Err(RpcV1RouteRegistryError::ReservedRpcTransportPath {
                    operation: binding.operation,
                    route_source: binding.source,
                    path: binding.path,
                });
            }
            let Some(route) = route_map.lookup(binding.operation) else {
                return Err(RpcV1RouteRegistryError::UnknownOperation {
                    operation: binding.operation,
                    route_source: binding.source,
                });
            };
            if !route.transports.iter().any(|transport| transport == "http") {
                return Err(RpcV1RouteRegistryError::HttpTransportNotAllowed {
                    operation: binding.operation,
                    route_source: binding.source,
                });
            }
            if route.path != binding.path
                || route.methods.len() != 1
                || route.methods[0] != binding.method
            {
                return Err(RpcV1RouteRegistryError::ContractMismatch {
                    operation: binding.operation,
                    route_source: binding.source,
                    method: binding.method,
                    path: binding.path,
                    actual_methods: route.methods.clone(),
                    actual_path: route.path.clone(),
                });
            }
        }

        Ok(Self {
            routes: Arc::new(route_map),
            bindings,
            service,
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

    /// Shared transport entry point. TCP/WebSocket/NATS adapters decode their
    /// transport frame to `RpcV1Call` and invoke this directly. The business
    /// handler is still the generated Axum route service; no socket-level
    /// loopback request is made.
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

        let request = match request_from_call(binding, &call, &trusted_ingress_headers) {
            Ok(request) => request,
            Err(message) => {
                return failure(call, 400, "rpc_http_projection_failed", &message, transport)
            }
        };

        let response = match self.service.clone().oneshot(request).await {
            Ok(response) => response,
            Err(error) => match error {},
        };
        receipt_from_response(call, response, transport).await
    }
}

impl RpcV1Dispatcher for RpcV1RouteRegistry {
    fn dispatch(&self, context: RpcV1HttpContext, call: RpcV1Call) -> RpcV1RouteFuture {
        let dispatcher = self.clone();
        let ingress = context.request_headers().clone();
        Box::pin(async move {
            dispatcher
                .dispatch_call(call, ingress, Transport::Http)
                .await
        })
    }
}

/// Build only the RPC transport router over an already-finalized REST operation
/// router. The caller should merge the returned router with the same `service`
/// instance it supplied here; it must never pass a router that already contains
/// the RPC transport route back into this function.
pub fn filesystem_rpc_v1_router(
    route_map: RouteMap,
    bindings: &'static [RpcV1RouteBinding],
    service: Router,
) -> Result<Router, RpcV1RouteRegistryError> {
    let dispatcher = RpcV1RouteRegistry::new(route_map.clone(), bindings, service)?;
    Ok(rpc_v1_router(route_map, dispatcher))
}

fn is_reserved_rpc_transport_path(path: &str) -> bool {
    path == RPC_V1_HTTP_PATH
        || path
            .strip_prefix(RPC_V1_HTTP_PATH)
            .is_some_and(|suffix| suffix.starts_with('/'))
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
        .map_err(|error| format!("build in-process HTTP request: {error}"))?;
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
    response: axum::response::Response,
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
    use axum::{extract::Path, routing::get, Json};
    use serde_json::json;

    async fn health() -> Json<Value> {
        Json(json!({"status": "ok"}))
    }

    async fn get_item(Path(id): Path<String>) -> Json<Value> {
        Json(json!({"id": id, "verb": "GET"}))
    }

    async fn post_item(Path(id): Path<String>, Json(body): Json<Value>) -> Json<Value> {
        Json(json!({"id": id, "verb": "POST", "body": body}))
    }

    static HEALTH_BINDING: &[RpcV1RouteBinding] = &[RpcV1RouteBinding::new(
        "healthz",
        "GET",
        "/healthz",
        "src/routes/healthz/route.rs",
    )];

    #[test]
    fn reserved_rpc_transport_namespace_is_not_an_operation_target() {
        assert!(is_reserved_rpc_transport_path(RPC_V1_HTTP_PATH));
        assert!(is_reserved_rpc_transport_path("/rpc/v1/internal"));
        assert!(!is_reserved_rpc_transport_path("/rpc/v10"));

        let map = RouteMap::from_json_str(
            r#"{
                "schema_version":"1.0.0",
                "service":"test",
                "map":{
                    "recursive_rpc":{
                        "path":"/rpc/v1",
                        "methods":["POST"],
                        "transports":["http"]
                    }
                }
            }"#,
        )
        .expect("route map");
        static RECURSIVE_BINDING: &[RpcV1RouteBinding] = &[RpcV1RouteBinding::new(
            "recursive_rpc",
            "POST",
            "/rpc/v1",
            "src/routes/rpc/v1/route.rs",
        )];
        let error = RpcV1RouteRegistry::new(map, RECURSIVE_BINDING, Router::new())
            .expect_err("RPC transport route must never be callable as an operation");
        assert!(matches!(
            error,
            RpcV1RouteRegistryError::ReservedRpcTransportPath { .. }
        ));
    }

    #[tokio::test]
    async fn dispatches_rpc_through_same_http_service_without_network() {
        let map =
            RouteMap::from_json_str(include_str!("../../examples/canonical-api.route-map.json"))
                .expect("canonical route map");
        let service = Router::new().route("/healthz", get(health));
        let registry = RpcV1RouteRegistry::new(map, HEALTH_BINDING, service).expect("registry");
        let call = RpcV1Call::new("call-1", "healthz");
        let receipt = registry
            .dispatch_call(call, HeaderMap::new(), Transport::Http)
            .await;
        assert!(receipt.ok);
        assert_eq!(receipt.transport, Some(Transport::Http));
        assert_eq!(receipt.body.value(), Some(&json!({"status": "ok"})));
    }

    #[tokio::test]
    async fn rejects_undeclared_or_mismatched_adapter_transport() {
        let map = RouteMap::from_json_str(
            r#"{
                "schema_version":"1.0.0",
                "service":"test",
                "map":{
                    "healthz":{
                        "path":"/healthz",
                        "methods":["GET"],
                        "transports":["http","tcp"]
                    }
                }
            }"#,
        )
        .expect("route map");
        let service = Router::new().route("/healthz", get(health));
        let registry = RpcV1RouteRegistry::new(map, HEALTH_BINDING, service).expect("registry");

        let denied = registry
            .dispatch_call(
                RpcV1Call::new("call-denied", "healthz"),
                HeaderMap::new(),
                Transport::Nats,
            )
            .await;
        assert!(!denied.ok);
        assert_eq!(denied.status, Some(400));
        assert_eq!(
            denied.error.as_ref().and_then(|e| e.get("code")),
            Some(&Value::String("transport_not_allowed".into()))
        );

        let mut mismatched = RpcV1Call::new("call-mismatch", "healthz");
        mismatched.transport = Some(Transport::Http);
        let receipt = registry
            .dispatch_call(mismatched, HeaderMap::new(), Transport::Tcp)
            .await;
        assert!(!receipt.ok);
        assert_eq!(receipt.status, Some(400));
        assert_eq!(
            receipt.error.as_ref().and_then(|e| e.get("code")),
            Some(&Value::String("transport_mismatch".into()))
        );
    }

    #[tokio::test]
    async fn same_route_file_can_bind_multiple_verbs_and_path_parameters() {
        let map = RouteMap::from_json_str(
            r#"{
                "schema_version":"1.0.0",
                "service":"test",
                "map":{
                    "get_item":{
                        "path":"/v1/items/{id}",
                        "methods":["GET"],
                        "transports":["http","websocket"],
                        "path_params":{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}
                    },
                    "update_item":{
                        "path":"/v1/items/{id}",
                        "methods":["POST"],
                        "transports":["http","websocket"],
                        "path_params":{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}},
                        "request_schema":{"type":"object"}
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
                "update_item",
                "POST",
                "/v1/items/{id}",
                "src/routes/v1/items/[id]/route.rs",
            ),
        ];
        let service = Router::new().route("/v1/items/{id}", get(get_item).post(post_item));
        let registry = RpcV1RouteRegistry::new(map, BINDINGS, service).expect("registry");

        let mut get_call = RpcV1Call::new("call-get", "get_item");
        get_call.path = Some(Map::from_iter([(
            "id".into(),
            Value::String("abc 123".into()),
        )]));
        let get_receipt = registry
            .dispatch_call(get_call, HeaderMap::new(), Transport::Http)
            .await;
        assert!(get_receipt.ok);
        assert_eq!(
            get_receipt.body.value(),
            Some(&json!({"id": "abc 123", "verb": "GET"}))
        );

        let mut post_call = RpcV1Call::new("call-post", "update_item");
        post_call.path = Some(Map::from_iter([("id".into(), Value::String("abc".into()))]));
        post_call.body = OptionalJson::present(json!({"enabled": true}));
        let post_receipt = registry
            .dispatch_call(post_call, HeaderMap::new(), Transport::Websocket)
            .await;
        assert!(post_receipt.ok);
        assert_eq!(
            post_receipt.body.value(),
            Some(&json!({"id": "abc", "verb": "POST", "body": {"enabled": true}}))
        );
    }
}
