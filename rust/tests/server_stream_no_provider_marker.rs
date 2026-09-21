#![cfg(feature = "operation-runtime")]

#[test]
fn stream_runtime_requires_no_provider_feature() {
    let _ = ores_api_docs::rpc_stream_frame_json(&ores_api_docs::RpcStreamFrame::End {
        id: "provider-neutral".to_owned(),
    });
}
