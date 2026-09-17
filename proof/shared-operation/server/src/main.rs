mod model;
mod routes;
mod rpc_support;
mod state;

use std::{future::Future, pin::Pin};

use axum::{extract::State, response::IntoResponse, routing::{get, patch, post}, Json, Router};
use ores_api_docs::{rpc_v1_router, RouteMap, RpcV1Call, RpcV1Dispatcher, RpcV1HttpContext, RpcV1Receipt};

use state::AppState;

#[derive(Clone)]
struct ProofDispatcher {
    state: AppState,
}

impl RpcV1Dispatcher for ProofDispatcher {
    fn dispatch(
        &self,
        context: RpcV1HttpContext,
        call: RpcV1Call,
    ) -> Pin<Box<dyn Future<Output = RpcV1Receipt> + Send + 'static>> {
        let state = self.state.clone();
        Box::pin(async move {
            match call.key.as_str() {
                "demo.users.create_user" => {
                    routes::v1::users::rpc::dispatch(state, context, call).await
                }
                "demo.users.find_user_by_id" => {
                    routes::v1::users::user_id::rpc::dispatch_find(state, context, call).await
                }
                "demo.users.update_user" => {
                    routes::v1::users::user_id::rpc::dispatch_update(state, context, call).await
                }
                _ => unreachable!("rpc_v1_router rejects unknown keys before dispatch"),
            }
        })
    }
}

async fn counters(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.counters.snapshot())
}

async fn health() -> &'static str {
    "ok"
}

fn route_map() -> RouteMap {
    RouteMap::from_json_str(
        r#"{
          "schema_version":"1.0.0",
          "service":"shared-operation-proof",
          "map":{
            "create_user":{
              "path":"/v1/users",
              "methods":["POST"],
              "rpc_key":"demo.users.create_user",
              "transports":["http"]
            },
            "find_user_by_id":{
              "path":"/v1/users/{user_id}",
              "methods":["GET"],
              "rpc_key":"demo.users.find_user_by_id",
              "transports":["http"]
            },
            "update_user":{
              "path":"/v1/users/{user_id}",
              "methods":["PATCH"],
              "rpc_key":"demo.users.update_user",
              "transports":["http"]
            }
          }
        }"#,
    )
    .expect("proof route map")
}

#[tokio::main]
async fn main() {
    let state = AppState::new();
    let rpc = rpc_v1_router(
        route_map(),
        ProofDispatcher {
            state: state.clone(),
        },
    );
    let http = Router::new()
        .route("/healthz", get(health))
        .route("/__proof/counters", get(counters))
        .route("/v1/users", post(routes::v1::users::route::post))
        .route(
            "/v1/users/{user_id}",
            get(routes::v1::users::user_id::route::get)
                .merge(patch(routes::v1::users::user_id::route::patch)),
        )
        .with_state(state);
    let app = http.merge(rpc);

    let port = std::env::var("PROOF_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39091);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind proof server");
    println!("shared-operation proof server listening on http://127.0.0.1:{port}");
    axum::serve(listener, app).await.expect("serve proof server");
}
