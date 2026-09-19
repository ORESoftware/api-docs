use ores_api_docs::{
    rpc_client_bundle_v3, RouteMap, RpcClientAudience, RpcCodecSet, RpcHttpProjection,
    RpcOperationContract, RpcOperationScope, RpcOperationSource, RpcPayloadCodec, RpcRequestShape,
    RpcResponseShape, RpcStreamMode,
};
use serde_json::json;

fn route_map(key: &str) -> RouteMap {
    RouteMap::from_json_str(
        &serde_json::to_string(&json!({
            "schema_version": "1.0.0",
            "service": "demo-api",
            "map": {
                (key): {
                    "path": "/v1/events/stream",
                    "methods": ["GET"],
                    "rpc_key": key,
                    "transports": ["websocket", "tcp"]
                }
            }
        }))
        .expect("route map json"),
    )
    .expect("route map")
}

fn operation(mode: RpcStreamMode) -> RpcOperationContract {
    RpcOperationContract {
        schema_version: ores_api_docs::RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
        operation_key: "demo.events.watch_events_stream".to_owned(),
        namespace: vec!["demo".to_owned(), "events".to_owned()],
        source: RpcOperationSource {
            route_file: Some("src/routes/events/stream/route.rs".to_owned()),
            handlers_file: None,
            handler: "get".to_owned(),
            operation: Some("watch_events_stream".to_owned()),
            invoker: Some("__ores_invoke_watch_events_stream".to_owned()),
            execution_model: "shared_operation".to_owned(),
            repository: None,
            commit_sha: None,
        },
        rpc_transport_path: "/v1/rpc",
        http: Some(RpcHttpProjection {
            method: "GET".to_owned(),
            path: "/v1/events/stream".to_owned(),
        }),
        scope: RpcOperationScope::Regular,
        stream: mode,
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
                    "event_id": {"type": "string"},
                    "kind": {"type": "string"}
                },
                "required": ["event_id", "kind"]
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

fn server_stream() -> ores_api_docs::RpcClientBundleV3 {
    rpc_client_bundle_v3(
        &route_map("demo.events.watch_events_stream"),
        &[operation(RpcStreamMode::ServerStream)],
        "crate::dto",
        "public",
    )
    .expect("server stream v3 bundle")
}

#[test]
fn task_01_v3_accepts_handlers_authoritative_server_stream() {
    let bundle = server_stream();
    assert_eq!(bundle.operations.len(), 1);
    assert_eq!(
        bundle.operations[0].operation_key,
        "demo.events.watch_events_stream"
    );
}

#[test]
fn task_02_v3_operation_unit_preserves_server_stream_mode() {
    let bundle = server_stream();
    assert_eq!(bundle.operations[0].stream, RpcStreamMode::ServerStream);
}

#[test]
fn task_03_typescript_named_stream_uses_central_stream_builder() {
    let source = &server_stream().operations[0].typescript;
    assert!(source.contains("@oresoftware/api-docs/stream-rpc"));
    assert!(source.contains(
        "watchEventsStream(client: OresRpcStreamClient<\"demo.events.watch_events_stream\">): RpcStreamCallBuilder<WatchEventsStreamResponse>"
    ));
}

#[test]
fn task_04_dart_named_stream_uses_central_stream_builder() {
    let source = &server_stream().operations[0].dart;
    assert!(source.contains("package:ores_api_docs/ores_api_docs.dart"));
    assert!(source.contains(
        "RpcStreamCallBuilder<WatchEventsStreamResponse> watchEventsStream(OresRpcStreamClient client)"
    ));
}

#[test]
fn task_05_go_named_stream_uses_central_stream_builder() {
    let source = &server_stream().operations[0].go;
    assert!(source.contains("github.com/oresoftware/api-docs/clients/go"));
    assert!(source.contains(
        "func WatchEventsStream(client *oresapidocs.OresRPCStreamClient) (*oresapidocs.RPCStreamCallBuilder[WatchEventsStreamResponse], error)"
    ));
}

#[test]
fn task_06_rust_named_stream_uses_central_stream_builder() {
    let source = &server_stream().operations[0].rust;
    assert!(source.contains("::ores_api_docs_client::OresRpcStreamClient"));
    assert!(source.contains("::ores_api_docs_client::RpcStreamCallBuilder"));
    assert!(source.contains("WatchEventsStreamResponse"));
}

#[test]
fn task_07_gleam_named_stream_uses_central_stream_builder() {
    let source = &server_stream().operations[0].gleam;
    assert!(source.contains("import ores_api_docs/stream_rpc"));
    assert!(source.contains(
        "pub fn watch_events_stream(transport: stream_rpc.Transport(WatchEventsStreamResponse), id: String) -> stream_rpc.StreamBuilder(WatchEventsStreamResponse)"
    ));
}

#[test]
fn task_08_generated_stream_facades_preserve_exact_operation_key() {
    let operation = &server_stream().operations[0];
    for source in [
        &operation.rust,
        &operation.go,
        &operation.dart,
        &operation.typescript,
        &operation.gleam,
    ] {
        assert!(source.contains("demo.events.watch_events_stream"));
    }
}

#[test]
fn task_09_generated_stream_facades_use_canonical_rpc_transport_not_http_projection() {
    let operation = &server_stream().operations[0];
    for source in [
        &operation.rust,
        &operation.go,
        &operation.dart,
        &operation.typescript,
        &operation.gleam,
    ] {
        assert!(source.contains("/v1/rpc"));
        assert!(source.contains("POST"));
        assert!(!source.contains("/v1/events/stream"));
        assert!(!source.contains("GET"));
    }
}

#[test]
fn task_10_generated_facades_prepare_but_do_not_open_streams() {
    let operation = &server_stream().operations[0];
    assert!(operation.typescript.contains("client.prepare"));
    assert!(!operation.typescript.contains(".stream()"));
    assert!(operation.dart.contains("client.prepare"));
    assert!(!operation.dart.contains(".stream()"));
    assert!(operation.go.contains("PrepareRPCStream"));
    assert!(!operation.go.contains(".Stream("));
    assert!(operation.rust.contains("client.prepare"));
    assert!(!operation.rust.contains(".stream()"));
    assert!(operation.gleam.contains("stream_rpc.prepare"));
    assert!(!operation.gleam.contains("stream_rpc.stream"));
}

#[test]
fn task_11_unary_v3_surface_remains_make_call_builder_based() {
    let mut unary = operation(RpcStreamMode::Unary);
    unary.operation_key = "demo.events.get_events".to_owned();
    unary.source.operation = Some("get_events".to_owned());
    unary.source.invoker = Some("__ores_invoke_get_events".to_owned());
    let bundle = rpc_client_bundle_v3(
        &route_map("demo.events.get_events"),
        &[unary],
        "crate::dto",
        "public",
    )
    .expect("unary bundle");
    let generated = &bundle.operations[0];
    assert_eq!(generated.stream, RpcStreamMode::Unary);
    assert!(generated
        .typescript
        .contains("RpcCallBuilder<GetEventsResponse"));
    assert!(!generated.typescript.contains("RpcStreamCallBuilder"));
}

#[test]
fn task_12_client_stream_stays_fail_closed() {
    let error = rpc_client_bundle_v3(
        &route_map("demo.events.watch_events_stream"),
        &[operation(RpcStreamMode::ClientStream)],
        "crate::dto",
        "public",
    )
    .expect_err("client stream must remain unsupported");
    assert!(error.contains("client_stream"));
    assert!(error.contains("outbound stream writes"));
}

#[test]
fn task_13_bidi_stays_fail_closed() {
    let error = rpc_client_bundle_v3(
        &route_map("demo.events.watch_events_stream"),
        &[operation(RpcStreamMode::Bidi)],
        "crate::dto",
        "public",
    )
    .expect_err("bidi must remain unsupported");
    assert!(error.contains("bidi"));
    assert!(error.contains("outbound stream writes"));
}

#[test]
fn task_14_server_stream_request_sections_fail_closed() {
    let mut stream = operation(RpcStreamMode::ServerStream);
    stream.request.body_schema = Some(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {"cursor": {"type": "string"}},
        "required": ["cursor"]
    }));
    let error = rpc_client_bundle_v3(
        &route_map("demo.events.watch_events_stream"),
        &[stream],
        "crate::dto",
        "public",
    )
    .expect_err("request sections are not portable yet");
    assert!(error.contains("only NoSection streams are generated"));
}

/// The same operation with no route.rs at all: no HTTP method, no HTTP path, no
/// adapter file. Only what identifies it over RPC is left.
fn route_less(mode: RpcStreamMode) -> ores_api_docs::RpcOperationContract {
    let mut contract = operation(mode);
    contract.http = None;
    contract.source.route_file = None;
    contract.source.handlers_file = Some("src/routes/events/handlers.rs".to_owned());
    contract
}

#[test]
fn a_route_less_operation_generates_clients() {
    // The point of making `http` optional. The registry contract has always
    // allowed an operation with no HTTP projection; this IR could not represent
    // one, so consumers dropped them or fabricated a route.
    for mode in [RpcStreamMode::Unary, RpcStreamMode::ServerStream] {
        let contract = route_less(mode);
        assert!(contract.is_route_less());
        contract.validate().expect("a route-less contract is valid");
        let bundle = rpc_client_bundle_v3(
            &route_map("demo.events.watch_events_stream"),
            &[contract],
            "crate::dto",
            "public",
        )
        .expect("a route-less operation must generate");
        assert_eq!(bundle.manifest.http_endpoint, "/v1/rpc");
        assert_eq!(
            bundle.manifest.operations,
            vec!["demo.events.watch_events_stream".to_owned()],
            "the route-less operation must be in the bundle, not dropped from it"
        );
        assert_eq!(bundle.operations.len(), 1);
    }
}

#[test]
fn task_15_server_stream_requires_canonical_rpc_transport_path_not_route_projection() {
    // Twin of the test above: identical, except the transport path is gone.
    let mut stream = route_less(RpcStreamMode::ServerStream);
    stream.rpc_transport_path = "";
    let error = rpc_client_bundle_v3(
        &route_map("demo.events.watch_events_stream"),
        &[stream],
        "crate::dto",
        "public",
    )
    .expect_err("missing RPC transport path must fail");
    assert!(error.contains("canonical absolute RPC transport path"));
    assert!(error.contains("route.rs HTTP projection metadata is not the RPC transport authority"));
}

#[test]
fn a_contract_that_contradicts_itself_is_refused() {
    // Each case is a valid contract with ONE field changed.
    let valid = operation(RpcStreamMode::Unary);
    valid
        .validate()
        .expect("the premise: unchanged, it is valid");

    let mut projection_without_adapter = valid.clone();
    projection_without_adapter.source.route_file = None;

    let mut adapter_without_projection = valid.clone();
    adapter_without_projection.http = None;

    let mut route_less_without_owner = route_less(RpcStreamMode::Unary);
    route_less_without_owner.source.handlers_file = None;

    let mut relative_http_path = valid.clone();
    relative_http_path.http.as_mut().expect("http").path = "v1/version".to_owned();

    let mut stale_schema = valid.clone();
    stale_schema.schema_version = 2;

    for (why, contract, expected) in [
        (
            "projection without adapter",
            projection_without_adapter,
            "has no source",
        ),
        (
            "adapter without projection",
            adapter_without_projection,
            "has no HTTP projection",
        ),
        (
            "route-less without owner",
            route_less_without_owner,
            "must name the handlers.rs",
        ),
        ("relative HTTP path", relative_http_path, "absolute path"),
        (
            "stale schema version",
            stale_schema,
            "schema_version must be 3",
        ),
    ] {
        let error = contract.validate().expect_err(why);
        assert!(error.contains(expected), "{why}: {error}");
        // And the generator refuses it too, rather than emitting from it.
        rpc_client_bundle_v3(
            &route_map("demo.events.watch_events_stream"),
            &[contract],
            "crate::dto",
            "public",
        )
        .expect_err(why);
    }
}
