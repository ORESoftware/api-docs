#![cfg(feature = "operation-runtime")]

#[test]
fn operation_runtime_exports_stream_helpers() {
    let _ = ores_api_docs::rpc_v1_server_stream_from_frames(std::iter::empty());
}
