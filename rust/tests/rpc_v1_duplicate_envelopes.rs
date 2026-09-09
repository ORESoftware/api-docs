use ores_api_docs::{
    decode_rpc_v1_call, decode_rpc_v1_receipt, rpc_v1_call_from_ndjson,
    rpc_v1_receipt_from_ndjson,
};
use serde_json::json;

#[test]
fn every_call_envelope_member_must_be_unique() {
    let base = json!({
        "v": 1, "op": "call", "id": "c1", "key": "get_item",
        "transport": "tcp", "path": {}, "query": {}, "headers": {},
        "body": null, "traceId": "trace", "spanId": "span"
    });
    let encoded = serde_json::to_string(&base).unwrap();
    assert!(decode_rpc_v1_call(encoded.as_bytes()).is_ok());
    for (key, value) in base.as_object().unwrap() {
        let duplicate = format!(
            "{},{}:{value}}}",
            &encoded[..encoded.len() - 1],
            serde_json::to_string(key).unwrap()
        );
        let error = decode_rpc_v1_call(duplicate.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("duplicate RPC envelope member"));
        let line = format!("{duplicate}\n");
        assert!(rpc_v1_call_from_ndjson(line.as_bytes()).is_err());
    }
}

#[test]
fn every_success_and_failure_receipt_member_must_be_unique() {
    let bases = [
        json!({
            "v": 1, "op": "receipt", "id": "c1", "key": "get_item",
            "transport": "tcp", "ok": true, "status": 200, "body": null,
            "traceId": "trace", "spanId": "span"
        }),
        json!({
            "v": 1, "op": "receipt", "id": "c1", "key": "get_item",
            "ok": false, "status": 409, "error": {"code": "conflict"}
        }),
    ];
    for base in bases {
        let encoded = serde_json::to_string(&base).unwrap();
        assert!(decode_rpc_v1_receipt(encoded.as_bytes()).is_ok());
        for (key, value) in base.as_object().unwrap() {
            let duplicate = format!(
                "{},{}:{value}}}",
                &encoded[..encoded.len() - 1],
                serde_json::to_string(key).unwrap()
            );
            let error = decode_rpc_v1_receipt(duplicate.as_bytes()).unwrap_err();
            assert!(error.to_string().contains("duplicate RPC envelope member"));
            let line = format!("{duplicate}\r\n");
            assert!(rpc_v1_receipt_from_ndjson(line.as_bytes()).is_err());
        }
    }
}

#[test]
fn escaped_names_and_conflicting_values_cannot_hide_duplicate_members() {
    let call = br#"{"v":1,"op":"call","id":"first","\u0069d":"second","key":"get_item"}"#;
    assert!(decode_rpc_v1_call(call).is_err());
    let receipts: &[&[u8]] = &[
        br#"{"v":1,"op":"receipt","id":"first","\u0069d":"second","key":"get_item","ok":true}"#,
        br#"{"v":1,"op":"receipt","id":"c1","key":"get_item","ok":false,"ok":true}"#,
        br#"{"v":1,"op":"receipt","id":"c1","key":"get_item","ok":true,"status":500,"status":200}"#,
    ];
    for receipt in receipts {
        assert!(decode_rpc_v1_receipt(receipt).is_err());
    }
}

#[test]
fn_envelope_uniqueness_does_not_confuse_nested_operation_fields() {
    let call = br#"{"v":1,"op":"call","id":"c1","key":"get_item","body":{"id":"item-1","nested":{"id":"nested-1"}}}"#;
    let decoded = decode_rpc_v1_call(call).unwrap();
    assert_eq!(decoded.id, "c1");
    let receipt = br#"{"v":1,"op":"receipt","id":"c1","key":"get_item","ok":true,"body":{"id":"item-1","ok":false}}"#;
    let decoded = decode_rpc_v1_receipt(receipt).unwrap();
    assert!(decoded.ok);
}
