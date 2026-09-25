use ores_api_docs::{
    rpc_client_bundle_v2, rpc_client_bundle_v3, RouteMap, RpcClientAudience, RpcCodecSet,
    RpcHttpProjection, RpcOperationContract, RpcOperationScope, RpcOperationSource,
    RpcPayloadCodec, RpcRequestShape, RpcResponseShape,
};
use serde_json::{json, Value};

const PATH: u8 = 1;
const QUERY: u8 = 2;
const HEADERS: u8 = 4;
const BODY: u8 = 8;
const ALL: u8 = PATH | QUERY | HEADERS | BODY;

fn map() -> RouteMap {
    RouteMap::from_json_str(
        r#"{
          "schema_version":"1.0.0",
          "service":"demo-api",
          "map":{
            "demo.version.get_version":{
              "path":"/v1/version",
              "methods":["POST"],
              "rpc_key":"demo.version.get_version",
              "transports":["http"]
            }
          }
        }"#,
    )
    .expect("route map")
}

fn section_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "value": { "type": "string" }
        },
        "required": ["value"]
    })
}

fn operation(mask: u8) -> RpcOperationContract {
    RpcOperationContract {
        schema_version: ores_api_docs::RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
        operation_key: "demo.version.get_version".to_owned(),
        namespace: vec!["demo".to_owned(), "version".to_owned()],
        source: RpcOperationSource {
            route_file: Some("src/routes/version/route.rs".to_owned()),
            handlers_file: None,
            http_handler: Some("post".to_owned()),
            operation: Some("get_version".to_owned()),
            invoker: Some("__ores_invoke_get_version".to_owned()),
            execution_model: "shared_operation".to_owned(),
            repository: None,
            commit_sha: None,
        },
        rpc_transport_path: "/v1/rpc",
        http: Some(RpcHttpProjection {
            method: "POST".to_owned(),
            path: "/v1/version".to_owned(),
        }),
        scope: RpcOperationScope::Regular,
        stream: ores_api_docs::RpcStreamMode::Unary,
        audiences: vec![RpcClientAudience::Browser, RpcClientAudience::Server],
        codecs: RpcCodecSet {
            allowed: vec![RpcPayloadCodec::Json],
            default: RpcPayloadCodec::Json,
        },
        request: RpcRequestShape {
            path_schema: (mask & PATH != 0).then(section_schema),
            query_schema: (mask & QUERY != 0).then(section_schema),
            header_schema: (mask & HEADERS != 0).then(section_schema),
            body_schema: (mask & BODY != 0).then(section_schema),
        },
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

fn v2(mask: u8) -> ores_api_docs::RpcClientBundleV2 {
    rpc_client_bundle_v2(&map(), &[operation(mask)], "crate::dto", "public")
        .expect("typed v2 bundle")
}

#[test]
fn task_01_go_no_section_projects_nil_path() {
    assert!(v2(0).go.contains("CallArgs{Path: nil,"));
}

#[test]
fn task_02_go_no_section_projects_nil_query() {
    assert!(v2(0).go.contains("Query: nil,"));
}

#[test]
fn task_03_go_no_section_projects_nil_headers() {
    assert!(v2(0).go.contains("Headers: nil,"));
}

#[test]
fn task_04_go_no_section_projects_nil_body() {
    assert!(v2(0).go.contains("Body: nil,"));
}

#[test]
fn task_05_go_no_section_never_references_phantom_input_fields() {
    let go = v2(0).go;
    for phantom in ["input.Path", "input.Query", "input.Headers", "input.Body"] {
        assert!(
            !go.contains(phantom),
            "NoSection Go facade referenced phantom field {phantom}"
        );
    }
}

#[test]
fn task_06_go_path_only_projects_only_path() {
    let go = v2(PATH).go;
    assert!(go.contains("CallArgs{Path: toMap(input.Path), Query: nil, Headers: nil, Body: nil,"));
}

#[test]
fn task_07_go_query_only_projects_only_query() {
    let go = v2(QUERY).go;
    assert!(go.contains("CallArgs{Path: nil, Query: toMap(input.Query), Headers: nil, Body: nil,"));
}

#[test]
fn task_08_go_headers_only_projects_only_headers() {
    let go = v2(HEADERS).go;
    assert!(
        go.contains("CallArgs{Path: nil, Query: nil, Headers: toMap(input.Headers), Body: nil,")
    );
}

#[test]
fn task_09_go_body_only_projects_only_body() {
    let go = v2(BODY).go;
    assert!(go.contains("CallArgs{Path: nil, Query: nil, Headers: nil, Body: input.Body,"));
}

#[test]
fn task_10_go_all_sections_project_all_typed_fields() {
    let go = v2(ALL).go;
    assert!(go.contains(
        "CallArgs{Path: toMap(input.Path), Query: toMap(input.Query), Headers: toMap(input.Headers), Body: input.Body,"
    ));
}

#[test]
fn task_11_rust_no_section_input_and_envelope_omit_semantic_fields() {
    let rust = v2(0).rust;
    let start = rust
        .find("pub struct GetVersionInput {")
        .expect("Rust input");
    let end = rust[start..]
        .find("}\n")
        .map(|offset| start + offset + 2)
        .expect("Rust input end");
    let input = &rust[start..end];
    assert!(input.contains("pub trace_id: Option<String>"));
    assert!(input.contains("pub span_id: Option<String>"));
    for phantom in ["pub path:", "pub query:", "pub headers:", "pub body:"] {
        assert!(!input.contains(phantom), "Rust input exposed {phantom}");
    }
    // Scope the projection assertion to the generated operation method.
    // The shared fluent builder intentionally contains add_* mutation helpers
    // for callers that opt into them; their presence must not be confused with
    // the operation generator eagerly projecting an absent semantic section.
    let method_start = rust
        .find("pub fn get_version(&self, input: GetVersionInput)")
        .expect("Rust get_version method");
    let method_tail = &rust[method_start..];
    let method_end = method_tail
        .find("TypedRpcCallBuilder::new")
        .expect("Rust get_version builder construction");
    let method = &method_tail[..method_end];
    for projection in [
        "envelope[\"path\"]",
        "envelope[\"query\"]",
        "envelope[\"headers\"]",
        "envelope[\"body\"]",
    ] {
        assert!(
            !method.contains(projection),
            "Rust NoSection operation emitted {projection}"
        );
    }
}

#[test]
fn task_12_typescript_no_section_surface_stays_typed_and_minimal() {
    let typescript = v2(0).typescript;
    assert!(typescript.contains(
        "export interface GetVersionInput {\n  traceId?: string;\n  spanId?: string;\n}"
    ));
    assert!(typescript
        .contains("getVersion(input: GetVersionInput): RpcCallBuilder<GetVersionResponse"));
    for phantom in ["  path:", "  query:", "  headers:", "  body:"] {
        assert!(
            !typescript
                .split("export interface GetVersionInput {")
                .nth(1)
                .and_then(|tail| tail.split('}').next())
                .unwrap_or_default()
                .contains(phantom),
            "TypeScript NoSection input exposed {phantom}"
        );
    }
}

#[test]
fn task_13_dart_no_section_surface_uses_null_projections_and_typed_result() {
    let dart = v2(0).dart;
    assert!(dart.contains("Map<String, Object?>? get pathJson => null;"));
    assert!(dart.contains("Map<String, Object?>? get queryJson => null;"));
    assert!(dart.contains("Map<String, Object?>? get headersJson => null;"));
    assert!(dart.contains("Object? get bodyJson => null;"));
    assert!(dart.contains("RpcCallBuilder<GetVersionResponse> getVersion(GetVersionInput input)"));
}

#[test]
fn task_14_gleam_no_section_surface_uses_none_without_phantom_fields() {
    let gleam = v2(0).gleam;
    assert!(gleam.contains(
        "let args = CallArgs([], [], [], option.None, [], input.trace_id, input.span_id)"
    ));
    for phantom in [
        "input.path_json",
        "input.query_json",
        "input.headers_json",
        "input.body_json",
    ] {
        assert!(
            !gleam.contains(phantom),
            "Gleam NoSection emitted {phantom}"
        );
    }
    assert!(gleam.contains("-> TypedCall(GetVersionResponse)"));
}

#[test]
fn task_15_v3_operation_modules_stay_typed_and_no_section_safe_in_all_languages() {
    let bundle = rpc_client_bundle_v3(&map(), &[operation(0)], "crate::dto", "public")
        .expect("typed v3 bundle");
    let generated = bundle.operations.first().expect("operation source");

    assert!(generated.rust.contains("GetVersionResponse"));
    assert!(!generated
        .rust
        .contains("type Response = ::serde_json::Value;"));

    assert!(generated.go.contains("CallJSONOutcome("));
    assert!(generated
        .go
        .contains("path: nil, query: nil, headers: nil, body: nil"));
    for phantom in ["input.Path", "input.Query", "input.Headers", "input.Body"] {
        assert!(
            !generated.go.contains(phantom),
            "v3 Go NoSection emitted phantom field {phantom}"
        );
    }
    assert!(!generated.go.contains("out any"));

    assert!(generated
        .dart
        .contains("RpcCallBuilder<GetVersionResponse>"));
    assert!(!generated.dart.contains("Future<Object?>"));

    assert!(generated
        .typescript
        .contains("RpcCallBuilder<GetVersionResponse"));
    assert!(!generated.typescript.contains("RpcCallArgs"));
    assert!(!generated.typescript.contains("Promise<unknown>"));

    assert!(generated.gleam.contains("TypedCall(GetVersionResponse)"));
    assert!(!generated.gleam.contains("Result(dynamic.Dynamic, String)"));
    for phantom in [
        "input.path_json",
        "input.query_json",
        "input.headers_json",
        "input.body_json",
    ] {
        assert!(
            !generated.gleam.contains(phantom),
            "v3 Gleam emitted {phantom}"
        );
    }
}
