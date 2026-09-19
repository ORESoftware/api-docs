#![forbid(unsafe_code)]

use std::net::SocketAddr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use ores_rpc_shared_operation_proof_rust_client::generated::{
    CreateUserRequest, ProofRpcClient, User,
};

#[derive(Clone)]
struct AppState {
    api: ProofRpcClient,
}

async fn health() -> &'static str {
    "ok"
}

/// Browser-facing web handler whose server-side data dependency is the
/// generated RPC client. It does not import or mount the API server router.
async fn rpc_round_trip(
    Path(user_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<User>, (StatusCode, String)> {
    let created = state
        .api
        .create_user(
            "tenant-web-server".into(),
            CreateUserRequest {
                id: user_id.clone(),
                display_name: "Web Server RPC User".into(),
            },
        )
        .await
        .map_err(bad_gateway)?;
    if created.result.id != user_id {
        return Err((
            StatusCode::BAD_GATEWAY,
            "RPC create returned the wrong user id".into(),
        ));
    }

    let found = state
        .api
        .find_user_by_id(user_id, Some(false), None)
        .await
        .map_err(bad_gateway)?;
    Ok(Json(found.result))
}

fn bad_gateway(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_GATEWAY, message)
}

#[tokio::main]
async fn main() {
    let api_base_url = std::env::var("PROOF_API_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:39091".into());
    let port = std::env::var("PROOF_WEB_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39092);

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/__proof/rpc-user/{user_id}", get(rpc_round_trip))
        .with_state(AppState {
            api: ProofRpcClient::new(api_base_url),
        });

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind proof web server");
    println!("shared-operation proof web server listening on http://{addr}");
    axum::serve(listener, app)
        .await
        .expect("serve proof web server");
}
