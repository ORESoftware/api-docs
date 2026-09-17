use std::sync::atomic::Ordering;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ores_api_docs::{NoSection, OperationContext, OperationRequestData, RpcPayloadCodec, TypedOperationContext};
use ores_api_docs_operation_macros::ores_route;

use crate::{
    model::{
        FindUserHeaders, FindUserOperation, FindUserPath, FindUserQuery, UpdateUserHeaders,
        UpdateUserOperation, UpdateUserPath, UpdateUserRequest,
    },
    state::AppState,
};

use super::handlers;

#[ores_route(operation = handlers::find_user_by_id)]
pub async fn get(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<FindUserQuery>,
    headers: HeaderMap,
) -> Response {
    state
        .counters
        .http_adapter_hits
        .fetch_add(1, Ordering::SeqCst);
    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    request.insert_path::<FindUserOperation>(FindUserPath { user_id });
    request.insert_query::<FindUserOperation>(query);
    request.insert_headers::<FindUserOperation>(FindUserHeaders {
        if_none_match: headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    });
    request.insert_body::<FindUserOperation>(NoSection);
    request.set_semantic_input(serde_json::json!({"source":"http","operation":"find_user_by_id"}));
    let base = OperationContext::http_with_headers(state.clone(), headers)
        .with_policy(state.policy.clone());
    let ctx = TypedOperationContext::<AppState, FindUserOperation>::new(base, request);
    match handlers::__ores_invoke_find_user_by_id(ctx).await {
        Ok(mut output) => {
            output.prepend_trace_id("ores-trace-proof-http-find-Z3vP8kR5mQs");
            Json(output).into_response()
        }
        Err(error) => (StatusCode::NOT_FOUND, Json(error)).into_response(),
    }
}

#[ores_route(operation = handlers::update_user)]
pub async fn patch(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateUserRequest>,
) -> Response {
    state
        .counters
        .http_adapter_hits
        .fetch_add(1, Ordering::SeqCst);
    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    request.insert_path::<UpdateUserOperation>(UpdateUserPath { user_id });
    request.insert_query::<UpdateUserOperation>(NoSection);
    request.insert_headers::<UpdateUserOperation>(UpdateUserHeaders {
        idempotency_key: headers
            .get("idempotency-key")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned(),
    });
    request.insert_body::<UpdateUserOperation>(body);
    request.set_semantic_input(serde_json::json!({"source":"http","operation":"update_user"}));
    let base = OperationContext::http_with_headers(state.clone(), headers)
        .with_policy(state.policy.clone());
    let ctx = TypedOperationContext::<AppState, UpdateUserOperation>::new(base, request);
    match handlers::__ores_invoke_update_user(ctx).await {
        Ok(mut output) => {
            output.prepend_trace_id("ores-trace-proof-http-update-H4tN9sV2bXq");
            Json(output).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, Json(error)).into_response(),
    }
}
