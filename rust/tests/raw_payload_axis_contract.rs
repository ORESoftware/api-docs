use ores_api_docs::{OperationResponseRepresentation, RpcPayloadCodec};

fn structured_codec_name(codec: RpcPayloadCodec) -> &'static str {
    return match codec {
        RpcPayloadCodec::Json => "json",
        RpcPayloadCodec::Messagepack => "messagepack",
        RpcPayloadCodec::Cbor => "cbor",
        RpcPayloadCodec::Protobuf => "protobuf",
    };
}

#[test]
fn raw_bytes_remain_a_response_representation_not_a_structured_rpc_codec() {
    let structured_codecs = [
        RpcPayloadCodec::Json,
        RpcPayloadCodec::Messagepack,
        RpcPayloadCodec::Cbor,
        RpcPayloadCodec::Protobuf,
    ];

    assert_eq!(structured_codecs.len(), 4);
    assert_eq!(
        structured_codecs.map(structured_codec_name),
        ["json", "messagepack", "cbor", "protobuf"]
    );
    assert_eq!(OperationResponseRepresentation::Binary.as_str(), "binary");
}
