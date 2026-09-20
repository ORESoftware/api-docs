#![forbid(unsafe_code)]

mod axum_router;
mod binding;
mod call;
mod catalog;
mod client_codegen;
mod client_codegen_v2;
mod client_codegen_v3;
mod client_stream;
mod discovery;
mod fs_codegen;
mod fs_discovery;
mod fs_route;
mod generated_rpc_layout;
mod headers;
mod html;
mod infer;
mod map;
mod module_analysis;
mod operation_dispatch;
mod operation_dispatch_input;
mod operation_policy;
mod operation_runtime;
mod operation_spec;
mod opto_sync;
mod page_build;
mod page_lambda_codegen;
mod page_router_codegen;
mod paths;
mod pool_codegen;
mod project;
mod request_headers;
mod route_folder_contract;
mod route_module;
mod route_source;
mod rpc_axum;
mod rpc_client_options;
mod rpc_file_router;
mod rpc_fluent;
mod rpc_http_context;
mod rpc_key_lookup;
mod rpc_operation_contract;
mod rpc_shared_operation;
mod rpc_telemetry;
mod rpc_v1;
mod schema;
mod shared_operation;
mod shared_operation_invocation;
mod telemetry;
mod template;
mod typed_operation_context;
mod verified_operation_contract;

pub use axum_router::{discovery_router, CatalogState};
pub use binding::{operation_binding, OperationBinding};
pub use call::{
    decode_length_prefixed_call, decode_ndjson_call, encode_length_prefixed_call,
    encode_ndjson_call, validate_call_json, CallValidationError, RpcCall,
};
pub use catalog::{Catalog, CatalogError, Operation};
pub use client_codegen::{generate_client_bundle, ClientBundle};
pub use client_codegen_v2::{generate_named_client_bundle, NamedClientBundle};
pub use client_codegen_v3::{generate_typed_client_bundle, TypedClientBundle};
pub use client_stream::{RpcStreamHandle, RpcStreamItem};
pub use discovery::{discovery_manifest, DiscoveryManifest};
pub use fs_codegen::{
    api_route_module_ident, method_router_expr, page_module_ident, render_api_route_module,
    render_page_route_module,
};
pub use fs_discovery::{discover_api_routes, discover_pages, FsDiscoveryError};
pub use fs_route::{
    validate_route_conflicts, FsRoute, FsRouteConflict, FsRouteKind, RouteSegment,
};
pub use generated_rpc_layout::{
    generated_rpc_path_for_key, generated_rpc_root, GeneratedRpcLayoutError,
};
pub use headers::{html_headers, json_headers};
pub use html::render_html;
pub use infer::{infer_connect_path, infer_http_method};
pub use map::{parse_map, MapError, RpcMap};
pub use module_analysis::{analyze_gen_module, analyze_page_module, ModuleAnalysis, ModuleAnalysisError};
pub use operation_dispatch::{
    dispatch_operation, dispatch_operation_in, dispatch_operation_with_policy,
    render_operation_dispatch_result, DispatchError, OperationDispatchResult,
};
pub use operation_dispatch_input::{
    OperationDispatchFuture, OperationDispatchInput, OperationHostError, OperationState,
    OperationStateError, OperationStateFn, OperationStateFuture, OperationStateInitError,
};
pub use operation_policy::{
    AfterOperationContext, BeforeOperationContext, OperationPolicy, OperationPolicyError,
    OperationPolicyResult, ProviderIdentity, ProviderIdentityKind,
};
pub use operation_runtime::{
    ExecutionEnvironmentKind, IngressProvenance, OperationContext, OperationTransportKind,
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
    GENERATED_PAGE_LAMBDA_MARKER, PAGE_LAMBDA_PAGES_MODULE, PAGE_LAMBDA_STATE_FN,
    PAGE_LAMBDA_WEB_APP_ALIAS,
};
pub use page_router_codegen::page_router_glue;
pub use pool_codegen::rpc_pool_bindings;
pub use project::contract_sha256;
pub use request_headers::{
    is_canonical_application_header_name, is_runtime_owned_request_header, HeaderAdmission,
    HeaderAdmissionError,
};
pub use route_folder_contract::{
    analyze_route_folder, RouteFolderContract, RouteFolderContractError, GEN_FILE, HANDLERS_FILE,
    RPC_FILE, ROUTE_FILE,
};
pub use route_module::{parse_route_module, RouteMethodBinding, RouteModuleError};
pub use route_source::{parse_route_source, RouteSourceError, RouteSourceOperation};
pub use rpc_axum::{rpc_endpoint, RpcAxumState};
pub use rpc_client_options::{
    audit_call_sites, build_plan, conformance_catalog, derive_method_name, derive_variant_name,
    read_option_catalog, RpcCallSiteFinding, RpcClientOptionsError, RpcOptionCatalog,
    RpcOptionPlan,
};
pub use rpc_file_router::{
    build_rpc_file_router, dispatch_rpc_file_call, RpcFileRouter, RpcFileRouterError,
};
pub use rpc_fluent::{
    RpcClient, RpcClientError, RpcExecutionIdentity, RpcExecutionPlan, RpcPreparedCall,
    RpcPreparedStream,
};
pub use rpc_http_context::RpcV1HttpContext;
pub use rpc_key_lookup::{lookup_rpc_key, RpcKeyLookupError};
pub use rpc_operation_contract::{
    normalize_rpc_operation_contract, NormalizedRpcOperation, RpcOperationContractError,
};
pub use rpc_shared_operation::{
    invoke_shared_operation, SharedOperationRegistry, SharedOperationRegistryError,
};
pub use rpc_telemetry::{
    emit_error as emit_rpc_error_event, NoopRpcTelemetrySink, RpcErrorEvent, RpcTelemetrySink,
};
pub use rpc_v1::{
    correlation_id, decode_rpc_v1_call, encode_rpc_v1_receipt, RpcV1Call, RpcV1CallDecodeError,
    RpcV1Receipt, RpcV1ReceiptError,
};
pub use schema::{load_schema, SchemaName};
pub use shared_operation::{
    analyze_shared_operations, SharedOperationAnalysis, SharedOperationAnalysisError,
    SharedOperationIr,
};
pub use shared_operation_invocation::{
    verify_shared_operation_invocation, SharedOperationInvocationError,
};
pub use telemetry::{
    operation_telemetry_attributes, rpc_telemetry_attributes, OperationTelemetryAttributes,
    RpcTelemetryAttributes,
};
pub use template::{expand_path_template, PathTemplateError};
pub use typed_operation_context::{
    decode_operation_context, OperationContextDecodeError, TypedOperationContext,
};
pub use verified_operation_contract::{
    verify_operation_contract, VerifiedOperationContract, VerifiedOperationContractError,
};
