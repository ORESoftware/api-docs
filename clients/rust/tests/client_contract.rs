use std::collections::BTreeMap;

use ores_api_docs_client::{
    assert_rpc_v1_receipt_for_call, decode_rpc_v1_call, decode_rpc_v1_receipt,
    encode_length_prefixed, encode_query, expand_path, path_template_vars, rpc_v1_call_from_ndjson,
    rpc_v1_receipt_from_ndjson, split_rpc_v1_length_prefixed, QueryValue, RpcV1Correlator,
    MAX_FRAME_BYTES,
};
use serde_json::json;

const CALL: &[u8] = br#"{"v":1,"op":"call","id":"c1","key":"get_item","transport":"tcp"}"#;
const RECEIPT: &[u8] = br#"{"v":1,"op":"receipt","id":"c1","key":"get_item","transport":"tcp","ok":true,"status":200}"#;

#[test]
fn client_and_core_share_types_and_decoding() {
    let call: ores_api_docs::RpcV1Call = decode_rpc_v1_call(CALL).unwrap();
    assert_eq!(call, ores_api_docs::decode_rpc_v1_call(CALL).unwrap());
    let receipt: ores_api_docs::RpcV1Receipt = decode_rpc_v1_receipt(RECEIPT).unwrap();
    assert_eq!(
        receipt,
        ores_api_docs::decode_rpc_v1_receipt(RECEIPT).unwrap()
    );
    assert_rpc_v1_receipt_for_call(&call, &receipt).unwrap();
}

#[test]
fn absent_body_and_explicit_null_remain_distinct() {
    let absent = decode_rpc_v1_call(CALL).unwrap();
    let mut with_null: serde_json::Value = serde_json::from_slice(CALL).unwrap();
    with_null["body"] = serde_json::Value::Null;
    let present = decode_rpc_v1_call(&serde_json::to_vec(&with_null).unwrap()).unwrap();
    assert_ne!(absent, present);
}

#[test]
fn call_admission_rejects_unknown_duplicate_and_malformed_fields() {
    let invalid: &[&[u8]] = &[
        br#"{"v":1,"op":"call","id":"c1","key":"get_item","unknown":true}"#,
        br#"{"v":1,"op":"call","id":"c1","id":"c2","key":"get_item"}"#,
        br#"{"v":2,"op":"call","id":"c1","key":"get_item"}"#,
        br#"{"v":1,"op":"call","id":"","key":"get_item"}"#,
        br#"{"v":1,"op":"call","id":"c1","key":"get_item","path":[]}"#,
        br#"{"v":1,"op":"call","id":"c1","key":"get_item","transport":"udp"}"#,
        br#"{"v":1,"op":"call","id":"c1","key":"get_item"} trailing"#,
        br#"[]"#,
    ];
    for payload in invalid {
        assert!(decode_rpc_v1_call(payload).is_err(), "accepted {payload:?}");
    }
}

#[test]
fn receipt_success_and_failure_states_are_disjoint() {
    let invalid = [
        json!({"v": 1, "op": "receipt", "id": "c1", "key": "get_item", "ok": true, "status": 500}),
        json!({"v": 1, "op": "receipt", "id": "c1", "key": "get_item", "ok": true, "error": {}}),
        json!({"v": 1, "op": "receipt", "id": "c1", "key": "get_item", "ok": false}),
        json!({"v": 1, "op": "receipt", "id": "c1", "key": "get_item", "ok": false, "error": {}, "body": null}),
        json!({"v": 1, "op": "receipt", "id": "c1", "key": "get_item", "ok": false, "error": {}, "status": 200}),
    ];
    for payload in invalid {
        assert!(decode_rpc_v1_receipt(&serde_json::to_vec(&payload).unwrap()).is_err());
    }
    let failure = json!({"v": 1, "op": "receipt", "id": "c1", "key": "get_item", "ok": false, "error": {"code": "conflict"}, "status": 409});
    assert!(decode_rpc_v1_receipt(&serde_json::to_vec(&failure).unwrap()).is_ok());
}

#[test]
fn receipts_must_match_call_identity_operation_and_transport() {
    let call = decode_rpc_v1_call(CALL).unwrap();
    for (field, value) in [
        ("id", "other"),
        ("key", "other_operation"),
        ("transport", "http"),
    ] {
        let mut payload: serde_json::Value = serde_json::from_slice(RECEIPT).unwrap();
        payload[field] = json!(value);
        let receipt = decode_rpc_v1_receipt(&serde_json::to_vec(&payload).unwrap()).unwrap();
        assert!(assert_rpc_v1_receipt_for_call(&call, &receipt).is_err());
    }
}

#[test]
fn ndjson_admits_exactly_one_envelope() {
    let mut line = CALL.to_vec();
    line.push(b'\n');
    assert_eq!(
        rpc_v1_call_from_ndjson(&line).unwrap(),
        decode_rpc_v1_call(CALL).unwrap()
    );
    line.extend_from_slice(CALL);
    line.push(b'\n');
    assert!(rpc_v1_call_from_ndjson(&line).is_err());
    let mut receipt = RECEIPT.to_vec();
    receipt.push(b'\n');
    assert_eq!(
        rpc_v1_receipt_from_ndjson(&receipt).unwrap(),
        decode_rpc_v1_receipt(RECEIPT).unwrap()
    );
}

#[test]
fn length_prefixed_frames_preserve_incomplete_tail() {
    let first = encode_length_prefixed(CALL).unwrap();
    let second = encode_length_prefixed(RECEIPT).unwrap();
    let mut buffer = first.clone();
    buffer.extend_from_slice(&second[..second.len() - 1]);
    let (frames, remaining) = split_rpc_v1_length_prefixed(&buffer).unwrap();
    assert_eq!(frames, vec![CALL]);
    assert_eq!(remaining, &second[..second.len() - 1]);
    buffer.push(*second.last().unwrap());
    let (frames, remaining) = split_rpc_v1_length_prefixed(&buffer).unwrap();
    assert_eq!(frames, vec![CALL, RECEIPT]);
    assert!(remaining.is_empty());
    for prefix in 0..first.len() {
        let (frames, remaining) = split_rpc_v1_length_prefixed(&first[..prefix]).unwrap();
        assert!(frames.is_empty());
        assert_eq!(remaining, &first[..prefix]);
    }
}

#[test]
fn oversized_frames_are_rejected_before_payload_arrives() {
    let length = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
    assert!(split_rpc_v1_length_prefixed(&length).is_err());
    assert!(encode_length_prefixed(&vec![0; MAX_FRAME_BYTES + 1]).is_err());
}

#[test]
fn path_parameters_are_encoded_and_exactly_matched() {
    let mut params = BTreeMap::from([("id".to_owned(), "a/b ?#é".to_owned())]);
    assert_eq!(
        expand_path("/items/{id}", &params).unwrap(),
        "/items/a%2Fb%20%3F%23%C3%A9"
    );
    params.insert("extra".to_owned(), "value".to_owned());
    assert!(expand_path("/items/{id}", &params).is_err());
    assert!(expand_path("/items/{id}", &BTreeMap::new()).is_err());
    for template in [
        "/items/{id",
        "/items/id}",
        "/items/{id}/{id}",
        "/items/{bad-name}",
    ] {
        assert!(path_template_vars(template).is_err());
    }
}

#[test]
fn query_values_are_sorted_encoded_and_repeated() {
    let query = BTreeMap::from([
        ("z".to_owned(), QueryValue::One("a b".to_owned())),
        (
            "tag".to_owned(),
            QueryValue::Repeat(vec!["x/y".to_owned(), "z".to_owned()]),
        ),
    ]);
    assert_eq!(encode_query(&query), "tag=x%2Fy&tag=z&z=a%20b");
}

#[test]
fn correlation_ids_are_distinct_and_bounded() {
    let mut ids = RpcV1Correlator::new("client-").unwrap();
    assert_eq!(ids.take().unwrap(), "client-1");
    assert_eq!(ids.take().unwrap(), "client-2");
    assert!(RpcV1Correlator::new("x".repeat(128)).is_err());
}
