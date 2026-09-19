use std::sync::atomic::Ordering;

use ores_api_docs::TypedOperationContext;
use ores_api_docs_operation_macros::ores_operation;

use crate::{
    model::{CreateUserOperation, OperationEnvelope, ProofError, User},
    state::AppState,
};

#[ores_operation(
    spec = CreateUserOperation,
    key = "demo.users.create_user",
    codecs("json"),
    default_codec = "json",
    audiences("browser", "server"),
    scope = "regular"
)]
pub async fn create_user(
    ctx: TypedOperationContext<AppState, CreateUserOperation>,
) -> Result<OperationEnvelope<User>, ProofError> {
    ctx.state()
        .counters
        .operation_hits
        .fetch_add(1, Ordering::SeqCst);
    let headers = ctx.headers().map_err(|error| ProofError {
        code: "headers_missing".into(),
        message: error.to_string(),
    })?;
    let body = ctx.body().map_err(|error| ProofError {
        code: "body_missing".into(),
        message: error.to_string(),
    })?;
    if headers.x_ores_tenant.is_empty() {
        return Err(ProofError {
            code: "tenant_required".into(),
            message: "x-ores-tenant is required".into(),
        });
    }
    let user = User {
        id: body.id.clone(),
        display_name: body.display_name.clone(),
    };
    ctx.state()
        .users
        .lock()
        .expect("users lock")
        .insert(user.id.clone(), user.clone());
    Ok(OperationEnvelope::new(
        user,
        "ores-trace-kyWJwSSkCRw6JPP1fGBXa",
    ))
}
