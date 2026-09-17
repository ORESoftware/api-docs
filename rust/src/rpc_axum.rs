//! Reusable Axum transport for the ORE RPC v1 envelope.
//!
//! This is server-only infrastructure. Web/admin-web consumers use the
//! client-only facade and must never import or mount this module.

use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::post,
    Router,
};
use serde_json::{Map, Value};

use crate::{decode_rpc_v1_call, RouteMap, RpcV1Call, RpcV1Receipt, Transport, MAX_FRAME_BYTES};

#[path = "rpc_operation_router.rs"]
pub mod operation_router;

/// Canonical server-to-server RPC HTTP endpoint mounted by *-api-server.rs.
pub const RPC_V1_HTTP_PATH: &str = "/v1/rpc";
/// Temporary compatibility alias for older generated clients.
pub const RPC_V1_LEGACY_HTTP_PATH: &str = "/rpc/v1";

/// Trusted metadata supplied by the concrete HTTP transport rather than by the
/// application RPC envelope.
///
/// Product dispatchers should use these headers for proxy-derived client
/// identity, transport authentication, request correlation, and other values
/// whose trust depends on the HTTP ingress. `RpcV1Call::headers` remains the
/// typed application-header surface and must not be treated as a substitute for
/// ingress metadata such as `cf-connecting-ip` or `x-real-ip`.
#[derive(Clone, Debug)]
pub struct RpcV1HttpContext {
    request_headers: HeaderMap,
}

impl RpcV1HttpContext {
    #[must_use]
    pub fn from_headers(request_headers: HeaderMap) -> Self {
        Self { request_headers }
    }

    #[must_use]
    pub fn request_headers(&self) -> &HeaderMap {
        &self.request_headers
    }
}

/// Product API servers implement this small boundary and keep
/// authorization/business logic in their reviewed server/core layers. The
/// transport validates framing, route identity and transport admission before
/// dispatch and supplies trusted HTTP ingress context separately from
/// application envelope headers.
pub trait RpcV1Dispatcher: Clone + Send + Sync + 'static {
    fn dispatch(
        &self,
        context: RpcV1HttpContext,
        call: RpcV1Call,
    ) -> Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>>;
}

#[derive(Clone)]
struct RpcState<D> {
    routes: Arc<RouteMap>,
    dispatcher: D,
}

/// Mount the custom RPC v1 endpoint on the API server. Product auth/realm
/// middleware should wrap this router before it is exposed outside the service
/// boundary. `*-web-server.rs` must not mount this router; web servers are RPC
/// clients and should import generated calls from `*-lib-code` instead.
pub fn rpc_v1_router<D>(routes: RouteMap, dispatcher: D) -> Router
where
    D: RpcV1Dispatcher,
{
    Router::new()
        .route(RPC_V1_HTTP_PATH, post(rpc_post::<D>))
        .route(RPC_V1_LEGACY_HTTP_PATH, post(rpc_post::<D>))
        .with_state(RpcState {
            routes: Arc::new(routes),
            dispatcher,
        })
}

async fn rpc_post<D>(
    State(state): State<RpcState<D>>,
    request_headers: HeaderMap,
    body: Bytes,
) -> Response
where
    D: RpcV1Dispatcher,
{
    if body.len() > MAX_FRAME_BYTES {
        return protocol_failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            "frame_too_large",
            "RPC request exceeded the maximum frame size",
        );
    }

    let call = match decode_rpc_v1_call(&body) {
        Ok(call) => call,
        Err(error) => {
            return protocol_failure(
                StatusCode::BAD_REQUEST,
                "invalid_rpc_envelope",
                &error.to_string(),
            )
        }
    };

    let Some(route) = state.routes.lookup(&call.key) else {
        return call_failure(
            &call,
            StatusCode::NOT_FOUND,
            "unknown_rpc_key",
            "unknown RPC key",
        );
    };
    if !route.transports.iter().any(|transport| transport == "http") {
        return call_failure(
            &call,
            StatusCode::BAD_REQUEST,
            "transport_not_allowed",
            "operation does not declare the HTTP transport",
        );
    }
    if call
        .transport
        .is_some_and(|transport| transport != Transport::Http)
    {
        return call_failure(
            &call,
            StatusCode::BAD_REQUEST,
            "transport_mismatch",
            "HTTP RPC endpoint requires transport=http when transport is declared",
        );
    }

    let call_id = call.id.clone();
    let call_key = call.key.clone();
    let trace_id = call.trace_id.clone();
    let span_id = call.span_id.clone();
    let context = RpcV1HttpContext::from_headers(request_headers);
    let mut receipt = state.dispatcher.dispatch(context, call).await;

    // Correlation fields are transport-owned invariants. A dispatcher may omit
    // them but may not redirect a response to another call/key.
    if receipt.id != call_id || receipt.key != call_key {
        return protocol_failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "dispatcher_correlation_mismatch",
            "dispatcher returned a receipt for a different call",
        );
    }
    receipt.transport = Some(Transport::Http);
    if receipt.trace_id.is_none() {
        receipt.trace_id = trace_id;
    }
    if receipt.span_id.is_none() {
        receipt.span_id = span_id;
    }

    let status = receipt
        .status
        .and_then(|value| StatusCode::from_u16(value).ok())
        .unwrap_or(if receipt.ok {
            StatusCode::OK
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        });
    match receipt.encode() {
        Ok(bytes) => json_response(status, bytes),
        Err(error) => protocol_failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_rpc_receipt",
            &error.to_string(),
        ),
    }
}

fn call_failure(call: &RpcV1Call, status: StatusCode, code: &str, message: &str) -> Response {
    let mut error = Map::new();
    error.insert("code".into(), Value::String(code.to_owned()));
    error.insert("message".into(), Value::String(message.to_owned()));
    let mut receipt =
        RpcV1Receipt::failure(call.id.clone(), call.key.clone(), status.as_u16(), error);
    receipt.transport = Some(Transport::Http);
    receipt.trace_id = call.trace_id.clone();
    receipt.span_id = call.span_id.clone();
    match receipt.encode() {
        Ok(bytes) => json_response(status, bytes),
        Err(_) => protocol_failure(status, code, message),
    }
}

fn protocol_failure(status: StatusCode, code: &str, message: &str) -> Response {
    let value = serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    });
    json_response(status, serde_json::to_vec(&value).unwrap_or_default())
}

fn json_response(status: StatusCode, bytes: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
        .header(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        )
        .body(Body::from(bytes))
        .expect("valid RPC response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OptionalJson;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct Echo;

    impl RpcV1Dispatcher for Echo {
        fn dispatch(
            &self,
            context: RpcV1HttpContext,
            call: RpcV1Call,
        ) -> Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>> {
            let client_ip = context
                .request_headers()
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing")
                .to_owned();
            Box::pin(async move {
                RpcV1Receipt::success(
                    call.id,
                    call.key,
                    OptionalJson::present(Value::String(client_ip)),
                )
            })
        }
    }

    fn app() -> Router {
        let map =
            RouteMap::from_json_str(include_str!("../../examples/canonical-api.route-map.json"))
                .expect("canonical route map");
        rpc_v1_router(map, Echo)
    }

    #[tokio::test]
    async fn dispatches_valid_http_rpc_envelope_with_trusted_transport_context() {
        let body = serde_json::json!({
            "v": 1,
            "op": "call",
            "id": "call-1",
            "key": "healthz",
            "transport": "http"
        });
        let response = app()
            .oneshot(
                Request::post(RPC_V1_HTTP_PATH)
                    .header("content-type", "application/json")
                    .header("x-real-ip", "203.0.113.9")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let receipt = crate::decode_rpc_v1_receipt(&bytes).expect("receipt");
        assert!(receipt.ok);
        assert_eq!(receipt.id, "call-1");
        assert_eq!(receipt.key, "healthz");
        assert_eq!(receipt.transport, Some(Transport::Http));
        assert_eq!(
            receipt.body.value(),
            Some(&Value::String("203.0.113.9".into()))
        );
    }

    #[tokio::test]
    async fn legacy_endpoint_alias_remains_available() {
        let body = serde_json::json!({
            "v": 1,
            "op": "call",
            "id": "call-legacy",
            "key": "healthz",
            "transport": "http"
        });
        let response = app()
            .oneshot(
                Request::post(RPC_V1_LEGACY_HTTP_PATH)
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_unknown_key_before_dispatch() {
        let body = serde_json::json!({
            "v": 1,
            "op": "call",
            "id": "call-2",
            "key": "not_real",
            "transport": "http"
        });
        let response = app()
            .oneshot(
                Request::post(RPC_V1_HTTP_PATH)
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
