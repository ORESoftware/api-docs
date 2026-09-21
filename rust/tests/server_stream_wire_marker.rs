#![cfg(feature = "operation-runtime")]

#[test]
fn stream_frame_type_remains_shared_with_clients() {
    let frame = ores_api_docs::RpcStreamFrame::End { id: "x".into() };
    assert!(matches!(frame, ores_api_docs::RpcStreamFrame::End { .. }));
}
