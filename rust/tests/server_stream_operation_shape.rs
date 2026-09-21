#![cfg(feature = "operation-runtime")]

use ores_api_docs::OperationServerStream;

fn assert_shape<T, E>(_stream: OperationServerStream<T, E>) {}

#[test]
fn authored_stream_wrapper_is_transport_neutral() {
    let _ = assert_shape::<serde_json::Value, serde_json::Value>
        as fn(OperationServerStream<serde_json::Value, serde_json::Value>);
}
