//! Compiles the reference transports against a real generated client.
//!
//! This mirrors the layout a consumer uses: the generated module at the crate
//! root, and the runtime modules vendored beside it so their `crate::` paths
//! resolve to that service's own operations. See `../README.md`.

#[path = "../../../examples/generated/rust/generated/demo/ridl_generated.rs"]
mod generated;

pub use generated::*;

#[path = "../opto_sync.rs"]
pub mod opto_sync;

#[path = "../transports.rs"]
pub mod transports;

pub use opto_sync::{
    DirectTransport, LocalReadback, MutationQueue, OptoSyncTransport, OptoTransportError,
};
pub use transports::{
    Frame, MultiTransport, NoTelemetry, Outcome, TelemetrySink, TransportError, TransportKind,
    Wire,
};
