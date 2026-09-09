//! Transport-neutral Rust client for ORESoftware route maps and v1 RPC.
//!
//! This is an additive facade over `ores-api-docs`, not a second implementation
//! or schema authority. The dependency disables Axum's default feature. Network
//! I/O, TLS, authentication, retries and timeouts remain the caller's concern.
//! RIDL v2 streaming frames belong to `runtime/rust`, not this v1 client.
//!
//! ```
//! use ores_api_docs_client::{
//!     assert_rpc_v1_receipt_for_call, decode_rpc_v1_call, decode_rpc_v1_receipt,
//!     OptionalJson, RpcV1Call, RpcV1Receipt, Transport,
//! };
//!
//! let mut call = RpcV1Call::new("client-1", "get_item");
//! call.transport = Some(Transport::Websocket);
//! let payload = call.encode()?;
//! assert_eq!(decode_rpc_v1_call(&payload)?, call);
//!
//! // In an application, these bytes come from its own transport adapter.
//! let reply = RpcV1Receipt::success("client-1", "get_item", OptionalJson::absent());
//! let receipt = decode_rpc_v1_receipt(&reply.encode()?)?;
//! assert_rpc_v1_receipt_for_call(&call, &receipt)?;
//! # Ok::<(), ores_api_docs_client::SchemaError>(())
//! ```

#![forbid(unsafe_code)]

// Re-export the exact upstream types: no copied validators, frames or codegen.
pub use ores_api_docs::schema::SchemaError;
pub use ores_api_docs::{
    assert_rpc_v1_receipt_for_call, contract_sha256, decode_rpc_v1_call, decode_rpc_v1_receipt,
    encode_length_prefixed, expand_path, path_template_vars, rpc_v1_call_from_ndjson,
    rpc_v1_receipt_from_ndjson, split_length_prefixed, split_rpc_v1_length_prefixed,
    DocsDiscoveryManifest, DocsProjectionRoutes, OptionalJson, OptoSyncQueue, QueryValue,
    RouteBinding, RouteEntry, RouteMap, RouteMapEnvelope, RpcHttp, RpcMethod, RpcTransport,
    RpcV1Call, RpcV1Correlator, RpcV1Envelope, RpcV1Receipt, Transport, UnaryFn,
    DISCOVERY_SCHEMA_VERSION, MAX_FRAME_BYTES, OPTO_SYNC_SCOPE, RPC_V1_VERSION, SCHEMA_VERSION,
};
pub use ores_api_docs::{binding, call, discovery, map, opto_sync, rpc_v1, template};
