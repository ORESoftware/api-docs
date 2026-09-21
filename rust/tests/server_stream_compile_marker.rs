#![cfg(feature = "operation-runtime")]

#[test]
fn server_stream_runtime_module_is_linked() {
    let _ = std::any::type_name::<ores_api_docs::OperationServerStream<(), ()>>();
}
