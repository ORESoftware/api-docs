use ores_api_docs::{
    rpc_client_bundle_v3, RouteMap, RpcClientAudience, RpcClientBundleV3, RpcCodecSet,
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
            repository: None,
            commit_sha: None,
        },
        http: RpcHttpProjection {
            method: "GET".to_owned(),
            path: "/v1/version".to_owned(),
            rpc_transport_path: "/v1/rpc",
        },
        scope: RpcOperationScope::Regular,
        stream: ores_api_docs::RpcStreamMode::Unary,
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
                    "version": { "type": "string" }
                },
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

fn bundle() -> RpcClientBundleV3 {
    rpc_client_bundle_v3(&sample_map(), &[sample_operation()], "crate::dto", "public")
        .expect("typed v3 RPC bundle")
}

#[test]
fn task_01_manifest_uses_canonical_rpc_endpoint_even_for_get_projection() {
    assert_eq!(bundle().manifest.http_endpoint, "/v1/rpc");
}

#[test]
fn task_02_http_projection_metadata_may_remain_get_without_becoming_rpc_transport() {
    let operation = sample_operation();
    assert_eq!(operation.http.method, "GET");
    assert_eq!(operation.http.path, "/v1/version");
    assert_eq!(operation.http.rpc_transport_path, "/v1/rpc");
}

#[test]
fn task_03_typescript_named_operation_only_prepares_a_builder() {
    let source = &bundle().operations[0].typescript;
    assert!(source.contains("RpcCallBuilder<GetVersionResponse"));
    assert!(source.contains("return client.prepare<GetVersionResponse"));
    assert!(!source.contains("fetchImpl("));
}

#[test]
fn task_04_typescript_fetch_occurs_only_inside_make_call_and_is_post() {
    let source = include_str!("../../clients/typescript/src/fluent-rpc.js");
    assert_eq!(source.matches("this.fetchImpl(").count(), 1);
    let make_call = source.find("async makeCall()").expect("makeCall");
    let fetch = source.find("this.fetchImpl(").expect("fetch");
    let end = source[make_call..]
        .find("async makeCallOrThrow()")
        .map(|offset| make_call + offset)
        .expect("makeCallOrThrow");
    assert!(make_call < fetch && fetch < end);
    assert!(!source[..make_call].contains("this.fetchImpl("));
    assert!(source[make_call..end].contains("method: \"POST\""));
}

#[test]
fn task_05_dart_named_operation_only_prepares_a_builder() {
    let source = &bundle().operations[0].dart;
    assert!(source.contains("RpcCallBuilder<GetVersionResponse>"));
    assert!(source.contains("return client.prepare<GetVersionResponse>"));
    assert!(!source.contains("executeCall<"));
}

#[test]
fn task_06_dart_make_call_is_the_execution_boundary_for_rpc_path() {
    let source = &bundle().transport.dart;
    let make_call = source
        .find("Future<RpcOutcome<T>> makeCall()")
        .expect("Dart makeCall");
    let execute = source[make_call..]
        .find("client.executeCall<T>(")
        .map(|offset| make_call + offset)
        .expect("Dart executeCall");
    assert!(make_call < execute);
    assert!(source.contains("baseUri.resolve(rpcHttpPath)"));
    assert!(source.contains("const rpcHttpPath = \"/v1/rpc\""));
}

#[test]
fn task_07_go_named_operation_constructs_call_without_transport_io() {
    let source = &bundle().operations[0].go;
    let constructor = source.find("func GetVersion(").expect("Go constructor");
    let make_call = source
        .find("func (c *GetVersionCall) MakeCall(")
        .expect("Go MakeCall");
    assert!(constructor < make_call);
    assert!(!source[constructor..make_call].contains("CallJSONOutcome("));
}

#[test]
fn task_08_go_make_call_performs_the_typed_transport_dispatch() {
    let source = &bundle().operations[0].go;
    let make_call = source
        .find("func (c *GetVersionCall) MakeCall(")
        .expect("Go MakeCall");
    assert!(source[make_call..].contains("CallJSONOutcome("));
}

#[test]
fn task_09_go_transport_uses_post_and_canonical_rpc_path() {
    let source = &bundle().transport.go;
    assert!(source.contains("const HTTPPath = \"/v1/rpc\""));
    assert!(source.contains("http.NewRequestWithContext(ctx, http.MethodPost"));
    assert!(!source.contains("http.MethodGet"));
}

#[test]
fn task_10_rust_named_operation_only_returns_typed_builder() {
    let source = &bundle().operations[0].rust;
    assert!(source.contains("TypedRpcCallBuilder"));
    assert!(source.contains("TypedRpcCallBuilder::new"));
    assert!(!source.contains("send_plain"));
}

#[test]
fn task_11_rust_make_call_delegates_before_transport_send() {
    let source = &bundle().transport.rust;
    let make_call = source
        .find("pub async fn make_call(self)")
        .expect("Rust make_call");
    let delegate = source[make_call..]
        .find("call_typed_outcome::<B, E>")
        .map(|offset| make_call + offset)
        .expect("Rust typed outcome delegate");
    let send = source.find("send_plain(&request)").expect("Rust send");
    assert!(make_call < delegate);
    assert!(delegate < send);
}

#[test]
fn task_12_rust_transport_is_post_to_canonical_rpc_path() {
    let source = &bundle().transport.rust;
    assert!(source.contains("TYPED_RPC_HTTP_PATH: &str = \"/v1/rpc\""));
    assert!(source.contains("HttpMethod::Post, TYPED_RPC_HTTP_PATH"));
}

#[test]
fn task_13_gleam_named_operation_only_prepares_typed_call() {
    let source = &bundle().operations[0].gleam;
    assert!(source.contains("-> TypedCall(GetVersionResponse)"));
    assert!(source.contains("prepare(transport, base_url, id"));
    assert!(!source.contains("send_transport("));
}

#[test]
fn task_14_gleam_make_call_is_the_dispatch_boundary() {
    let source = &bundle().transport.gleam;
    let make_call = source
        .find("pub fn make_call(call: TypedCall(a))")
        .expect("Gleam make_call");
    assert!(source[make_call..].contains("send_raw(builder)"));
}

#[test]
fn task_15_gleam_dispatch_uses_only_canonical_rpc_path() {
    let source = &bundle().transport.gleam;
    assert!(source.contains("pub const rpc_http_path = \"/v1/rpc\""));
    assert!(source.contains("send_transport(base_url <> rpc_http_path"));
    assert!(!source.contains("/v1/version"));
}
