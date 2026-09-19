//! The telemetry seam. ores-otel plugs in here; this module never imports it.
//!
//! # Direction of the dependency
//!
//! Same arrow as the opto-sync seam, and for the same reason: an application
//! depends on `ores-otel`, and hands this module something that satisfies
//! [`RpcTelemetrySink`]. Nothing here links an OTel SDK, installs a global
//! provider, owns exporter shutdown, or decides sampling. That mirrors what
//! `opto-sync-clients/clients/rust/src/telemetry.rs` already does with
//! `ProtocolSyncTelemetrySink` -- one seam shape across the stack, so an
//! application writes one adapter and points both at it.
//!
//! # Fail-open, always
//!
//! Telemetry that can break a call is worse than no telemetry. Every emit is
//! wrapped: a sink that returns an error or panics changes nothing about the
//! RPC, and the failure is swallowed at this boundary.
//!
//! # What is deliberately absent
//!
//! No request body, no response body, no path parameter values, no `meta`
//! contents. An RPC payload is the caller's data and a route map cannot know
//! which fields are sensitive, so none of it crosses this boundary. The
//! operation key, the transport, and the outcome are enough to build latency
//! and error-rate signals; anything richer belongs to the application, which
//! knows what it is allowed to record.
//!
//! # Error paths: log, then re-raise
//!
//! [`RpcErrorEvent`] is the second shape this seam carries. A dispatcher that
//! fails emits one of these and then returns the failure -- or, when a handler
//! unwound, `resume_unwind`s the original payload. Logging is never a
//! substitute for propagating: an observed error still reaches the caller
//! unchanged, and a panic is never quietly downgraded into an error receipt.
//!
//! Every error event carries a `ores_trace_id`: a static `ores-trace-` literal
//! written at the call site that failed, so a line in a log names one exact
//! branch of one exact function rather than a shared message string. It is
//! `&'static str` on purpose -- a value assembled at runtime cannot be that.
//! It is unrelated to [`RpcErrorEvent::key`]'s W3C `trace_id` cousin on
//! [`RpcEvent`], which is a propagated distributed-tracing id.
//!
//! Error emission also carries a closed [`RpcLayer`] classification alongside
//! the payload. It is deliberately not inferred from `code`, `kind`, or the
//! static trace id: those are identities/classifications with different jobs,
//! and using them as a proxy would turn a logging convention into hidden
//! correlation semantics. Existing sinks remain source-compatible through the
//! legacy [`RpcTelemetrySink::emit_error`] callback; sinks that serialize the
//! fleet `RpcErrorLogEvent` contract override
//! [`RpcTelemetrySink::emit_error_with_layer`] and receive the required layer
//! explicitly.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// How the call was carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    Http,
    WebSocket,
    Tcp,
    /// Written to the opto-sync queue rather than sent.
    Queue,
}

impl Carrier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::WebSocket => "websocket",
            Self::Tcp => "tcp",
            Self::Queue => "queue",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// The peer answered, and the answer was a failure.
    Failed,
    /// The call never reached a peer.
    TransportError,
    /// Queued locally; the authoritative result arrives later through sync.
    Queued,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::TransportError => "transport_error",
            Self::Queued => "queued",
        }
    }
}

/// The seam that observed an RPC failure.
///
/// This mirrors the closed `rpc_layer` vocabulary in the fleet error-log
/// contract. It is a classification dimension, not a correlation or dedupe
/// key: the same failure can legitimately be observed at more than one layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcLayer {
    Handler,
    Dispatch,
    Transport,
    Client,
}

impl RpcLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Handler => "handler",
            Self::Dispatch => "dispatch",
            Self::Transport => "transport",
            Self::Client => "client",
        }
    }
}

/// One completed call, reduced to what is safe to record everywhere.
#[derive(Clone, Debug)]
pub struct RpcEvent<'a> {
    /// Operation key from the route map. Low cardinality by construction --
    /// safe as a metric label, which a path with an id in it is not.
    pub key: &'a str,
    pub service: &'a str,
    pub method: &'a str,
    /// The route map's path *template*, never the substituted path: the
    /// template has no customer identifiers in it.
    pub path_template: &'a str,
    pub carrier: Carrier,
    pub outcome: Outcome,
    pub duration_micros: u64,
    /// Failure code when `outcome` is not `Ok`. An HTTP status or a slug.
    pub code: Option<&'a str>,
    /// Frame correlation id, for stitching a client call to a server span on
    /// a framed transport. Absent over HTTP.
    pub correlation_id: Option<&'a str>,
    /// W3C trace context, if the caller is already inside a trace. This module
    /// neither creates nor propagates it -- it passes through what it is given.
    pub trace_id: Option<&'a str>,
    pub span_id: Option<&'a str>,
}

/// Where a failure was observed, coarse enough to stay low cardinality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// An envelope, a typed request section, or a response body did not
    /// decode. The bytes that failed are *not* carried.
    Decode,
    /// The transport refused the call: size, unknown key, carrier admission,
    /// correlation mismatch.
    Protocol,
    /// The operation ran and answered with its own declared error type.
    Operation,
    /// A handler unwound across the dispatch boundary. The event is emitted
    /// before the payload is re-raised, never instead of re-raising it.
    Panic,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::Protocol => "protocol",
            Self::Operation => "operation",
            Self::Panic => "panic",
        }
    }
}

/// One observed failure, reduced to what is safe to record everywhere.
///
/// Strictly narrower than [`RpcEvent`]: there is no message, no detail string
/// and no body, because an error message is the one field most likely to have
/// interpolated a customer identifier, a row, or a decoder's view of the input.
/// A stable `code` plus the static [`RpcErrorEvent::ores_trace_id`] identify
/// the branch precisely without quoting anything the caller sent. The emitting
/// seam is supplied separately as [`RpcLayer`] so adding the required fleet
/// classification does not break every existing struct literal.
#[derive(Clone, Copy, Debug)]
pub struct RpcErrorEvent<'a> {
    /// Operation key from the route map, or `""` when the failure happened
    /// before a key could be read. Low cardinality by construction.
    pub key: &'a str,
    pub carrier: Carrier,
    pub outcome: Outcome,
    pub kind: ErrorKind,
    /// Stable failure slug -- `invalid_rpc_envelope`, `unknown_rpc_key`,
    /// `handler_panicked`. Chosen from a closed set in the emitting code, never
    /// derived from input.
    pub code: &'a str,
    /// The static `ores-trace-` literal of the failing call site.
    pub ores_trace_id: &'static str,
}

/// Application-owned adapter seam. Back it with `ores-otel` / `next-loggers`.
///
/// [`RpcTelemetrySink::emit_error`] defaults to doing nothing, so an adapter
/// written before error events existed still compiles and still reports
/// successful calls. New adapters that serialize the fleet error-log contract
/// should override [`RpcTelemetrySink::emit_error_with_layer`]; its default
/// delegates to the legacy callback so old adapters continue to work.
pub trait RpcTelemetrySink: Send + Sync {
    fn emit(&self, event: &RpcEvent<'_>) -> Result<(), String>;

    fn emit_error(&self, event: &RpcErrorEvent<'_>) -> Result<(), String> {
        let _ = event;
        Ok(())
    }

    fn emit_error_with_layer(
        &self,
        layer: RpcLayer,
        event: &RpcErrorEvent<'_>,
    ) -> Result<(), String> {
        let _ = layer;
        self.emit_error(event)
    }
}

impl<F> RpcTelemetrySink for F
where
    F: Fn(&RpcEvent<'_>) -> Result<(), String> + Send + Sync,
{
    fn emit(&self, event: &RpcEvent<'_>) -> Result<(), String> {
        self(event)
    }
}

/// Deliver one event without letting it affect the call.
///
/// A missing sink, a sink error, and a sink panic are all the same outcome
/// here: nothing happens and the RPC is unaffected.
pub fn emit(sink: Option<&dyn RpcTelemetrySink>, event: RpcEvent<'_>) {
    let Some(sink) = sink else { return };
    let _ = catch_unwind(AssertUnwindSafe(|| sink.emit(&event)));
}

fn emit_error_at(sink: Option<&dyn RpcTelemetrySink>, layer: RpcLayer, event: RpcErrorEvent<'_>) {
    let Some(sink) = sink else { return };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        sink.emit_error_with_layer(layer, &event)
    }));
}

/// Deliver a transport-layer error event without letting it affect the failure
/// it describes.
///
/// This is the historical helper imported directly by the Axum transport. Its
/// layer is therefore explicit here rather than guessed by adapters from the
/// error code or kind.
pub fn emit_error(sink: Option<&dyn RpcTelemetrySink>, event: RpcErrorEvent<'_>) {
    emit_error_at(sink, RpcLayer::Transport, event);
}

/// Deliver a dispatch-layer error event.
///
/// The crate root re-exports this as `emit_rpc_error_event`, which is the seam
/// generated dispatch support already imports. Keeping the layer at that
/// boundary avoids changing the positional generated call surface merely to
/// teach a logging adapter where the event originated.
pub fn emit_dispatch_error(sink: Option<&dyn RpcTelemetrySink>, event: RpcErrorEvent<'_>) {
    emit_error_at(sink, RpcLayer::Dispatch, event);
}

/// Deliver an explicitly classified error event for handler/client adapters.
pub fn emit_error_with_layer(
    sink: Option<&dyn RpcTelemetrySink>,
    layer: RpcLayer,
    event: RpcErrorEvent<'_>,
) {
    emit_error_at(sink, layer, event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn event<'a>(key: &'a str) -> RpcEvent<'a> {
        RpcEvent {
            key,
            service: "demo",
            method: "POST",
            path_template: "/v1/matters/{id}/walk",
            carrier: Carrier::WebSocket,
            outcome: Outcome::Ok,
            duration_micros: 1234,
            code: None,
            correlation_id: Some("c7-1"),
            trace_id: None,
            span_id: None,
        }
    }

    #[test]
    fn no_sink_is_not_an_error() {
        emit(None, event("walk_matter"));
    }

    #[test]
    fn a_panicking_sink_cannot_break_the_call() {
        struct Boom;
        impl RpcTelemetrySink for Boom {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                panic!("exporter is down");
            }
        }
        emit(Some(&Boom), event("walk_matter"));
    }

    #[test]
    fn a_failing_sink_is_swallowed() {
        emit(
            Some(&(|_: &RpcEvent<'_>| Err("queue full".to_string()))),
            event("healthz"),
        );
    }

    fn error_event<'a>(key: &'a str) -> RpcErrorEvent<'a> {
        RpcErrorEvent {
            key,
            carrier: Carrier::Http,
            outcome: Outcome::Failed,
            kind: ErrorKind::Decode,
            code: "body_decode_failed",
            ores_trace_id: "ores-trace-vMFIZbtPBYe4jTjOD0pJW",
        }
    }

    #[test]
    fn layer_wire_values_match_the_fleet_contract() {
        assert_eq!(RpcLayer::Handler.as_str(), "handler");
        assert_eq!(RpcLayer::Dispatch.as_str(), "dispatch");
        assert_eq!(RpcLayer::Transport.as_str(), "transport");
        assert_eq!(RpcLayer::Client.as_str(), "client");
    }

    #[test]
    fn no_sink_swallows_an_error_event_too() {
        emit_error(None, error_event("walk_matter"));
    }

    #[test]
    fn an_adapter_written_before_layered_error_events_still_compiles() {
        struct SuccessOnly;
        impl RpcTelemetrySink for SuccessOnly {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                Ok(())
            }
            fn emit_error(&self, _: &RpcErrorEvent<'_>) -> Result<(), String> {
                Ok(())
            }
        }
        emit_error(Some(&SuccessOnly), error_event("walk_matter"));
    }

    #[test]
    fn a_panicking_error_sink_cannot_replace_the_failure_it_describes() {
        struct Boom;
        impl RpcTelemetrySink for Boom {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                Ok(())
            }
            fn emit_error_with_layer(
                &self,
                _: RpcLayer,
                _: &RpcErrorEvent<'_>,
            ) -> Result<(), String> {
                panic!("exporter is down");
            }
        }
        emit_error(Some(&Boom), error_event("walk_matter"));
    }

    #[test]
    fn transport_and_dispatch_helpers_supply_literal_layers() {
        struct Recorder(std::sync::Mutex<Vec<RpcLayer>>);
        impl RpcTelemetrySink for Recorder {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                Ok(())
            }
            fn emit_error_with_layer(
                &self,
                layer: RpcLayer,
                _: &RpcErrorEvent<'_>,
            ) -> Result<(), String> {
                self.0.lock().expect("recorder").push(layer);
                Ok(())
            }
        }
        let recorder = Recorder(std::sync::Mutex::new(Vec::new()));
        emit_error(Some(&recorder), error_event("walk_matter"));
        emit_dispatch_error(Some(&recorder), error_event("walk_matter"));
        assert_eq!(
            *recorder.0.lock().expect("recorder"),
            vec![RpcLayer::Transport, RpcLayer::Dispatch]
        );
    }

    #[test]
    fn an_error_event_reaches_the_legacy_sink_with_its_static_id() {
        struct Recorder(std::sync::Mutex<Vec<(String, String, &'static str)>>);
        impl RpcTelemetrySink for Recorder {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                Ok(())
            }
            fn emit_error(&self, event: &RpcErrorEvent<'_>) -> Result<(), String> {
                self.0.lock().expect("recorder").push((
                    event.key.to_owned(),
                    event.code.to_owned(),
                    event.ores_trace_id,
                ));
                Ok(())
            }
        }
        let recorder = Recorder(std::sync::Mutex::new(Vec::new()));
        emit_error(Some(&recorder), error_event("walk_matter"));
        let seen = recorder.0.lock().expect("recorder").clone();
        assert_eq!(
            seen,
            vec![(
                "walk_matter".to_owned(),
                "body_decode_failed".to_owned(),
                "ores-trace-vMFIZbtPBYe4jTjOD0pJW",
            )]
        );
    }

    #[test]
    fn an_error_event_has_no_field_that_could_hold_a_payload() {
        let rendered = format!("{:?}", error_event("walk_matter"));
        assert_eq!(
            rendered,
            "RpcErrorEvent { key: \"walk_matter\", carrier: Http, outcome: Failed, \
             kind: Decode, code: \"body_decode_failed\", \
             ores_trace_id: \"ores-trace-vMFIZbtPBYe4jTjOD0pJW\" }"
        );
    }

    #[test]
    fn a_closure_is_a_sink() {
        static SEEN: AtomicUsize = AtomicUsize::new(0);
        let sink = |e: &RpcEvent<'_>| {
            assert_eq!(e.key, "healthz");
            SEEN.fetch_add(1, Ordering::SeqCst);
            Ok(())
        };
        emit(Some(&sink), event("healthz"));
        assert_eq!(SEEN.load(Ordering::SeqCst), 1);
    }
}
