use std::sync::atomic::Ordering;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ores_api_docs::{
    NoSection, OperationContext, OperationRequestData, RpcPayloadCodec, TypedOperationContext,
};
use ores_api_docs_operation_macros::ores_route;

use crate::{
    model::{CreateUserHeaders, CreateUserOperation, CreateUserRequest},
    state::AppState,
};

use super::handlers;

#[ores_route(operation = handlers::create_user)]
pub async fn post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateUserRequest>,
) -> Response {
    state
        .counters
        .http_adapter_hits
        .fetch_add(1, Ordering::SeqCst);
    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    request.insert_path::<CreateUserOperation>(NoSection);
    request.insert_query::<CreateUserOperation>(NoSection);
    request.insert_headers::<CreateUserOperation>(CreateUserHeaders {
        x_ores_tenant: headers
            .get("x-ores-tenant")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned(),
    });
    request.insert_body::<CreateUserOperation>(body);
    request.set_semantic_input(serde_json::json!({"source":"http","operation":"create_user"}));
    let base = OperationContext::http_with_headers(state.clone(), headers)
        .with_policy(state.policy.clone());
    let ctx = TypedOperationContext::<AppState, CreateUserOperation>::new(base, request);
    match handlers::__ores_invoke_create_user(ctx).await {
        Ok(mut output) => {
            output.prepend_trace_id("ores-trace-SAOItFgCPnPU1xXata9VD");
            (StatusCode::CREATED, Json(output)).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, Json(error)).into_response(),
    }
}
