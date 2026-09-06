mod tests {
    use super::*;

    fn document() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/rpc-v1/conformance.json"
        )))
        .expect("fixture JSON")
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn shared_fixtures_round_trip_and_invalid_cases_fail_closed() {
        let document = document();
        for case in document["valid"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let source = case["encoded"].as_str().unwrap();
            let (encoded, framed) = match case["kind"].as_str().unwrap() {
                "call" => {
                    let value = decode_rpc_v1_call(source.as_bytes())
                        .unwrap_or_else(|error| panic!("{name}: {error}"));
                    (value.encode().unwrap(), value.to_length_prefixed().unwrap())
                }
                "receipt" => {
                    let value = decode_rpc_v1_receipt(source.as_bytes())
                        .unwrap_or_else(|error| panic!("{name}: {error}"));
                    (value.encode().unwrap(), value.to_length_prefixed().unwrap())
                }
                other => panic!("unknown kind {other}"),
            };
            assert_eq!(String::from_utf8(encoded).unwrap(), source, "{name}");
            assert_eq!(
                hex(&framed[..4]),
                case["tcp_prefix_hex"].as_str().unwrap(),
                "{name}"
            );
        }
        for case in document["invalid"].as_array().unwrap() {
            let source = case["encoded"].as_str().unwrap();
            let rejected = match case["kind"].as_str().unwrap() {
                "call" => decode_rpc_v1_call(source.as_bytes()).is_err(),
                "receipt" => decode_rpc_v1_receipt(source.as_bytes()).is_err(),
                other => panic!("unknown kind {other}"),
            };
            assert!(rejected, "accepted {}", case["name"]);
        }
    }

    #[test]
    fn null_body_correlation_and_framing_boundaries_are_preserved() {
        let absent =
            decode_rpc_v1_receipt(br#"{"v":1,"op":"receipt","id":"c1","key":"healthz","ok":true}"#)
                .unwrap();
        let present = decode_rpc_v1_receipt(
            br#"{"v":1,"op":"receipt","id":"c1","key":"healthz","ok":true,"body":null}"#,
        )
        .unwrap();
        assert!(!absent.body.is_present());
        assert_eq!(present.body.value(), Some(&Value::Null));

        let mut call = RpcV1Call::new("c1", "healthz");
        call.transport = Some(Transport::Tcp);
        let mut receipt = RpcV1Receipt::success("c2", "healthz", OptionalJson::absent());
        receipt.transport = Some(Transport::Tcp);
        assert!(assert_rpc_v1_receipt_for_call(&call, &receipt).is_err());

        let line = call.to_ndjson().unwrap();
        assert_eq!(rpc_v1_call_from_ndjson(line.as_bytes()).unwrap(), call);
        assert!(rpc_v1_call_from_ndjson(&[line.as_bytes(), b"{}\n"].concat()).is_err());

        let framed = call.to_length_prefixed().unwrap();
        let partial = [framed.as_slice(), &[0, 0, 0]].concat();
        let (frames, rest) = split_rpc_v1_length_prefixed(&partial).unwrap();
        assert_eq!((frames.len(), rest.len()), (1, 3));
        assert!(split_rpc_v1_length_prefixed(&u32::MAX.to_be_bytes()).is_err());
    }

    #[test]
    fn correlation_ids_are_monotonic_and_bounded() {
        let mut value = RpcV1Correlator::new("request-").unwrap();
        assert_eq!(
            [value.take().unwrap(), value.take().unwrap()],
            ["request-1", "request-2"]
        );
        assert!(RpcV1Correlator::new("x".repeat(128)).is_err());
    }
}
