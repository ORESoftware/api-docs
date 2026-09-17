use ores_api_docs::{
    rpc_client_bundle_v2, RouteMap, RpcClientAudience, RpcCodecSet, RpcHttpProjection,
    RpcOperationContract, RpcOperationScope, RpcOperationSource, RpcPayloadCodec, RpcRequestShape,
    RpcResponseShape,
};
use serde_json::json;

fn sample_map() -> RouteMap {
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
    .expect("sample route map")
}

fn sample_operation() -> RpcOperationContract {
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
        request: RpcRequestShape::default(),
        response: RpcResponseShape {
            header_schema: None,
            trailer_schema: None,
            body_schema: Some(json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "result": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "service": {"type": "string"},
                            "version": {"type": "string"}
                        },
                        "required": ["service", "version"]
                    },
                    "traceIds": {
                        "type": "array",
                        "items": {"type": "string"}
                    }
                },
                "required": ["result", "traceIds"]
            })),
            error_schema: Some(json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "code": {"type": "string"},
                    "message": {"type": "string"}
                },
                "required": ["code", "message"]
            })),
        },
        contract_sha256: "0".repeat(64),
    }
}

#[test]
fn named_public_methods_are_schema_typed_in_all_five_languages() {
    let bundle = rpc_client_bundle_v2(&sample_map(), &[sample_operation()], "crate::dto", "public")
        .expect("typed bundle generation");

    assert!(bundle.rust.contains(
        "pub async fn get_version(&self, input: GetVersionInput) -> Result<GetVersionResponse"
    ));
    assert!(bundle.go.contains(
        "func (c *Client) GetVersion(ctx context.Context, input GetVersionInput) (GetVersionResponse, error)"
    ));
    assert!(bundle
        .dart
        .contains("Future<GetVersionResponse> getVersion(GetVersionInput input)"));
    assert!(bundle
        .typescript
        .contains("async getVersion(input: GetVersionInput): Promise<GetVersionResponse>"));
    assert!(bundle.gleam.contains(
        "pub fn get_version(transport: Transport, base_url: String, id: String, input: GetVersionInput) -> Result(GetVersionResponse, String)"
    ));

    assert!(!bundle
        .typescript
        .contains("async getVersion(input: GetVersionInput): Promise<unknown>"));
    assert!(!bundle
        .dart
        .contains("Future<Object?> getVersion(GetVersionInput input)"));
    assert!(!bundle.go.contains(
        "func (c *Client) GetVersion(ctx context.Context, input GetVersionInput, out any)"
    ));
    assert!(!bundle.gleam.contains(
        "pub fn get_version(transport: Transport, base_url: String, id: String, input: GetVersionInput) -> Result(dynamic.Dynamic, String)"
    ));
}

#[test]
fn typed_response_preserves_wire_keys_and_canonical_rpc_endpoint() {
    let bundle = rpc_client_bundle_v2(&sample_map(), &[sample_operation()], "crate::dto", "public")
        .expect("typed bundle generation");

    assert_eq!(bundle.manifest.http_endpoint, "/v1/rpc");
    assert_eq!(bundle.manifest.operations, ["demo.version.get_version"]);

    assert!(bundle.rust.contains("#[serde(rename = \"traceIds\")]"));
    assert!(bundle.go.contains("`json:\"traceIds\"`"));
    assert!(bundle.dart.contains("traceIds"));
    assert!(bundle.typescript.contains("\"traceIds\""));
    assert!(bundle.gleam.contains("\"traceIds\""));

    for source in [
        &bundle.rust,
        &bundle.go,
        &bundle.dart,
        &bundle.typescript,
        &bundle.gleam,
    ] {
        assert!(!source.contains("\"/rpc/v1\""));
    }
}
