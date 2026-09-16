//! Project ORE RPC v1 calls through the same Axum route service used by normal HTTP.
//!
//! Filesystem `route.rs` modules stay HTTP-first. Generated code builds their
//! ordinary Axum router and this dispatcher synthesizes an HTTP request from an
//! RPC envelope, invokes that exact router, then projects the HTTP response back
//! into an RPC receipt. There is therefore no second product handler to drift.

use std::{collections::BTreeMap, future::Future, pin::Pin};

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderName, HeaderValue, Method, Request, StatusCode},
    Router,
};
use serde_json::{Map, Value};
use thiserror::Error;
use tower::ServiceExt;

use crate::{
    encode_query, expand_path, is_runtime_owned_request_header, OptionalJson, QueryValue, RouteMap,
    RpcV1Call, RpcV1Dispatcher, RpcV1HttpContext, RpcV1Receipt, MAX_FRAME_BYTES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpcV1HttpRouteBinding {
    pub operation: &'static str,
    pub source: &'static str,
    pub method: &'static str,
    pub path: &'static str,
}

impl RpcV1HttpRouteBinding {
    #[must_use]
    pub const fn new(
        operation: &'static str,
        source: &'static str,
        method: &'static str,
        path: &'static str,
    ) -> Self {
        Self {
            operation,
            source,
            method,
            path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RpcV1HttpBridgeError {
    #[error("filesystem HTTP/RPC binding from {source} has an empty operation key")]
    EmptyOperation { source: &'static str },
    #[error("filesystem HTTP/RPC operation {operation:?} is bound more than once")]
    DuplicateOperation { operation: &'static str },
    #[error("filesystem HTTP/RPC operation {operation:?} from {source} is absent from the route map")]
    UnknownOperation {
        operation: &'static str,
        source: &'static str,
    },
    #[error("filesystem HTTP/RPC operation {operation:?} from {source} does not admit HTTP transport")]
    HttpTransportNotAllowed {
        operation: &'static str,
        source: &'static str,
    },
    #[error("filesystem HTTP/RPC operation {operation:?} from {source} expected path {expected:?}, got {actual:?}")]
    PathMismatch {
        operation: &'static str,
        source: &'static str,
        expected: String,
        actual: &'static str,
    },
    #[error("filesystem HTTP/RPC operation {operation:?} from {source} does not admit method {method}")]
    MethodMismatch {
        operation: &'static str,
        source: &'static str,
        method: &'static str,
    },
}

/// Clone-cheap RPC dispatcher backed by the already-stateful HTTP route router.
///
/// A monolith supplies every generated binding. A route-scoped function build
/// supplies only the selected route file's bindings and the correspondingly
/// small HTTP router. Both execute the exact same authored verb handlers.
#[derive(Clone)]
pub struct RpcV1HttpRouterDispatcher {
    router: Router,
    bindings: &'static [RpcV1HttpRouteBinding],
}

impl RpcV1HttpRouterDispatcher {
    pub fn new(
        route_map: &RouteMap,
        bindings: &'static [RpcV1HttpRouteBinding],
        router: Router,
    ) -> Result<Self, RpcV1HttpBridgeError> {
        let mut seen = std::collections::BTreeSet::new();
        for binding in bindings {
            if binding.operation.trim().is_empty() {
                return Err(RpcV1HttpBridgeError::EmptyOperation {
                    source: binding.source,
                });
            }
            if !seen.insert(binding.operation) {
                return Err(RpcV1HttpBridgeError::DuplicateOperation {
                    operation: binding.operation,
                });
            }
            let Some(route) = route_map.lookup(binding.operation) else {
                return Err(RpcV1HttpBridgeError::UnknownOperation {
                    operation: binding.operation,
                    source: binding.source,
                });
            };
            if !route.transports.iter().any(|transport| transport == "http") {
                return Err(RpcV1HttpBridgeError::HttpTransportNotAllowed {
                    operation: binding.operation,
                    source: binding.source,
                });
            }
            if route.path != binding.path {
                return Err(RpcV1HttpBridgeError::PathMismatch {
                    operation: binding.operation,
                    source: binding.source,
                    expected: route.path.clone(),
                    actual: binding.path,
                });
            }
            if !route.methods.iter().any(|method| method == binding.method) {
                return Err(RpcV1HttpBridgeError::MethodMismatch {
                    operation: binding.operation,
                    source: binding.source,
                    method: binding.method,
                });
            }
        }
        Ok(Self { router, bindings })
    }

    #[must_use]
    pub fn bindings(&self) -> &'static [RpcV1HttpRouteBinding] {
        self.bindings
    }
}

impl RpcV1Dispatcher for RpcV1HttpRouterDispatcher {
    fn dispatch(
        &self,
        context: RpcV1HttpContext,
        call: RpcV1Call,
    ) -> Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>> {
        let Some(binding) = self
            .bindings
            .iter()
            .copied()
            .find(|binding| binding.operation == call.key)
        else {
            return Box::pin(async move {
                failure(
                    &call,
                    StatusCode::NOT_IMPLEMENTED,
                    "rpc_route_not_in_build",
                    "RPC operation is valid for the service but is not included in this build slice",
                )
            });
        };
        let router = self.router.clone();
        Box::pin(async move { dispatch_through_http(router, binding, context, call).await })
    }
}

/// Mount `/rpc/v1` by projecting RPC calls through an already-stateful Axum
/// route router. The normal HTTP router can be merged alongside the returned
/// RPC router by generated product glue.
pub fn filesystem_http_rpc_v1_router(
    route_map: RouteMap,
    bindings: &'static [RpcV1HttpRouteBinding],
    http_router: Router,
) -> Result<Router, RpcV1HttpBridgeError> {
    let dispatcher = RpcV1HttpRouterDispatcher::new(&route_map, bindings, http_router)?;
    Ok(crate::rpc_v1_router(route_map, dispatcher))
}

async fn dispatch_through_http(
    router: Router,
    binding: RpcV1HttpRouteBinding,
    context: RpcV1HttpContext,
    call: RpcV1Call,
) -> RpcV1Receipt {
    let uri = match request_uri(binding.path, &call) {
        Ok(uri) => uri,
        Err(message) => {
            return failure(
                &call,
                StatusCode::BAD_REQUEST,
                "invalid_rpc_http_projection",
                &message,
            )
        }
    };
    let method = match Method::from_bytes(binding.method.as_bytes()) {
        Ok(method) => method,
        Err(_) => {
            return failure(
                &call,
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_generated_http_method",
                "generated filesystem HTTP method is invalid",
            )
        }
    };

    let mut request = match Request::builder().method(method).uri(uri).body(rpc_body(&call)) {
        Ok(request) => request,
        Err(error) => {
            return failure(
                &call,
                StatusCode::BAD_REQUEST,
                "invalid_rpc_http_request",
                &error.to_string(),
            )
        }
    };

    // The outer HTTP request is the trusted ingress. Copy useful ingress
    // metadata first, while dropping framing headers that describe the RPC
    // envelope rather than the synthesized product request.
    for (name, value) in context.request_headers() {
        if is_projection_framing_header(name.as_str()) {
            continue;
        }
        request.headers_mut().append(name.clone(), value.clone());
    }

    if let Some(headers) = &call.headers {
        for (name, value) in headers {
            let normalized = name.to_ascii_lowercase();
            if is_runtime_owned_request_header(&normalized) {
                return failure(
                    &call,
                    StatusCode::BAD_REQUEST,
                    "runtime_header_override",
                    "RPC application headers may not override runtime-owned ingress headers",
                );
            }
            let Ok(header_name) = HeaderName::from_bytes(normalized.as_bytes()) else {
                return failure(
                    &call,
                    StatusCode::BAD_REQUEST,
                    "invalid_application_header",
                    "RPC application header name is invalid",
                );
            };
            match append_json_header(request.headers_mut(), header_name, value) {
                Ok(()) => {}
                Err(message) => {
                    return failure(
                        &call,
                        StatusCode::BAD_REQUEST,
                        "invalid_application_header",
                        &message,
                    )
                }
            }
        }
    }
    if call.body.is_present() && !request.headers().contains_key(header::CONTENT_TYPE) {
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }

    let response = match router.oneshot(request).await {
        Ok(response) => response,
        Err(error) => match error {},
    };
    let status = response.status();
    let content_type_is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("application/json")
                || value
                    .to_ascii_lowercase()
                    .starts_with("application/json;")
                || value.to_ascii_lowercase().contains("+json")
        });
    let bytes = match to_bytes(response.into_body(), MAX_FRAME_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return failure(
                &call,
                StatusCode::BAD_GATEWAY,
                "http_response_too_large",
                &error.to_string(),
            )
        }
    };
    let body = match decode_http_body(&bytes, content_type_is_json) {
        Ok(body) => body,
        Err(message) => {
            return failure(
                &call,
                StatusCode::BAD_GATEWAY,
                "invalid_http_route_response",
                &message,
            )
        }
    };

    if status.is_success() || status.is_redirection() {
        let mut receipt = RpcV1Receipt::success(call.id, call.key, body);
        receipt.status = Some(status.as_u16());
        receipt
    } else {
        let mut error = Map::new();
        error.insert(
            "code".into(),
            Value::String(format!("http_{}", status.as_u16())),
        );
        if let Some(value) = body.value() {
            match value {
                Value::Object(object) if object.get("error").is_some() => {
                    error.insert("response".into(), Value::Object(object.clone()));
                }
                value => {
                    error.insert("response".into(), value.clone());
                }
            }
        }
        RpcV1Receipt::failure(call.id, call.key, status.as_u16(), error)
    }
}

fn request_uri(path_template: &str, call: &RpcV1Call) -> Result<String, String> {
    let mut path = BTreeMap::new();
    if let Some(values) = &call.path {
        for (name, value) in values {
            path.insert(name.clone(), scalar_string(value)?);
        }
    }
    let mut uri = expand_path(path_template, &path).map_err(|error| error.to_string())?;
    if let Some(values) = &call.query {
        let mut query = BTreeMap::new();
        for (name, value) in values {
            let value = match value {
                Value::Array(items) => QueryValue::Repeat(
                    items
                        .iter()
                        .map(scalar_string)
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                value => QueryValue::One(scalar_string(value)?),
            };
            query.insert(name.clone(), value);
        }
        let encoded = encode_query(&query);
        if !encoded.is_empty() {
            uri.push('?');
            uri.push_str(&encoded);
        }
    }
    Ok(uri)
}

fn scalar_string(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err("path/query values must be strings, numbers, booleans, or arrays of those".into()),
    }
}

fn rpc_body(call: &RpcV1Call) -> Body {
    match call.body.value() {
        Some(value) => Body::from(serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec())),
        None => Body::empty(),
    }
}

fn append_json_header(
    headers: &mut axum::http::HeaderMap,
    name: HeaderName,
    value: &Value,
) -> Result<(), String> {
    match value {
        Value::Array(values) => {
            for value in values {
                let value = scalar_string(value)?;
                let value = HeaderValue::from_str(&value)
                    .map_err(|_| "RPC application header value is invalid".to_owned())?;
                headers.append(name.clone(), value);
            }
        }
        value => {
            let value = scalar_string(value)?;
            let value = HeaderValue::from_str(&value)
                .map_err(|_| "RPC application header value is invalid".to_owned())?;
            headers.append(name, value);
        }
    }
    Ok(())
}

fn decode_http_body(bytes: &[u8], json: bool) -> Result<OptionalJson, String> {
    if bytes.is_empty() {
        return Ok(OptionalJson::absent());
    }
    if json {
        return serde_json::from_slice(bytes)
            .map(OptionalJson::present)
            .map_err(|error| format!("HTTP route returned invalid JSON: {error}"));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "non-JSON HTTP route response must be UTF-8 for RPC projection".to_owned())?;
    Ok(OptionalJson::present(Value::String(text.to_owned())))
}

fn is_projection_framing_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "content-length" | "content-type" | "transfer-encoding" | "connection" | "host"
    )
}

fn failure(call: &RpcV1Call, status: StatusCode, code: &str, message: &str) -> RpcV1Receipt {
    let mut error = Map::new();
    error.insert("code".into(), Value::String(code.to_owned()));
    error.insert("message".into(), Value::String(message.to_owned()));
    RpcV1Receipt::failure(call.id.clone(), call.key.clone(), status.as_u16(), error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Path, routing::get};

    #[test]
    fn binding_can_validate_multiple_verbs_at_one_path() {
        static BINDINGS: &[RpcV1HttpRouteBinding] = &[
            RpcV1HttpRouteBinding::new("list_quotes", "src/routes/api/v1/quotes/route.rs", "GET", "/api/v1/quotes"),
            RpcV1HttpRouteBinding::new("create_quote", "src/routes/api/v1/quotes/route.rs", "POST", "/api/v1/quotes"),
        ];
        let map = RouteMap::from_json_str(include_str!("../../examples/canonical-api.route-map.json"))
            .expect("canonical route map");
        let dispatcher = RpcV1HttpRouterDispatcher::new(&map, BINDINGS, Router::new())
            .expect("same route file may own GET and POST");
        assert_eq!(dispatcher.bindings().len(), 2);
    }

    #[tokio::test]
    async fn rpc_projection_reuses_the_http_path_handler() {
        async fn handler(Path(id): Path<String>) -> String {
            format!("item:{id}")
        }
        let router = Router::new().route("/v1/items/{id}", get(handler));
        let binding = RpcV1HttpRouteBinding::new("get_item", "src/routes/v1/items/[id]/route.rs", "GET", "/v1/items/{id}");
        let mut call = RpcV1Call::new("call-1", "get_item");
        call.path = Some(Map::from_iter([("id".into(), Value::String("abc".into()))]));
        let receipt = dispatch_through_http(
            router,
            binding,
            RpcV1HttpContext::from_headers(Default::default()),
            call,
        )
        .await;
        assert!(receipt.ok);
        assert_eq!(receipt.body.value(), Some(&Value::String("item:abc".into())));
    }
}
