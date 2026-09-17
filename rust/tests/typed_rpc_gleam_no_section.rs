use ores_api_docs::{
    rpc_client_bundle_v2, RouteMap, RpcClientAudience, RpcCodecSet, RpcHttpProjection,
    RpcOperationContract, RpcOperationScope, RpcOperationSource, RpcPayloadCodec, RpcRequestShape,
    RpcResponseShape,
};
use serde_json::json;

fn map() -> RouteMap {
    RouteMap::from_json_str(
        r#"{
          "schema_version":"1.0.0",
          "service":"demo-api",
          "map":{
            "demo.version.get_version":{
              "path":"/v1/version",
              "methods":["GET"],
              "rpc_key":"demo.version.get_version",
              "transports":["http"]
            }
          }
        }"#,
    )
    .expect("route map")
}

fn operation() -> RpcOperationContract {
    RpcOperationContract {
        schema_version: 2,
        operation_key: "demo.version.get_version".to_owned(),
        namespace: vec!["demo".to_owned(), "version".to_owned()],
        source: RpcOperationSource {
            route_file: "src/routes/version/route.rs".to_owned(),
            handler: "get".to_owned(),
            operation: Some("get_version".to_owned()),
            invoker: Some("__ores_invoke_get_version".to_owned()),
            execution_model: "shared_operation".to_owned(),
            repository: Some("https://github.com/example/demo-api".to_owned()),
            commit_sha: Some("1".repeat(40)),
        },
        http: RpcHttpProjection {
            method: "GET".to_owned(),
            path: "/v1/version".to_owned(),
            rpc_transport_path: "/v1/rpc",
        },
        scope: RpcOperationScope::Regular,
        audiences: vec![RpcClientAudience::Browser, RpcClientAudience::Server],
        codecs: RpcCodecSet {
            allowed: vec![RpcPayloadCodec::Json],
            default: RpcPayloadCodec::Json,
        },
        // Every request section is NoSection in the authoritative OperationSpec,
        // so the normalized contract contains no semantic request schema.
        request: RpcRequestShape::default(),
        response: RpcResponseShape {
            header_schema: None,
            trailer_schema: None,
            body_schema: Some(json!({
                "type": "object",
                "additionalProperties": false,
                "properties": { "version": { "type": "string" } },
                "required": ["version"]
            })),
            error_schema: Some(json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "code": { "type": "string" },
                    "message": { "type": "string" }
                },
                "required": ["code", "message"]
            })),
        },
        contract_sha256: "0".repeat(64),
    }
}

#[test]
fn gleam_no_section_operation_never_references_fields_that_were_not_generated() {
    let bundle = rpc_client_bundle_v2(&map(), &[operation()], "crate::dto", "public")
        .expect("typed RPC bundle");

    assert!(bundle.gleam.contains("pub type GetVersionInput"));
    assert!(bundle.gleam.contains("trace_id: option.Option(String)"));
    assert!(bundle.gleam.contains("span_id: option.Option(String)"));

    for nonexistent in [
        "path_json:",
        "query_json:",
        "headers_json:",
        "body_json:",
        "input.path_json",
        "input.query_json",
        "input.headers_json",
        "input.body_json",
    ] {
        assert!(
            !bundle.gleam.contains(nonexistent),
            "NoSection operation exposed or referenced nonexistent generated field {nonexistent}"
        );
    }

    assert!(bundle.gleam.contains(
        "let args = CallArgs(option.None, option.None, option.None, option.None, input.trace_id, input.span_id)"
    ));
}
