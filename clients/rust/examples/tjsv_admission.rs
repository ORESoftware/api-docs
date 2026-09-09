//! Fixed CI oracle for the TJSV admission runner; no flags or network I/O.
//! Schema validation is owned by the shared core, not duplicated here.

use ores_api_docs_client::{decode_rpc_v1_call, decode_rpc_v1_receipt};
use serde_json::{json, Value};
use std::error::Error;

const CORPUS: &str = include_str!("../../../examples/rpc-v1/conformance.json");

fn evaluate(entry: &Value) -> Result<Value, Box<dyn Error>> {
    let name = entry["name"].as_str().ok_or("missing fixture name")?;
    let kind = entry["kind"].as_str().ok_or("missing fixture kind")?;
    let encoded = entry["encoded"].as_str().ok_or("missing fixture bytes")?;
    // Only a decoder's own error is negative evidence. Encoding, metadata,
    // serialization, or process failures abort the oracle instead.
    let decoded: Option<Value> = match kind {
        "call" => match decode_rpc_v1_call(encoded.as_bytes()) {
            Ok(call) => Some(serde_json::from_slice(&call.encode()?)?),
            Err(_) => None,
        },
        "receipt" => match decode_rpc_v1_receipt(encoded.as_bytes()) {
            Ok(receipt) => Some(serde_json::from_slice(&receipt.encode()?)?),
            Err(_) => None,
        },
        _ => return Err("unsupported fixture kind".into()),
    };
    Ok(match decoded {
        Some(value) => json!({"name": name, "kind": kind, "accepted": true, "decoded": value}),
        None => json!({"name": name, "kind": kind, "accepted": false}),
    })
}

fn report(corpus: &Value) -> Result<Value, Box<dyn Error>> {
    let mut results = Vec::new();
    for group in ["valid", "invalid"] {
        let entries = corpus[group].as_array().ok_or("missing corpus group")?;
        if entries.is_empty() {
            return Err("empty corpus group".into());
        }
        for entry in entries {
            results.push(evaluate(entry)?);
        }
    }
    Ok(json!({"schema": "ores.api-docs.rust-rpc-admission/v1", "results": results}))
}

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::args_os().len() != 1 {
        return Err("this fixed admission oracle accepts no arguments".into());
    }
    let corpus: Value = serde_json::from_str(CORPUS)?;
    println!("{}", serde_json::to_string(&report(&corpus)?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, encoded: &str) -> Value {
        json!({"name": "probe", "kind": kind, "encoded": encoded})
    }

    #[test]
    fn round_trip_preserves_absent_and_explicit_null_bodies() {
        for encoded in [
            r#"{"v":1,"op":"call","id":"c1","key":"get_item"}"#,
            r#"{"v":1,"op":"call","id":"c1","key":"get_item","body":null}"#,
        ] {
            let result = evaluate(&row("call", encoded)).unwrap();
            assert_eq!(result["accepted"], true);
            assert_eq!(
                result["decoded"],
                serde_json::from_str::<Value>(encoded).unwrap()
            );
        }
    }

    #[test]
    fn receipts_use_the_real_client_decoder_and_encoder() {
        let encoded =
            r#"{"v":1,"op":"receipt","id":"c1","key":"get_item","ok":true,"body":{"id":"item"}}"#;
        let result = evaluate(&row("receipt", encoded)).unwrap();
        assert_eq!(result["accepted"], true);
        assert_eq!(
            result["decoded"],
            serde_json::from_str::<Value>(encoded).unwrap()
        );
    }

    #[test]
    fn decoder_rejections_have_no_fabricated_decoded_value() {
        for encoded in [
            r#"{"v":2,"op":"call","id":"c1","key":"get_item"}"#,
            r#"{"v":1,"op":"call","id":"c1","id":"c2","key":"get_item"}"#,
            "not JSON",
        ] {
            let result = evaluate(&row("call", encoded)).unwrap();
            assert_eq!(result["accepted"], false);
            assert!(result.get("decoded").is_none());
        }
    }

    #[test]
    fn metadata_failures_abort_instead_of_counting_as_rejections() {
        assert!(evaluate(&row("unknown", "{}")).is_err());
        assert!(evaluate(&json!({"name": "probe", "kind": "call"})).is_err());
        assert!(report(&json!({"valid": [], "invalid": []})).is_err());
        assert!(report(&json!({})).is_err());
    }

    #[test]
    fn every_committed_fixture_agrees_with_its_expectation() {
        let corpus: Value = serde_json::from_str(CORPUS).unwrap();
        for (group, expected) in [("valid", true), ("invalid", false)] {
            for entry in corpus[group].as_array().unwrap() {
                assert_eq!(
                    evaluate(entry).unwrap()["accepted"],
                    expected,
                    "{}",
                    entry["name"]
                );
            }
        }
    }
}
