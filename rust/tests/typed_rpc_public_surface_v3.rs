use ores_api_docs::{
    rpc_client_bundle_v2, rpc_client_bundle_v3, RouteMap, RpcClientAudience, RpcCodecSet,
    RpcHttpProjection, RpcOperationContract, RpcOperationScope, RpcOperationSource,
    RpcPayloadCodec, RpcRequestShape, RpcResponseShape,
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
fn v2_named_public_methods_are_schema_typed_in_all_five_languages() {
    let bundle = rpc_client_bundle_v2(&sample_map(), &[sample_operation()], "crate::dto", "public")
        .expect("typed bundle generation");

    assert!(bundle.rust.contains(
        "pub fn get_version(&self, input: GetVersionInput) -> TypedRpcCallBuilder<'_, GetVersionResponse"
    ));
    assert!(bundle.go.contains(
        "func (c *Client) GetVersion(input GetVersionInput) *TypedCall[GetVersionResponse]"
    ));
    assert!(bundle
        .dart
        .contains("RpcCallBuilder<GetVersionResponse> getVersion(GetVersionInput input)"));
    assert!(bundle
        .typescript
        .contains("getVersion(input: GetVersionInput): RpcCallBuilder<GetVersionResponse"));
    assert!(bundle.gleam.contains(
        "pub fn get_version(transport: Transport, base_url: String, id: String, input: GetVersionInput) -> TypedCall(GetVersionResponse)"
    ));
    assert!(bundle
        .typescript
        .contains("@oresoftware/api-docs/fluent-rpc"));
    assert!(bundle
        .typescript
        .contains("getVersion(input: GetVersionInput): RpcCallBuilder"));
    assert!(bundle.dart.contains("Future<RpcOutcome<T>> makeCall()"));
    assert!(bundle
        .go
        .contains("func (b *TypedCall[T]) MakeCall(ctx context.Context)"));
    assert!(bundle.rust.contains("pub async fn make_call(self)"));
    assert!(bundle
        .gleam
        .contains("pub fn make_call(call: TypedCall(a))"));

    assert!(!bundle
        .typescript
        .contains("async getVersion(input: GetVersionInput): Promise<unknown>"));
    assert!(!bundle.dart.contains("Future<Object?> getVersion"));
    assert!(!bundle
        .go
        .contains("GetVersion(ctx context.Context, input GetVersionInput, out any)"));
    assert!(!bundle.gleam.contains(
        "pub fn get_version(transport: Transport, base_url: String, id: String, input: GetVersionInput) -> Result(dynamic.Dynamic, String)"
    ));
}

#[test]
fn v3_namespace_operation_units_stay_typed_and_preserve_wire_keys() {
    let bundle = rpc_client_bundle_v3(&sample_map(), &[sample_operation()], "crate::dto", "public")
        .expect("structured typed bundle");
    let operation = bundle.operations.first().expect("operation unit");

    assert_eq!(bundle.manifest.http_endpoint, "/v1/rpc");
    assert_eq!(bundle.manifest.layout, "namespace-files/v1");
    assert_eq!(bundle.manifest.operations, ["demo.version.get_version"]);
    assert_eq!(operation.namespace, ["version"]);
    assert_eq!(operation.operation_name, "get_version");

    assert!(operation.rust.contains("GetVersionResponse"));
    assert!(operation.rust.contains("#[serde(rename = \"traceIds\")]"));
    assert!(operation.go.contains("GetVersionResponse"));
    assert!(operation.go.contains("json:\"traceIds\""));
    assert!(operation.dart.contains("GetVersionResponse"));
    assert!(operation.dart.contains("traceIds"));
    assert!(operation
        .typescript
        .contains("RpcCallBuilder<GetVersionResponse"));
    assert!(operation.typescript.contains("\"traceIds\""));
    assert!(operation.gleam.contains("TypedCall(GetVersionResponse)"));
    assert!(operation.gleam.contains("\"traceIds\""));

    for source in [
        &operation.rust,
        &operation.go,
        &operation.dart,
        &operation.typescript,
        &operation.gleam,
    ] {
        assert!(!source.contains("/rpc/v1"));
    }
}
