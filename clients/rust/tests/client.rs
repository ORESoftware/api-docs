use ores_api_docs_client::{
    assert_rpc_v1_receipt_for_call, decode_rpc_v1_call, decode_rpc_v1_receipt,
    rpc_v1_call_from_ndjson, rpc_v1_receipt_from_ndjson, split_rpc_v1_length_prefixed,
    OptionalJson, RpcV1Call, RpcV1Correlator, RpcV1Receipt, Transport, MAX_FRAME_BYTES,
};
use serde_json::{json, Value};

#[test]
fn calls_roundtrip_for_every_declared_transport() {
    for transport in [Transport::Http, Transport::Tcp, Transport::Websocket, Transport::Nats] {
        let mut call = RpcV1Call::new("client-1", "get_item");
        call.transport = Some(transport);
        call.body = OptionalJson::present(json!({"label": "café", "active": true}));
        let payload = call.encode().unwrap();
        assert_eq!(decode_rpc_v1_call(&payload).unwrap(), call);
    }
}

#[test]
fn absent_and_explicit_null_call_bodies_remain_distinct() {
    let mut call = RpcV1Call::new("client-1", "get_item");
    let absent = call.encode().unwrap();
    assert!(serde_json::from_slice::<Value>(&absent).unwrap().get("body").is_none());
    assert!(!decode_rpc_v1_call(&absent).unwrap().body.is_present());
    call.body = OptionalJson::present(Value::Null);
    let present = call.encode().unwrap();
    assert_eq!(decode_rpc_v1_call(&present).unwrap().body.value(), Some(&Value::Null));
    assert!(serde_json::from_slice::<Value>(&present).unwrap().get("body").is_some());
}

#[test]
fn absent_and_explicit_null_receipt_bodies_remain_distinct() {
    for body in [OptionalJson::absent(), OptionalJson::present(Value::Null)] {
        let receipt = RpcV1Receipt::success("client-1", "get_item", body);
        assert_eq!(decode_rpc_v1_receipt(&receipt.encode().unwrap()).unwrap(), receipt);
    }
}

#[test]
fn malformed_calls_fail_closed() {
    let valid = json!({"v": 1, "op": "call", "id": "client-1", "key": "get_item"});
    for (field, value) in [
        ("v", json!(2)),
        ("op", json!("receipt")),
        ("id", json!("")),
        ("id", json!(7)),
        ("key", json!("")),
        ("headers", json!(42)),
        ("transport", json!("smtp")),
        ("unexpected", json!(true)),
    ] {
        let mut candidate = valid.clone();
        candidate[field] = value;
        assert!(decode_rpc_v1_call(&serde_json::to_vec(&candidate).unwrap()).is_err(), "accepted {field}");
    }
    assert!(decode_rpc_v1_call(&[0xff]).is_err());
    assert!(decode_rpc_v1_call(b"[]").is_err());
    assert!(decode_rpc_v1_call(br#"{"t":"call","id":"client-1"}"#).is_err());
}

#[test]
fn inconsistent_receipt_states_fail_closed() {
    let mut success = RpcV1Receipt::success("client-1", "get_item", OptionalJson::absent());
    success.status = Some(500);
    assert!(success.encode().is_err());
    success.status = Some(200);
    success.error = Some(serde_json::Map::new());
    assert!(success.encode().is_err());
    let mut failure = json!({"v": 1, "op": "receipt", "id": "client-1", "key": "get_item", "ok": false, "status": 500});
    assert!(decode_rpc_v1_receipt(&serde_json::to_vec(&failure).unwrap()).is_err());
    failure["body"] = Value::Null;
    failure["error"] = json!({"code": "INTERNAL", "message": "failure"});
    assert!(decode_rpc_v1_receipt(&serde_json::to_vec(&failure).unwrap()).is_err());
}

#[test]
fn correlation_checks_id_key_and_explicit_transport() {
    let mut call = RpcV1Call::new("client-1", "get_item");
    call.transport = Some(Transport::Tcp);
    let mut receipt = RpcV1Receipt::success("client-1", "get_item", OptionalJson::absent());
    receipt.transport = Some(Transport::Tcp);
    assert_rpc_v1_receipt_for_call(&call, &receipt).unwrap();
    for invalid in [
        RpcV1Receipt { id: "other".into(), ..receipt.clone() },
        RpcV1Receipt { key: "other".into(), ..receipt.clone() },
        RpcV1Receipt { transport: Some(Transport::Websocket), ..receipt.clone() },
    ] {
        assert!(assert_rpc_v1_receipt_for_call(&call, &invalid).is_err());
    }
}

#[test]
fn ndjson_accepts_one_message_and_rejects_multiple_messages() {
    let call = RpcV1Call::new("client-1", "get_item");
    let line = call.to_ndjson().unwrap();
    assert_eq!(rpc_v1_call_from_ndjson(line.as_bytes()).unwrap(), call);
    assert!(rpc_v1_call_from_ndjson(format!("{line}{line}").as_bytes()).is_err());
    let receipt = RpcV1Receipt::success("client-1", "get_item", OptionalJson::absent());
    let line = receipt.to_ndjson().unwrap();
    assert_eq!(rpc_v1_receipt_from_ndjson(line.as_bytes()).unwrap(), receipt);
    assert!(rpc_v1_receipt_from_ndjson(format!("{line}{line}").as_bytes()).is_err());
}

#[test]
fn partial_tcp_frames_are_retained_until_complete() {
    let call = RpcV1Call::new("client-1", "get_item");
    let framed = call.to_length_prefixed().unwrap();
    for end in 0..framed.len() {
        let (frames, tail) = split_rpc_v1_length_prefixed(&framed[..end]).unwrap();
        assert!(frames.is_empty());
        assert_eq!(tail, &framed[..end]);
    }
    let (frames, tail) = split_rpc_v1_length_prefixed(&framed).unwrap();
    assert!(tail.is_empty());
    assert_eq!(frames.len(), 1);
    assert_eq!(decode_rpc_v1_call(frames[0]).unwrap(), call);
}

#[test]
fn coalesced_tcp_frames_preserve_an_incomplete_tail() {
    let first = RpcV1Call::new("client-1", "get_item").to_length_prefixed().unwrap();
    let second = RpcV1Call::new("client-2", "get_item").to_length_prefixed().unwrap();
    let mut stream = first;
    stream.extend_from_slice(&second);
    stream.extend_from_slice(&second[..3]);
    let (frames, tail) = split_rpc_v1_length_prefixed(&stream).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(decode_rpc_v1_call(frames[0]).unwrap().id, "client-1");
    assert_eq!(decode_rpc_v1_call(frames[1]).unwrap().id, "client-2");
    assert_eq!(tail, &second[..3]);
}

#[test]
fn oversized_tcp_prefix_is_rejected_without_a_payload() {
    let prefix = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
    assert!(split_rpc_v1_length_prefixed(&prefix).is_err());
}

#[test]
fn correlation_ids_are_unique_and_prefixes_are_bounded() {
    let mut ids = RpcV1Correlator::new("rust-").unwrap();
    assert_eq!(ids.take().unwrap(), "rust-1");
    assert_eq!(ids.take().unwrap(), "rust-2");
    assert!(RpcV1Correlator::new("x".repeat(128)).is_err());
}

#[test]
fn facade_and_core_use_identical_types() {
    let client: RpcV1Call = ores_api_docs::RpcV1Call::new("client-1", "get_item");
    let core: ores_api_docs::RpcV1Call = client.clone();
    assert_eq!(core, client);
    assert_eq!(core.encode().unwrap(), client.encode().unwrap());
}
