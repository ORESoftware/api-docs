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
pub mod client_codegen_v3;
pub mod client_stream;
pub mod discovery;
pub mod fs_codegen;
pub mod fs_discovery;
pub mod fs_route;
pub mod generated_rpc_layout;
pub mod headers;
pub mod html;
pub mod infer;
pub mod map;
pub mod module_analysis;
pub mod operation_spec;
pub mod opto_sync;
pub mod page_build;
pub mod page_lambda_codegen;
pub mod page_layout;
pub mod page_layout_codegen;
pub mod page_layout_router_codegen;
pub mod page_router_codegen;
pub mod paths;
pub mod pool_codegen;
pub mod project;
pub mod request_headers;
pub mod route_folder_contract;
pub mod route_module;
pub mod route_source;
pub mod rpc_client_options;
pub mod rpc_client_surface;
pub mod rpc_fluent;
pub mod rpc_operation_contract;
#[path = "../../runtime/rust/telemetry.rs"]
pub mod rpc_telemetry;
pub mod rpc_v1;
pub mod schema;
#[path = "shared_operation_v2.rs"]
pub mod shared_operation;
pub mod shared_operation_invocation;
pub mod telemetry;
pub mod template;
mod typed_rpc_sdk_codegen;
pub mod verified_operation_contract;

#[cfg(feature = "axum")]
pub mod axum_router;
#[cfg(feature = "operation-runtime")]
pub mod operation_dispatch;
#[cfg(feature = "operation-runtime")]
pub mod operation_dispatch_input;
#[cfg(feature = "operation-runtime")]
pub mod operation_policy;
#[cfg(feature = "operation-runtime")]
pub mod operation_runtime;
#[cfg(feature = "axum")]
pub mod rpc_axum;
#[cfg(feature = "axum")]
pub mod rpc_file_router;
#[cfg(feature = "operation-runtime")]
pub mod rpc_http_context;
#[cfg(feature = "axum")]
mod rpc_key_lookup;
#[cfg(feature = "axum")]
pub mod rpc_shared_operation;
#[cfg(feature = "operation-runtime")]
pub mod typed_operation_context;

pub use binding::{RouteBinding, RpcHttp, RpcMethod, RpcTransport, UnaryFn};
pub use call::{
    encode_length_prefixed, split_length_prefixed, RpcCall, RpcReceipt, Transport, MAX_FRAME_BYTES,
};
pub use catalog::Catalog;
pub use client_codegen::{rpc_client_bundle, RpcClientBundle, RpcClientBundleManifest};
pub use client_codegen_v2::{rpc_client_bundle_v2, RpcClientBundleV2, RpcClientBundleV2Manifest};
pub use client_codegen_v3::{
    rpc_client_bundle_v3, RpcClientBundleV3, RpcClientBundleV3Manifest,
    RpcClientTransportSourcesV3, RpcOperationClientSourcesV3,
};
pub use client_stream::{
    FramedRpcStream, OresRpcStreamClient, RpcStreamCall, RpcStreamCallBuilder, RpcStreamCarrier,
    RpcStreamClient, RpcStreamContext, RpcStreamError, RpcStreamFrame, RpcStreamPrepareError,
    RpcStreamRequest, RpcStreamSession,
};
pub use discovery::{DocsDiscoveryManifest, DocsProjectionRoutes, DISCOVERY_SCHEMA_VERSION};
pub use fs_codegen::{api_compile_glue, api_server_glue};
pub use fs_discovery::discover_fs_routes;
pub use fs_route::{
    validate_and_sort_fs_routes, FsRoute, FsRouteError, FsRouteKind, FsRouteSegment,
};
pub use generated_rpc_layout::{
    generated_source_header, RpcOperationModulePath, RpcSdkLanguage, GENERATED_AGENTS,
    GENERATED_README,
};
pub use map::{AuthorizationPolicy, OptoSyncQueue, RouteEntry, RouteMap};
pub use module_analysis::{
    analyze_generator_source, analyze_page_source, ModuleAnalysisError, PageModuleMetadata,
    RouteModuleAnalysis, RouteModuleKind,
};
#[cfg(feature = "operation-runtime")]
pub use operation_dispatch::{
    dispatch_typed_json_operation_in, rpc_receipt_for_dispatch_error, DispatchError,
};
#[cfg(feature = "operation-runtime")]
pub use operation_dispatch_input::{
    OperationDispatchInput, OperationHostError, OperationState, OperationStateError,
    OperationStateFn, OperationStateFuture, OperationStateInitError,
};
#[cfg(feature = "operation-runtime")]
pub use operation_policy::{
    AllowAllOperationPolicy, IdentityProvider, OperationDescriptor, OperationOutcomeKind,
    OperationPolicy, OperationPolicyFuture, OperationPolicyOutcome, OperationPolicyPermit,
    OperationPolicyRejection, OperationPolicyRequest, ProviderIdentity,
};
#[cfg(feature = "operation-runtime")]
pub use operation_runtime::{
    decode_rpc_operation_input, invoke_operation_with_policy, invoke_shared_rpc_operation,
    ExecutionEnvironmentKind, OperationContext, OperationInvokeError, OperationTransportKind,
    RpcV1OperationAdapterError,
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
pub use page_lambda_codegen::{
    page_lambda_entry_ident, page_lambda_finalize_ident, page_lambda_glue,
    page_lambda_glue_with_auth, GENERATED_PAGE_LAMBDA_MARKER, PAGE_LAMBDA_ADMISSION_FN,
    PAGE_LAMBDA_PAGES_MODULE, PAGE_LAMBDA_STATE_FN, PAGE_LAMBDA_WEB_APP_ALIAS,
};
pub use page_layout::{page_layout_sources, PAGE_LAYOUT_FILE};
pub use page_layout_codegen::page_compile_glue_with_layouts as page_compile_glue;
pub use page_layout_router_codegen::page_router_glue_with_layouts as page_router_glue;
pub use pool_codegen::rpc_pool_bindings;
pub use project::contract_sha256;
pub use request_headers::{
    is_canonical_application_header_name, is_runtime_owned_request_header, HeaderAdmission,
    HeaderAdmissionError, RUNTIME_OWNED_REQUEST_HEADERS,
};
#[allow(deprecated)]
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
pub use rpc_axum::{
    rpc_v1_router, rpc_v1_router_with_telemetry, RpcV1Dispatcher, RPC_V1_HTTP_PATH,
};
#[cfg(feature = "axum")]
pub use rpc_file_router::{
    filesystem_rpc_v1_router, RpcV1RouteBinding, RpcV1RouteFuture, RpcV1RouteHandler,
    RpcV1RouteRegistry, RpcV1RouteRegistryError,
};
#[cfg(feature = "operation-runtime")]
pub use rpc_http_context::{IngressProvenance, RpcV1HttpContext};
pub use rpc_operation_contract::{
    rpc_operation_contract, rpc_operation_contract_with_route_source, rpc_operation_contracts,
    RpcClientAudience, RpcCodecSet, RpcHttpProjection, RpcOperationContract, RpcOperationScope,
    RpcOperationSource, RpcPayloadCodec, RpcRequestShape, RpcResponseShape, RpcStreamMode,
    RPC_OPERATION_CONTRACT_SCHEMA_VERSION, RPC_V1_HTTP_PATH as RPC_OPERATION_HTTP_PATH,
};
#[cfg(feature = "axum")]
pub use rpc_shared_operation::{
    shared_operation_rpc_v1_router, RpcV1SharedOperationBinding, RpcV1SharedOperationFuture,
    RpcV1SharedOperationHandler, RpcV1SharedOperationRegistry, RpcV1SharedOperationRegistryError,
};
pub use rpc_telemetry::{
    emit_dispatch_error as emit_rpc_error_event, Carrier as RpcTelemetryCarrier,
    ErrorKind as RpcTelemetryErrorKind, Outcome as RpcTelemetryOutcome, RpcErrorEvent, RpcEvent,
    RpcLayer as RpcTelemetryLayer, RpcTelemetrySink,
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
#[cfg(feature = "operation-runtime")]
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
