//! Client-facing API for ORE route maps and validated RPC v1 envelopes.
//!
//! This is a facade over `ores-api-docs`, not a second validator or generator.
//! Types retain their identity across client and server imports. The dependency
//! disables the core crate's default Axum feature; this crate exposes no server
//! router and does not open HTTP, TCP, WebSocket, or NATS connections.
//!
//! Use the existing digest-bound route bundle for service-specific route keys.
//! TypeSpec and JSON Schema/OpenAPI remain independent authored authorities.
//! RPC v1 envelopes must not be mixed with the RIDL v2 streaming runtime.
//!
//! ```
//! use ores_api_docs_client::{decode_rpc_v1_call, RpcV1Correlator};
//!
//! let call = decode_rpc_v1_call(
//!     br#"{"v":1,"op":"call","id":"request-1","key":"get_item"}"#,
//! )?;
//! assert_eq!(call.key, "get_item");
//! let mut ids = RpcV1Correlator::new("request-")?;
//! assert_eq!(ids.take()?, "request-1");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Encoding and correlation use the same shared types. Applications supply the
//! actual network adapter; this example deliberately does not open a socket.
//!
//! ```
//! use ores_api_docs_client::{
//!     assert_rpc_v1_receipt_for_call, decode_rpc_v1_call, decode_rpc_v1_receipt,
//!     OptionalJson, RpcV1Call, RpcV1Receipt, SchemaError, Transport,
//! };
//!
//! let mut call = RpcV1Call::new("client-1", "get_item");
//! call.transport = Some(Transport::Websocket);
//! let payload = call.encode()?;
//! assert_eq!(decode_rpc_v1_call(&payload)?, call);
//!
//! let reply = RpcV1Receipt::success("client-1", "get_item", OptionalJson::absent());
//! let receipt = decode_rpc_v1_receipt(&reply.encode()?)?;
//! assert_rpc_v1_receipt_for_call(&call, &receipt)?;
//! # Ok::<(), SchemaError>(())
//! ```

#![forbid(unsafe_code)]

// Explicitly expose client-relevant modules. Do not glob-export the core crate:
// that could silently introduce server APIs when features unify in a consumer.
pub use ores_api_docs::{
    binding, call, discovery, headers, map, opto_sync, paths, rpc_v1, schema, telemetry,
    template,
};

pub use ores_api_docs::{
    assert_rpc_v1_receipt_for_call, contract_sha256, decode_rpc_v1_call,
    decode_rpc_v1_receipt, encode_length_prefixed, expand_path, path_template_vars,
    rpc_v1_call_from_ndjson, rpc_v1_receipt_from_ndjson, split_length_prefixed,
    split_rpc_v1_length_prefixed, DocsDiscoveryManifest, DocsProjectionRoutes, OptionalJson,
    OptoSyncQueue, QueryValue, RouteBinding, RouteEntry, RouteMap, RouteMapEnvelope, RpcCall,
    RpcHttp, RpcMethod, RpcReceipt, RpcTransport, RpcV1Call, RpcV1Correlator, RpcV1Envelope,
    RpcV1Receipt, TelemetryAttributes, Transport, UnaryFn, DISCOVERY_SCHEMA_VERSION,
    GENERATED_BY, MAX_FRAME_BYTES, OPTO_SYNC_SCOPE, RPC_SYSTEM, RPC_V1_VERSION, SCHEMA_VERSION,
};
pub use ores_api_docs::schema::SchemaError;
pub use ores_api_docs::template::{encode_query, TemplateError};
