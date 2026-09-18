//! Reusable Axum transport for the ORE RPC v1 envelope.
//!
//! This is server-only infrastructure. Web/admin-web consumers use the
//! client-only facade and must never import or mount this module.
//!
//! # Failure logging
//!
//! Every path that answers with a failure instead of a receipt emits one
//! [`RpcErrorEvent`] through the [`RpcTelemetrySink`] seam, carrying a static
//! `ores-trace-` literal written inline at that exact branch. The response it
//! returns is unchanged, byte for byte, whether a sink is mounted or not:
//! observing a failure is not the same as handling it.
//!
//! This crate deliberately has no logging dependency and never imports
//! ores-otel. `Failure` below is the whole vocabulary -- a status, a stable
//! slug, a message for the caller, and the call site's id. The message goes in
//! the HTTP response the caller already sees; it is **not** put on the event,
//! because a message is where a decoder's view of the input would leak.

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

use crate::{
    decode_rpc_v1_call,
    rpc_telemetry::{emit_error, Carrier, ErrorKind, Outcome, RpcErrorEvent, RpcTelemetrySink},
    RouteMap, RpcV1Call, RpcV1Receipt, Transport, MAX_FRAME_BYTES,
};

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
    telemetry: Option<Arc<dyn RpcTelemetrySink>>,
}

/// One failed branch of the transport, described once at the branch itself.
///
/// `ores_trace_id` is a `&'static str` on purpose: an id assembled at runtime
/// cannot name a source location, and a shared constant hoisted to the top of
/// the file would name several. Each construction below writes its own literal.
#[derive(Clone, Copy)]
struct Failure<'a> {
    status: StatusCode,
    kind: ErrorKind,
    /// Stable slug from a closed set, never derived from the request.
    code: &'a str,
    /// Human-readable detail for the caller's response body only.
    message: &'a str,
    ores_trace_id: &'static str,
}

/// Mount the custom RPC v1 endpoint on the API server. Product auth/realm
/// middleware should wrap this router before it is exposed outside the service
/// boundary. `*-web-server.rs` must not mount this router; web servers are RPC
/// clients and should import generated calls from `*-lib-code` instead.
pub fn rpc_v1_router<D>(routes: RouteMap, dispatcher: D) -> Router
where
    D: RpcV1Dispatcher,
{
    router_with_state(routes, dispatcher, None)
}

/// Same router, with an application-owned telemetry sink attached.
///
/// The sink sees one [`RpcErrorEvent`] per failed request and nothing else --
/// no bodies, no headers, no path values, no error messages. It cannot change
/// a response: a sink that errors or panics is contained at the seam, so the
/// caller gets the same bytes either way.
pub fn rpc_v1_router_with_telemetry<D>(
    routes: RouteMap,
    dispatcher: D,
    telemetry: Arc<dyn RpcTelemetrySink>,
) -> Router
where
    D: RpcV1Dispatcher,
{
    router_with_state(routes, dispatcher, Some(telemetry))
}

fn router_with_state<D>(
    routes: RouteMap,
    dispatcher: D,
    telemetry: Option<Arc<dyn RpcTelemetrySink>>,
) -> Router
where
    D: RpcV1Dispatcher,
{
    Router::new()
        .route(RPC_V1_HTTP_PATH, post(rpc_post::<D>))
        .route(RPC_V1_LEGACY_HTTP_PATH, post(rpc_post::<D>))
        .with_state(RpcState {
            routes: Arc::new(routes),
            dispatcher,
            telemetry,
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
    let telemetry = state.telemetry.as_deref();

    if body.len() > MAX_FRAME_BYTES {
        // No key yet: the envelope has not been parsed, and guessing one from
        // oversized bytes is exactly the kind of input-derived label this seam
        // refuses to carry.
        return protocol_failure(
            telemetry,
            "",
            Failure {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                kind: ErrorKind::Protocol,
                code: "frame_too_large",
                message: "RPC request exceeded the maximum frame size",
                ores_trace_id: "ores-trace-QaYrZgvqgeMLyNGqOPFgv",
            },
        );
    }

    let call = match decode_rpc_v1_call(&body) {
        Ok(call) => call,
        Err(error) => {
            // `error` reaches the caller in the response body, where it is
            // already theirs. It does not reach the sink.
            return protocol_failure(
                telemetry,
                "",
                Failure {
                    status: StatusCode::BAD_REQUEST,
                    kind: ErrorKind::Decode,
                    code: "invalid_rpc_envelope",
                    message: &error.to_string(),
                    ores_trace_id: "ores-trace-AGAliDh_pfhcWmzH540vd",
                },
            );
        }
    };

    // Generated clients send the stable dotted rpc_key. Legacy route-map keys
    // remain admitted during migration, but they are not the canonical wire ID.
    let Some(route) = crate::rpc_key_lookup::lookup_rpc_route(&state.routes, &call.key) else {
        return call_failure(
            telemetry,
            &call,
            Failure {
                status: StatusCode::NOT_FOUND,
                kind: ErrorKind::Protocol,
                code: "unknown_rpc_key",
                message: "unknown RPC key",
                ores_trace_id: "ores-trace-8Eff7BvbE2lKQP9FKyXBj",
            },
        );
    };
    if !route.transports.iter().any(|transport| transport == "http") {
        return call_failure(
            telemetry,
            &call,
            Failure {
                status: StatusCode::BAD_REQUEST,
                kind: ErrorKind::Protocol,
                code: "transport_not_allowed",
                message: "operation does not declare the HTTP transport",
                ores_trace_id: "ores-trace-YOcng0NEEKY_lLlHR6j8u",
            },
        );
    }
    if call
        .transport
        .is_some_and(|transport| transport != Transport::Http)
    {
        return call_failure(
            telemetry,
            &call,
            Failure {
                status: StatusCode::BAD_REQUEST,
                kind: ErrorKind::Protocol,
                code: "transport_mismatch",
                message: "HTTP RPC endpoint requires transport=http when transport is declared",
                ores_trace_id: "ores-trace-OffGCNY6Hd2QKYvn9Sqhr",
            },
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
        // The key logged is the one that was *asked for*, not the one the
        // dispatcher answered with, so the event stays inside the route map.
        return protocol_failure(
            telemetry,
            &call_key,
            Failure {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                kind: ErrorKind::Protocol,
                code: "dispatcher_correlation_mismatch",
                message: "dispatcher returned a receipt for a different call",
                ores_trace_id: "ores-trace-nEX1qFyniq4MdMzyuGvdO",
            },
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
            telemetry,
            &call_key,
            Failure {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                kind: ErrorKind::Decode,
                code: "invalid_rpc_receipt",
                message: &error.to_string(),
                ores_trace_id: "ores-trace-5fcIRVwppLodGEaP0GZ3C",
            },
        ),
    }
}

/// Answer a decoded call with a correlated failure receipt.
///
/// Emits exactly one event for the failure named by `failure`. If the failure
/// *receipt itself* cannot be encoded, that is a second and different fault and
/// gets its own event and its own id -- and the fallback response is built with
/// no sink, so the first failure is never reported twice.
fn call_failure(
    telemetry: Option<&dyn RpcTelemetrySink>,
    call: &RpcV1Call,
    failure: Failure<'_>,
) -> Response {
    emit_failure(telemetry, &call.key, failure);

    let mut error = Map::new();
    error.insert("code".into(), Value::String(failure.code.to_owned()));
    error.insert("message".into(), Value::String(failure.message.to_owned()));
    let mut receipt = RpcV1Receipt::failure(
        call.id.clone(),
        call.key.clone(),
        failure.status.as_u16(),
        error,
    );
    receipt.transport = Some(Transport::Http);
    receipt.trace_id = call.trace_id.clone();
    receipt.span_id = call.span_id.clone();
    match receipt.encode() {
        Ok(bytes) => json_response(failure.status, bytes),
        Err(_) => {
            emit_failure(
                telemetry,
                &call.key,
                Failure {
                    kind: ErrorKind::Decode,
                    code: "invalid_rpc_receipt",
                    ores_trace_id: "ores-trace-qxHqguSpUjRSQ25eV_-9L",
                    ..failure
                },
            );
            protocol_failure(None, &call.key, failure)
        }
    }
}

/// Answer with a bare protocol error, for failures that have no correlated
/// call to attach a receipt to.
fn protocol_failure(
    telemetry: Option<&dyn RpcTelemetrySink>,
    key: &str,
    failure: Failure<'_>,
) -> Response {
    emit_failure(telemetry, key, failure);

    let value = serde_json::json!({
        "ok": false,
        "error": {
            "code": failure.code,
            "message": failure.message,
        }
    });
    json_response(
        failure.status,
        serde_json::to_vec(&value).unwrap_or_default(),
    )
}

/// The single point where a transport failure crosses the telemetry seam.
///
/// `failure.message` is not read here, and that is the whole design: the
/// message is for the caller, who already owns whatever is in it.
fn emit_failure(telemetry: Option<&dyn RpcTelemetrySink>, key: &str, failure: Failure<'_>) {
    emit_error(
        telemetry,
        RpcErrorEvent {
            key,
            carrier: Carrier::Http,
            outcome: Outcome::Failed,
            kind: failure.kind,
            code: failure.code,
            ores_trace_id: failure.ores_trace_id,
        },
    );
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
    use crate::{rpc_telemetry::RpcEvent, OptionalJson};
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use std::sync::Mutex;
    use tower::ServiceExt;

    /// Records what actually crossed the seam, so a test can assert on the
    /// whole event rather than on the fact that something was logged.
    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<String>>,
        errors: Mutex<Vec<(String, String, String, &'static str)>>,
    }

    impl RpcTelemetrySink for Recorder {
        fn emit(&self, event: &RpcEvent<'_>) -> Result<(), String> {
            self.calls.lock().expect("recorder").push(event.key.into());
            Ok(())
        }

        fn emit_error(&self, event: &RpcErrorEvent<'_>) -> Result<(), String> {
            self.errors.lock().expect("recorder").push((
                event.key.to_owned(),
                event.code.to_owned(),
                event.kind.as_str().to_owned(),
                event.ores_trace_id,
            ));
            Ok(())
        }
    }

    impl Recorder {
        fn errors(&self) -> Vec<(String, String, String, &'static str)> {
            self.errors.lock().expect("recorder").clone()
        }
    }

    const ORES_ID: &str = r"^ores-(trace|routine)-[A-Za-z0-9_-]{21}$";

    fn is_well_formed_ores_id(id: &str) -> bool {
        // Cheaper than a regex dependency, and the shape is fixed.
        let Some(rest) = id
            .strip_prefix("ores-trace-")
            .or_else(|| id.strip_prefix("ores-routine-"))
        else {
            return false;
        };
        rest.len() == 21
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }

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

    fn canonical_map() -> RouteMap {
        RouteMap::from_json_str(include_str!("../../examples/canonical-api.route-map.json"))
            .expect("canonical route map")
    }

    fn app() -> Router {
        rpc_v1_router(canonical_map(), Echo)
    }

    fn observed_app(recorder: Arc<Recorder>) -> Router {
        rpc_v1_router_with_telemetry(canonical_map(), Echo, recorder)
    }

    async fn post_rpc(app: Router, body: Value) -> (StatusCode, Bytes) {
        let response = app
            .oneshot(
                Request::post(RPC_V1_HTTP_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (status, bytes)
    }

    fn rpc_key_app() -> Router {
        let map = RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo",
              "map":{
                "find_user_by_id":{
                  "path":"/v1/users/{id}",
                  "methods":["GET"],
                  "rpc_key":"demo.users.find_user"
                }
              }
            }"#,
        )
        .expect("rpc key map");
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
    async fn canonical_dotted_rpc_key_is_admitted() {
        let body = serde_json::json!({
            "v": 1,
            "op": "call",
            "id": "call-rpc-key",
            "key": "demo.users.find_user",
            "transport": "http"
        });
        let response = rpc_key_app()
            .oneshot(
                Request::post(RPC_V1_HTTP_PATH)
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
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

    #[tokio::test]
    async fn one_failure_emits_exactly_one_event_with_its_static_id() {
        let recorder = Arc::new(Recorder::default());
        let (status, _) = post_rpc(
            observed_app(recorder.clone()),
            serde_json::json!({
                "v": 1,
                "op": "call",
                "id": "call-unknown",
                "key": "not_real",
                "transport": "http"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            recorder.errors(),
            vec![(
                "not_real".to_owned(),
                "unknown_rpc_key".to_owned(),
                "protocol".to_owned(),
                "ores-trace-8Eff7BvbE2lKQP9FKyXBj",
            )]
        );
    }

    #[tokio::test]
    async fn an_undecodable_envelope_is_logged_without_quoting_it() {
        let recorder = Arc::new(Recorder::default());
        let response = observed_app(recorder.clone())
            .oneshot(
                Request::post(RPC_V1_HTTP_PATH)
                    .body(Body::from("{\"v\":1,\"op\":\"call\",\"key\":\"n0t-json"))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let errors = recorder.errors();
        assert_eq!(errors.len(), 1);
        let (key, code, kind, id) = errors[0].clone();
        // No key had been parsed yet, and none is invented from the bytes.
        assert_eq!(key, "");
        assert_eq!(code, "invalid_rpc_envelope");
        assert_eq!(kind, "decode");
        assert_eq!(id, "ores-trace-AGAliDh_pfhcWmzH540vd");
        // The rejected bytes never reached the sink, only a fixed slug did.
        assert!(!format!("{errors:?}").contains("n0t-json"));
    }

    #[tokio::test]
    async fn a_transport_mismatch_is_logged_and_still_rejected() {
        let recorder = Arc::new(Recorder::default());
        let (status, bytes) = post_rpc(
            observed_app(recorder.clone()),
            serde_json::json!({
                "v": 1,
                "op": "call",
                "id": "call-tcp",
                "key": "healthz",
                "transport": "tcp"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let receipt = crate::decode_rpc_v1_receipt(&bytes).expect("receipt");
        assert!(!receipt.ok);
        assert_eq!(receipt.id, "call-tcp");
        let errors = recorder.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, "transport_mismatch");
        assert_eq!(errors[0].3, "ores-trace-OffGCNY6Hd2QKYvn9Sqhr");
    }

    #[tokio::test]
    async fn a_correlation_mismatch_names_the_key_that_was_asked_for() {
        #[derive(Clone)]
        struct Liar;
        impl RpcV1Dispatcher for Liar {
            fn dispatch(
                &self,
                _context: RpcV1HttpContext,
                call: RpcV1Call,
            ) -> Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>> {
                Box::pin(async move {
                    RpcV1Receipt::success(
                        call.id,
                        "some_other_key".to_owned(),
                        OptionalJson::present(Value::Null),
                    )
                })
            }
        }

        let recorder = Arc::new(Recorder::default());
        let app = rpc_v1_router_with_telemetry(canonical_map(), Liar, recorder.clone());
        let (status, _) = post_rpc(
            app,
            serde_json::json!({
                "v": 1,
                "op": "call",
                "id": "call-liar",
                "key": "healthz",
                "transport": "http"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let errors = recorder.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "healthz");
        assert_eq!(errors[0].1, "dispatcher_correlation_mismatch");
        assert_eq!(errors[0].3, "ores-trace-nEX1qFyniq4MdMzyuGvdO");
    }

    #[tokio::test]
    async fn a_successful_call_emits_no_error_event() {
        let recorder = Arc::new(Recorder::default());
        let (status, _) = post_rpc(
            observed_app(recorder.clone()),
            serde_json::json!({
                "v": 1,
                "op": "call",
                "id": "call-ok",
                "key": "healthz",
                "transport": "http"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(recorder.errors().is_empty());
    }

    #[tokio::test]
    async fn a_panicking_sink_does_not_change_the_response() {
        struct Boom;
        impl RpcTelemetrySink for Boom {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                Ok(())
            }
            fn emit_error(&self, _: &RpcErrorEvent<'_>) -> Result<(), String> {
                panic!("exporter is down");
            }
        }

        let body = serde_json::json!({
            "v": 1,
            "op": "call",
            "id": "call-unknown",
            "key": "not_real",
            "transport": "http"
        });
        let (unobserved_status, unobserved_bytes) = post_rpc(app(), body.clone()).await;
        let (observed_status, observed_bytes) = post_rpc(
            rpc_v1_router_with_telemetry(canonical_map(), Echo, Arc::new(Boom)),
            body,
        )
        .await;

        assert_eq!(observed_status, unobserved_status);
        assert_eq!(observed_bytes, unobserved_bytes);
    }

    #[test]
    fn every_failure_site_carries_its_own_well_formed_inline_id() {
        // The ids are literals, so the file itself is the evidence: no hoisted
        // constant, no runtime assembly, and no id serving two branches.
        let source = include_str!("rpc_axum.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("the module has a body before its tests");

        // Only literals: `"ores-trace-…"`. Prose mentions of the prefix in the
        // module documentation are not call sites.
        let mut ids: Vec<&str> = production
            .match_indices("\"ores-trace-")
            .map(|(start, _)| {
                let rest = &production[start + 1..];
                let end = rest
                    .find('"')
                    .expect("an ores-trace- id is written as a string literal");
                &rest[..end]
            })
            .collect();

        assert_eq!(ids.len(), 8, "one id per failure branch: {ids:?}");
        for id in &ids {
            assert!(is_well_formed_ores_id(id), "{id} does not match {ORES_ID}");
        }
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "a trace id is reused across call sites");
    }
}
