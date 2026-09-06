//! Strict, transport-neutral v1 RPC call/receipt envelopes.
//!
//! This is the Rust peer of the Dart, Go, and TypeScript v1 codecs. It remains
//! deliberately separate from RIDL v2 frames (`t: call | data | end | error |
//! cancel`). HTTP maps these fields to HTTP primitives; WebSocket carries one
//! JSON object per message; TCP uses NDJSON or a four-byte big-endian length.

use serde_json::{Map, Value};

use crate::{
    call::{encode_length_prefixed, split_length_prefixed, Transport, MAX_FRAME_BYTES},
    schema::{validate_rpc_call, validate_rpc_receipt, SchemaError},
};

include!("rpc_v1/types.rs");
include!("rpc_v1/receipt.rs");
include!("rpc_v1/decode.rs");
include!("rpc_v1/helpers.rs");
#[cfg(test)]
include!("rpc_v1/tests.rs");
