//! Client-facing API for ORE route maps and validated RPC v1 envelopes.
//!
//! This is a facade over `ores-api-docs`, not a second validator or generator.
//! Types retain their identity across client and server imports. The dependency
//! disables the core crate's default Axum server adapter; this crate exposes no server
//! router and does not open HTTP, TCP, WebSocket, or NATS connections.
//!
//! Use the existing digest-bound route bundle for service-specific route keys.
//! TypeSpec and JSON Schema/OpenAPI remain independent authored authorities.
//! RPC v1 envelopes must not be mixed with the RIDL v2 streaming runtime.

#![forbid(unsafe_code)]

pub mod page;
pub mod page_admission;
pub mod page_layout;
pub mod page_response;
pub mod page_segment;
pub mod typed;

// Page proc macros deliberately live in the separate `ores-api-docs-macros`
// package so this isolated client facade keeps a minimal, server-free graph.

// Explicitly expose client-relevant modules. Do not glob-export the core crate:
// that could silently introduce server APIs when features unify in a consumer.
pub use ores_api_docs::{
    binding, call, discovery, fs_route, headers, map, opto_sync, paths, rpc_v1, schema, telemetry,
    template, FramedRpcStream, OresRpcStreamClient, RpcStreamCall, RpcStreamCallBuilder,
    RpcStreamCarrier, RpcStreamClient, RpcStreamContext, RpcStreamError, RpcStreamFrame,
    RpcStreamPrepareError, RpcStreamRequest, RpcStreamSession,
};

pub use ores_api_docs::schema::SchemaError;
pub use ores_api_docs::template::{encode_query, TemplateError};
pub use ores_api_docs::{
    assert_rpc_v1_receipt_for_call, contract_sha256, decode_rpc_v1_call, decode_rpc_v1_receipt,
    encode_length_prefixed, expand_path, path_template_vars, rpc_v1_call_from_ndjson,
    rpc_v1_receipt_from_ndjson, split_length_prefixed, split_rpc_v1_length_prefixed,
    validate_and_sort_fs_routes, DocsDiscoveryManifest, DocsProjectionRoutes, FsRoute,
    FsRouteError, FsRouteKind, FsRouteSegment, OptionalJson, OptoSyncQueue, QueryValue,
    RouteBinding, RouteEntry, RouteMap, RouteMapEnvelope, RpcCall, RpcHttp, RpcMethod, RpcReceipt,
    RpcTransport, RpcV1Call, RpcV1Correlator, RpcV1Envelope, RpcV1Receipt, TelemetryAttributes,
    Transport, UnaryFn, DISCOVERY_SCHEMA_VERSION, GENERATED_BY, MAX_FRAME_BYTES, OPTO_SYNC_SCOPE,
    RPC_SYSTEM, RPC_V1_VERSION, SCHEMA_VERSION,
};
pub use page::{
    GenerateStaticParamsFn, GenerateStaticParamsFuture, PageAssets, PageAssetsFn, PageClientKind,
    PageConfig, PageConfigFn, PageContext, PageDelivery, PageDocument, PageError, PageFn,
    PageFuture, PageLambdaStateError, PageLambdaStateFn, PageLambdaStateFuture, PageMetadata,
    PageRenderMode, PageRenderer, PageResult, PageState, PrerenderContext, PrerenderFn,
    PrerenderPath, PrerenderResult, RevalidationPolicy,
};
pub use page_admission::{
    admit_public_page, PageAdmissionFn, PageAdmissionFuture, PageAdmissionInput,
    PageAdmissionRejection, PageAdmissionResult, PageRequestContext, PageRequestMethod,
};
pub use page_layout::{PageLayoutFn, PageLayoutFuture};
pub use page_response::{
    finalize_page_response, FinalizedPageResponse, PageFinalizeFn, PageResponseAssets,
    PageResponseRequestHints, ERROR_CONTENT_TYPE, HTML_CONTENT_TYPE,
};
pub use page_segment::{
    PageErrorBoundaryFn, PageErrorBoundaryFuture, PageLoadingFn, PageLoadingFuture,
    PageNotFoundFn, PageNotFoundFuture, PageTemplateFn, PageTemplateFuture,
};
pub use typed::{TypedApiClient, TypedApiTransport};