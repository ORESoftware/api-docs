use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Request},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use http_body_util::BodyExt;
use ores_api_docs::{
    RouteMap, RpcV1Call, RpcV1RouteBinding, RpcV1RouteRegistry, Transport,
};
use serde_json::{json, Value};
use tower::ServiceExt;

static BINDINGS: &[RpcV1RouteBinding] = &[RpcV1RouteBinding::new(
    "middleware_probe",
    "GET",
    "/v1/middleware-probe",
    "src/routes/v1/middleware-probe/route.rs",
)];

async fn mark_operation_stack(mut request: Request<Body>, next: Next) -> Response {
    request.headers_mut().insert(
        "x-ores-operation-stack",
        HeaderValue::from_static("shared"),
    );
    next.run(request).await
}

async fn probe(headers: HeaderMap) -> Json<Value> {
    Json(json!({
        "operation_stack": headers
            .get("x-ores-operation-stack")
            .and_then(|value| value.to_str().ok())
    }))
}

fn route_map() -> RouteMap {
    RouteMap::from_json_str(
        r#"{
            "schema_version":"1.0.0",
            "service":"middleware-parity-test",
            "map":{
                "middleware_probe":{
                    "path":"/v1/middleware-probe",
                    "methods":["GET"],
                    "transports":["http"]
                }
            }
        }"#,
    )
    .expect("valid route map")
}

fn operation_router() -> Router {
    Router::new()
        .route("/v1/middleware-probe", get(probe))
        .layer(middleware::from_fn(mark_operation_stack))
}

#[tokio::test]
async fn rest_and_rpc_projection_traverse_the_same_operation_middleware() {
    let service = operation_router();

    let direct = service
        .clone()
        .oneshot(
            Request::get("/v1/middleware-probe")
                .body(Body::empty())
                .expect("direct REST request"),
        )
        .await
        .expect("direct REST response");
    let direct_body = direct
        .into_body()
        .collect()
        .await
        .expect("collect direct body")
        .to_bytes();
    assert_eq!(
        serde_json::from_slice::<Value>(&direct_body).expect("direct JSON"),
        json!({"operation_stack":"shared"})
    );

    let registry = RpcV1RouteRegistry::new(route_map(), BINDINGS, service).expect("RPC registry");
    let receipt = registry
        .dispatch_call(
            RpcV1Call::new("rpc-middleware-probe", "middleware_probe"),
            HeaderMap::new(),
            Transport::Http,
        )
        .await;

    assert!(receipt.ok);
    assert_eq!(
        receipt.body.value(),
        Some(&json!({"operation_stack":"shared"}))
    );
}
