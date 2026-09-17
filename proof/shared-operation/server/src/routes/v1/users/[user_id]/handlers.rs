use std::sync::atomic::Ordering;

use ores_api_docs::TypedOperationContext;
use ores_api_docs_operation_macros::ores_operation;

use crate::{
    model::{FindUserOperation, OperationEnvelope, ProofError, UpdateUserOperation, User},
    state::AppState,
};

#[ores_operation(
    spec = FindUserOperation,
    key = "demo.users.find_user_by_id",
    codecs("json"),
    default_codec = "json",
    audiences("browser", "server"),
    scope = "regular"
)]
pub async fn find_user_by_id(
    ctx: TypedOperationContext<AppState, FindUserOperation>,
) -> Result<OperationEnvelope<User>, ProofError> {
    ctx.state()
        .counters
        .operation_hits
        .fetch_add(1, Ordering::SeqCst);
    let path = ctx.path().map_err(|error| ProofError {
        code: "path_missing".into(),
        message: error.to_string(),
    })?;
    let _query = ctx.query().map_err(|error| ProofError {
        code: "query_missing".into(),
        message: error.to_string(),
    })?;
    let _headers = ctx.headers().map_err(|error| ProofError {
        code: "headers_missing".into(),
        message: error.to_string(),
    })?;
    let user = ctx
        .state()
        .users
        .lock()
        .expect("users lock")
        .get(&path.user_id)
        .cloned()
        .ok_or_else(|| ProofError::not_found(&path.user_id))?;
    Ok(OperationEnvelope::new(
        user,
        "ores-trace-proof-handler-find-bJ7mQ2vA1Ks",
    ))
}

#[ores_operation(
    spec = UpdateUserOperation,
    key = "demo.users.update_user",
    codecs("json"),
    default_codec = "json",
    audiences("browser", "server"),
    scope = "regular"
)]
pub async fn update_user(
    ctx: TypedOperationContext<AppState, UpdateUserOperation>,
) -> Result<OperationEnvelope<User>, ProofError> {
    ctx.state()
        .counters
        .operation_hits
        .fetch_add(1, Ordering::SeqCst);
    let path = ctx.path().map_err(|error| ProofError {
        code: "path_missing".into(),
        message: error.to_string(),
    })?;
    let headers = ctx.headers().map_err(|error| ProofError {
        code: "headers_missing".into(),
        message: error.to_string(),
    })?;
    let body = ctx.body().map_err(|error| ProofError {
        code: "body_missing".into(),
        message: error.to_string(),
    })?;
    if headers.idempotency_key.is_empty() {
        return Err(ProofError {
            code: "idempotency_key_required".into(),
            message: "idempotency-key is required".into(),
        });
    }
    let mut users = ctx.state().users.lock().expect("users lock");
    let user = users
        .get_mut(&path.user_id)
        .ok_or_else(|| ProofError::not_found(&path.user_id))?;
    user.display_name = body.display_name.clone();
    Ok(OperationEnvelope::new(
        user.clone(),
        "ores-trace-proof-handler-update-pR8tV5xC3Lm",
    ))
}
