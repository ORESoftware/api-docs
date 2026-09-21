#![cfg(feature = "operation-runtime")]

use ores_api_docs::{
    rpc_stream_frame_json, rpc_v1_server_stream_from_frames, OperationServerStream,
    OperationStreamDispatchFn, RpcStreamFrame,
};

#[test]
fn operation_server_stream_is_an_additive_runtime_type() {
    fn accepts(_stream: OperationServerStream<String, String>) {}
    let _ = accepts as fn(OperationServerStream<String, String>);
}

#[test]
fn server_stream_dispatch_fn_is_public_host_abi() {
    let _ = std::any::type_name::<OperationStreamDispatchFn>();
}

#[test]
fn canonical_server_frames_are_the_client_protocol() {
    let frame = RpcStreamFrame::Data {
        id: "stream-7".to_owned(),
        body: serde_json::json!({"event":"one"}),
    };
    assert_eq!(
        rpc_stream_frame_json(&frame),
        serde_json::json!({"v":1,"id":"stream-7","t":"data","body":{"event":"one"}})
    );
    let _ = rpc_v1_server_stream_from_frames([
        frame,
        RpcStreamFrame::End {
            id: "stream-7".to_owned(),
        },
    ]);
}
