mod generated;

use generated::{CreateUserRequest, ProofRpcClient, UpdateUserRequest};

fn assert_rpc_trace_chain(trace_ids: &[String], expected_rpc: &str, expected_handler: &str) {
    assert_eq!(trace_ids.first().map(String::as_str), Some(expected_rpc));
    assert!(trace_ids.iter().any(|value| value == expected_handler));
    assert!(trace_ids.iter().all(|value| !value.contains("proof-http")));
}

#[tokio::main]
async fn main() {
    let base_url =
        std::env::var("PROOF_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:39091".into());
    let rpc = ProofRpcClient::new(base_url);

    let created = rpc
        .create_user(
            "tenant-rust".into(),
            CreateUserRequest {
                id: "rust-user".into(),
                display_name: "Rust User".into(),
            },
        )
        .await
        .expect("create user");
    assert_eq!(created.result.id, "rust-user");
    assert_rpc_trace_chain(
        &created.trace_ids,
        "ores-trace-proof-rpc-create-M7qT2vB9nLs",
        "ores-trace-proof-handler-create-W8dYzQ8fJ2N",
    );

    let found = rpc
        .find_user_by_id("rust-user".into(), Some(false), None)
        .await
        .expect("find user");
    assert_eq!(found.result.display_name, "Rust User");
    assert_rpc_trace_chain(
        &found.trace_ids,
        "ores-trace-proof-rpc-find-C5mR8xK2vQz",
        "ores-trace-proof-handler-find-bJ7mQ2vA1Ks",
    );

    let updated = rpc
        .update_user(
            "rust-user".into(),
            "rust-idempotency-1".into(),
            UpdateUserRequest {
                display_name: "Rust Updated".into(),
            },
        )
        .await
        .expect("update user");
    assert_eq!(updated.result.display_name, "Rust Updated");
    assert_rpc_trace_chain(
        &updated.trace_ids,
        "ores-trace-proof-rpc-update-P4nV7sJ3bWt",
        "ores-trace-proof-handler-update-pR8tV5xC3Lm",
    );
}
