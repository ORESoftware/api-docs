//! Route-map API documentation.
//!
//! The interchange contract is a JSON object whose **keys are operations** and
//! whose **values are HTTP routes**. Languages may author those keys as
//! annotations, param types, return types, function types, or a mix; they
//! serialize to the same map.
//!
//! The map is projected into several standards closely (not one of them
//! perfectly): OpenAPI 3.1, JSON Schema 2020-12, Connect JSON unary, OpenRPC
//! 1.3, and JSON Hyper-Schema links. Every projection is checked with JSON
//! Schema and carries the same normalized RPC contract SHA-256 as generated
//! Rust, TypeScript, Dart, Gleam, and Go route surfaces.

pub mod binding;
pub mod call;
pub mod catalog;
pub mod client_codegen;
pub mod client_codegen_v2;
pub mod discovery;
pub mod fs_codegen;
pub mod fs_discovery;
pub mod fs_route;
pub mod headers;
pub mod html;
pub mod infer;
pub mod map;
pub mod module_analysis;
pub mod operation_spec;
pub mod opto_sync;
pub mod page_build;
pub mod page_router_codegen;
pub mod paths;
pub mod pool_codegen;
pub mod project;
pub mod request_headers;
pub mod route_folder_contract;
pub mod route_module;
pub mod route_source;
pub mod rpc_operation_contract;
pub mod rpc_v1;
pub mod schema;
pub mod shared_operation;
pub mod shared_operation_invocation;
pub mod telemetry;
pub mod template;
pub mod verified_operation_contract;

#[cfg(feature = "axum")]
pub mod axum_router;
#[cfg(feature = "axum")]
pub mod operation_policy;
#[cfg(feature = "axum")]
pub mod operation_runtime;
#[cfg(feature = "axum")]
mod rpc_key_lookup;
#[cfg(feature = "axum")]
pub mod rpc_axum;
#[cfg(feature = "axum")]
pub mod rpc_file_router;
#[cfg(feature = "axum")]
pub mod rpc_shared_operation;
#[cfg(feature = "axum")]
pub mod typed_operation_context;

pub use binding::{RouteBinding, RpcHttp, RpcMethod, RpcTransport, UnaryFn};
pub use call::{
    encode_length_prefixed, split_length_prefixed, RpcCall, RpcReceipt, Transport, MAX_FRAME_BYTES,
};
pub use catalog::Catalog;
pub use client_codegen::{rpc_client_bundle, RpcClientBundle, RpcClientBundleManifest};
pub use client_codegen_v2::{rpc_client_bundle_v2, RpcClientBundleV2, RpcClientBundleV2Manifest};
pub use discovery::{DocsDiscoveryManifest, DocsProjectionRoutes, DISCOVERY_SCHEMA_VERSION};
pub use fs_codegen::{api_compile_glue, api_server_glue, page_compile_glue};
pub use fs_discovery::discover_fs_routes;
pub use fs_route::{
    validate_and_sort_fs_routes, FsRoute, FsRouteError, FsRouteKind, FsRouteSegment,
};
pub use map::{AuthorizationPolicy, OptoSyncQueue, RouteEntry, RouteMap};
pub use module_analysis::{
    analyze_generator_source, analyze_page_source, ModuleAnalysisError, PageModuleMetadata,
    RouteModuleAnalysis, RouteModuleKind,
};
#[cfg(feature = "axum")]
pub use operation_policy::{
    AllowAllOperationPolicy, OperationDescriptor, OperationPolicy, OperationPolicyFuture,
    OperationPolicyOutcome, OperationPolicyPermit, OperationPolicyRejection, OperationPolicyRequest,
};
#[cfg(feature = "axum")]
pub use operation_runtime::{
    decode_rpc_operation_input, invoke_operation_with_policy, invoke_shared_rpc_operation,
    OperationContext, OperationInvokeError, OperationTransportKind, RpcV1OperationAdapterError,
};
pub use operation_spec::{
    NoSection, OperationRequestData, OperationRequestError, OperationSpec, TypedOperationRequest,
};
pub use opto_sync::{RouteMapEnvelope, SCOPE as OPTO_SYNC_SCOPE};
pub use page_build::{
    materialize_finalized_page_build, read_page_build_manifest, rewrite_page_router_glue,
    write_page_build_manifest, write_page_build_outputs, ContentAsset, PageBuildError,
    PageBuildManifest, PageBuildOutputs, PageBuildRoute, WasmBuildPlan, WASM_HAVE_COOKIE,
    WASM_HAVE_HEADER,
};
pub use page_router_codegen::page_router_glue;
pub use pool_codegen::rpc_pool_bindings;
pub use project::contract_sha256;
pub use request_headers::{
    is_canonical_application_header_name, is_runtime_owned_request_header, HeaderAdmission,
    HeaderAdmissionError, RUNTIME_OWNED_REQUEST_HEADERS,
};
pub use route_folder_contract::{
    analyze_route_folder_sources, verify_generated_rpc_source, verify_route_folder_invocations,
    RouteFolderContract, GENERATED_RPC_MARKER, GEN_FILE, HANDLERS_FILE, ROUTE_FILE, RPC_FILE,
};
pub use route_module::{ApiRouteDefinition, ApiRouteOperation, RouteDefinitionFn};
pub use route_source::{
    analyze_http_route_source, HttpRouteHandlerSource, HttpRouteModuleSource, HttpRouteSourceError,
    RpcRouteAttributeSource, HTTP_ROUTE_EXPORTS,
};
#[cfg(feature = "axum")]
pub use rpc_axum::{rpc_v1_router, RpcV1Dispatcher, RpcV1HttpContext, RPC_V1_HTTP_PATH};
#[cfg(feature = "axum")]
pub use rpc_file_router::{
    filesystem_rpc_v1_router, RpcV1RouteBinding, RpcV1RouteFuture, RpcV1RouteHandler,
    RpcV1RouteRegistry, RpcV1RouteRegistryError,
};
pub use rpc_operation_contract::{
    rpc_operation_contract, rpc_operation_contract_with_route_source, rpc_operation_contracts,
    RpcClientAudience, RpcCodecSet, RpcHttpProjection, RpcOperationContract, RpcOperationScope,
    RpcOperationSource, RpcPayloadCodec, RpcRequestShape, RpcResponseShape,
    RPC_V1_HTTP_PATH as RPC_OPERATION_HTTP_PATH,
};
#[cfg(feature = "axum")]
pub use rpc_shared_operation::{
    shared_operation_rpc_v1_router, RpcV1SharedOperationBinding, RpcV1SharedOperationFuture,
    RpcV1SharedOperationHandler, RpcV1SharedOperationRegistry, RpcV1SharedOperationRegistryError,
};
pub use rpc_v1::{
    assert_rpc_v1_receipt_for_call, decode_rpc_v1_call, decode_rpc_v1_receipt,
    rpc_v1_call_from_ndjson, rpc_v1_receipt_from_ndjson, split_rpc_v1_length_prefixed,
    OptionalJson, RpcV1Call, RpcV1Correlator, RpcV1Envelope, RpcV1Receipt, RPC_V1_VERSION,
};
pub use shared_operation::{
    analyze_shared_operation_route_source, HttpOperationAdapterSource, RpcExecutionModel,
    SharedOperationRouteSource, SharedOperationSource, SharedOperationSourceError,
};
pub use shared_operation_invocation::verify_shared_operation_invocations;
pub use telemetry::{TelemetryAttributes, RPC_SYSTEM};
pub use template::{encode_query, expand_path, path_template_vars, QueryValue};
#[cfg(feature = "axum")]
pub use typed_operation_context::{invoke_typed_context_operation, TypedOperationContext};
pub use verified_operation_contract::verified_rpc_operation_contract;

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const GENERATED_BY: &str = "ores-api-docs";

#[cfg(test)]
#[path = "../../generated/rust/src/pmap_api.rs"]
mod generated_pmap_api;

#[cfg(test)]
#[path = "../../generated/rust/src/canonical_api.rs"]
mod generated_canonical_api;

#[cfg(test)]
#[path = "../../generated/rust/src/chptr_api.rs"]
mod generated_chptr_api;

#[cfg(test)]
#[path = "../../generated/rust/src/cliptown_api.rs"]
mod generated_cliptown_api;

#[cfg(test)]
#[path = "../../generated/rust/src/gha_indie_worker.rs"]
mod generated_gha_indie_worker;

#[cfg(test)]
#[path = "../../generated/rust/src/hhm_api.rs"]
mod generated_hhm_api;

#[cfg(test)]
#[path = "../../generated/rust/src/hnpt_api.rs"]
mod generated_hnpt_api;

#[cfg(test)]
#[path = "../../generated/rust/src/rpc_transports.rs"]
mod generated_rpc_transports;

#[cfg(test)]
mod generated_key_objects {
    #[test]
    fn pmap_frontend_uses_keys_not_paths() {
        use crate::generated_pmap_api::RouteKey;
        assert_eq!(
            RouteKey::parse("get_matter").unwrap().path(),
            "/v1/matters/{id}"
        );
        assert_eq!(RouteKey::CheckFieldSanity.as_str(), "CheckFieldSanity");
        assert!(RouteKey::ALL.len() >= 10);
    }

    #[test]
    fn canonical_and_chapter_maps_generate() {
        use crate::generated_canonical_api::RouteKey as Canonical;
        use crate::generated_chptr_api::RouteKey as Chapter;
        assert_eq!(
            Canonical::parse("create_quote").unwrap().path(),
            "/api/v1/quotes"
        );
        assert_eq!(
            Chapter::parse("get_chapter").unwrap().path(),
            "/v1/chapters/{chapterId}"
        );
    }

    #[test]
    fn cliptown_gha_hhm_hnpt_maps_generate() {
        use crate::generated_cliptown_api::RouteKey as Clip;
        use crate::generated_gha_indie_worker::RouteKey as Gha;
        use crate::generated_hhm_api::RouteKey as Hhm;
        use crate::generated_hnpt_api::RouteKey as Hnpt;
        assert_eq!(
            Clip::parse("app_vault_sync_push").unwrap().path(),
            "/v1/app-vault/{appId}/sync/push"
        );
        assert_eq!(
            Gha::parse("get_build_logs").unwrap().path(),
            "/builds/{job_id}/logs"
        );
        assert_eq!(
            Hhm::parse("get_reservation").unwrap().path(),
            "/api/v1/reservations/{id}"
        );
        assert_eq!(
            Hnpt::parse("trigger_decoy").unwrap().path(),
            "/decoys/{decoyId}/triggers"
        );
    }

    #[test]
    fn generated_transports_compile() {
        use crate::generated_rpc_transports::RouteKey;
        assert_eq!(
            RouteKey::parse("get_item").unwrap().transports(),
            &["http", "tcp", "websocket"]
        );
        assert_eq!(RouteKey::parse("tcp_ping").unwrap().transports(), &["tcp"]);
        assert_eq!(
            RouteKey::parse("nats_ping").unwrap().transports(),
            &["nats"]
        );
    }
}
