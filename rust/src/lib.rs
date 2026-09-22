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
pub mod page_docs;
pub mod page_lambda_codegen;
pub mod page_layout;
pub mod page_layout_codegen;
pub mod page_layout_router_codegen;
pub mod page_router_codegen;
pub mod page_segment_codegen;
pub mod paths;
pub mod pool_codegen;
pub mod project;
pub mod request_headers;
pub mod route_folder_contract;
pub mod route_module;
pub mod route_source;
pub mod router_error_hints;
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
pub mod transport_leaf_build_identity;
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
#[cfg(feature = "operation-runtime")]
pub mod operation_server_stream;
#[cfg(feature = "operation-runtime")]
pub mod operation_stream_dispatch;
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
    OperationDispatchFn, OperationDispatchFuture, OperationDispatchInput, OperationDispatchResult,
    OperationHostError, OperationState, OperationStateError, OperationStateFn,
    OperationStateFuture, OperationStateInitError, OperationStreamDispatchFn,
    OperationStreamDispatchFuture, OperationStreamDispatchResult,
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
#[cfg(feature = "operation-runtime")]
pub use operation_server_stream::{
    next_rpc_v1_server_stream_frame, rpc_stream_frame_json, rpc_v1_server_stream_from_frames,
    OperationServerStream, RpcV1ServerStream, ServerStreamResult,
};
pub use operation_spec::{
    NoSection, OperationRequestData, OperationRequestError, OperationSpec, TypedOperationRequest,
};
#[cfg(feature = "operation-runtime")]
pub use operation_stream_dispatch::{
    dispatch_typed_json_server_stream_operation, dispatch_typed_json_server_stream_operation_in,
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
pub use page_layout::{
    page_layout_sources, page_segment_sources, PageSegmentSources, PAGE_ERROR_FILE,
    PAGE_LAYOUT_FILE, PAGE_LOADING_FILE, PAGE_NOT_FOUND_FILE, PAGE_TEMPLATE_FILE,
};
pub use page_layout_codegen::page_compile_glue_with_layouts as page_compile_glue;
pub use page_layout_router_codegen::page_router_glue_with_layouts as page_router_glue;
pub use page_segment_codegen::page_loading_entry_ident;
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
pub use transport_leaf_build_identity::{
    GraphqlLeafKind, TransportLeafBuildIdentity, TransportLeafBuildIdentityError,
    TransportLeafKind, TransportLeafStreamMode, TRANSPORT_LEAF_ABI_VERSION,
    TRANSPORT_LEAF_BUILD_IDENTITY_SCHEMA,
};
#[cfg(feature = "operation-runtime")]
pub use typed_operation_context::{
    invoke_typed_context_operation, invoke_typed_context_server_stream_operation,
    TypedOperationContext,
};
pub use verified_operation_contract::verified_rpc_operation_contract;

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const GENERATED_BY: &str = "ores-api-docs";

#[cfg(test)]
#[path = "../../generated/rust/src/pmap_api.rs"]
mod generated_pmap_api;
