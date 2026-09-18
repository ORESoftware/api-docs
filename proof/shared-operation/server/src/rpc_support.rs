//! What generated `routes/**/rpc.rs` dispatch compiles against.
//!
//! # The contract every failure path here keeps
//!
//! Log through the seam with the call site's static `ores-trace-` id, then
//! re-raise. "Re-raise" is literal: a decode failure still becomes the same
//! failure receipt, an operation error still becomes the same error receipt,
//! and a handler that panicked is `resume_unwind`-ed with its original payload
//! rather than quietly turned into a 400. A panic is a bug in the handler; an
//! error receipt is a fact about the request, and converting the first into the
//! second is how a broken service goes on looking healthy.
//!
//! The id arrives from the generated dispatch as `rpc_trace_id: &'static str`,
//! so an operation's failures are attributable to that operation's own dispatch
//! without this shared helper inventing an id at runtime.
//!
//! Nothing that crosses the seam is derived from the request: the operation
//! key, the carrier, the outcome, a fixed error kind, a fixed slug, and the id.
//! Serde's decode messages quote the input they rejected, so they go to the
//! caller in the receipt and no further.

use std::{
    future::{poll_fn, Future},
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    pin::pin,
    sync::atomic::Ordering,
    task::Poll,
};

use ores_api_docs::{
    emit_rpc_error_event, OperationContext, OperationInvokeError, OperationRequestData,
    OperationSpec, OptionalJson, RpcErrorEvent, RpcPayloadCodec, RpcTelemetryCarrier,
    RpcTelemetryErrorKind, RpcTelemetryOutcome, RpcV1Call, RpcV1HttpContext, RpcV1Receipt,
    TypedOperationContext,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use crate::{model::OperationEnvelope, state::AppState};

pub trait ProofTraceEnvelope {
    fn prepend_proof_trace(&mut self, trace_id: &'static str);
}

impl<T> ProofTraceEnvelope for OperationEnvelope<T> {
    fn prepend_proof_trace(&mut self, trace_id: &'static str) {
        self.prepend_trace_id(trace_id);
    }
}

pub async fn invoke_json_rpc<O, Invoke, Fut>(
    state: AppState,
    http_context: RpcV1HttpContext,
    call: RpcV1Call,
    rpc_trace_id: &'static str,
    invoke: Invoke,
) -> RpcV1Receipt
where
    O: OperationSpec,
    O::Path: DeserializeOwned,
    O::Query: DeserializeOwned,
    O::RequestHeaders: DeserializeOwned,
    O::RequestBody: DeserializeOwned,
    O::ResponseBody: Serialize + ProofTraceEnvelope,
    O::Error: Serialize,
    Invoke: FnOnce(TypedOperationContext<AppState, O>) -> Fut,
    Fut: Future<Output = Result<O::ResponseBody, OperationInvokeError<O::Error>>>,
{
    state
        .counters
        .rpc_adapter_hits
        .fetch_add(1, Ordering::SeqCst);

    let request = OperationRequestData::new(RpcPayloadCodec::Json);
    let path = match decode_object_or_null::<O::Path>(call.path.as_ref()) {
        Ok(value) => value,
        Err(message) => {
            return decode_failure(&state, &call, "path_decode_failed", message, rpc_trace_id)
        }
    };
    let query = match decode_object_or_null::<O::Query>(call.query.as_ref()) {
        Ok(value) => value,
        Err(message) => {
            return decode_failure(&state, &call, "query_decode_failed", message, rpc_trace_id)
        }
    };
    let headers = match decode_object_or_null::<O::RequestHeaders>(call.headers.as_ref()) {
        Ok(value) => value,
        Err(message) => {
            return decode_failure(
                &state,
                &call,
                "headers_decode_failed",
                message,
                rpc_trace_id,
            )
        }
    };
    let body = match serde_json::from_value::<O::RequestBody>(
        call.body.value().cloned().unwrap_or(Value::Null),
    ) {
        Ok(value) => value,
        Err(error) => {
            return decode_failure(
                &state,
                &call,
                "body_decode_failed",
                error.to_string(),
                rpc_trace_id,
            )
        }
    };

    request.insert_path::<O>(path);
    request.insert_query::<O>(query);
    request.insert_headers::<O>(headers);
    request.insert_body::<O>(body);
    request.set_semantic_input(serde_json::json!({
        "path": call.path,
        "query": call.query,
        "headers": call.headers,
        "body": call.body.value(),
    }));

    let base = OperationContext::rpc(state.clone(), http_context).with_policy(state.policy.clone());
    let ctx = TypedOperationContext::<AppState, O>::new(base, request);

    match invoke_guarded(invoke, ctx).await {
        Ok(Ok(mut output)) => {
            output.prepend_proof_trace(rpc_trace_id);
            match serde_json::to_value(output) {
                Ok(value) => {
                    let mut receipt = RpcV1Receipt::success(
                        call.id.clone(),
                        call.key.clone(),
                        OptionalJson::present(value),
                    );
                    receipt.status = Some(200);
                    receipt.trace_id = call.trace_id.clone();
                    receipt.span_id = call.span_id.clone();
                    receipt
                }
                Err(error) => decode_failure(
                    &state,
                    &call,
                    "response_encode_failed",
                    error.to_string(),
                    rpc_trace_id,
                ),
            }
        }
        Ok(Err(error)) => {
            // The operation answered with its declared error type. Record that
            // it happened, then return exactly the receipt this always
            // returned -- the error itself is untouched on its way out.
            emit_rpc_failure(
                &state,
                &call,
                RpcTelemetryErrorKind::Operation,
                "operation_error",
                rpc_trace_id,
            );

            let value = serde_json::to_value(error).unwrap_or_else(|encode_error| {
                serde_json::json!({
                    "code":"operation_error_encode_failed",
                    "message":encode_error.to_string()
                })
            });
            let mut object = value
                .as_object()
                .cloned()
                .unwrap_or_else(|| Map::from_iter([("detail".into(), value)]));
            object
                .entry("code".to_owned())
                .or_insert_with(|| Value::String("operation_error".into()));
            let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), 400, object);
            receipt.trace_id = call.trace_id.clone();
            receipt.span_id = call.span_id.clone();
            receipt
        }
        Err(payload) => {
            // A handler unwound across the dispatch boundary. Say so with this
            // dispatch's id, and then put the panic back exactly as it was:
            // the payload, its message, and the abort/hook behaviour the
            // process is configured for all stay the caller's to deal with.
            emit_rpc_failure(
                &state,
                &call,
                RpcTelemetryErrorKind::Panic,
                "handler_panicked",
                rpc_trace_id,
            );
            resume_unwind(payload);
        }
    }
}

/// Run the handler so a panic is observable without being absorbed.
///
/// Two places can unwind and both are guarded: building the future (the
/// `FnOnce` body up to the first `await`) and every later poll. A panic is
/// returned as the payload so the caller can log it and `resume_unwind` --
/// this function never decides what a panic means.
async fn invoke_guarded<O, Invoke, Fut>(
    invoke: Invoke,
    ctx: TypedOperationContext<AppState, O>,
) -> Result<Result<O::ResponseBody, OperationInvokeError<O::Error>>, Box<dyn std::any::Any + Send>>
where
    O: OperationSpec,
    Invoke: FnOnce(TypedOperationContext<AppState, O>) -> Fut,
    Fut: Future<Output = Result<O::ResponseBody, OperationInvokeError<O::Error>>>,
{
    let future = catch_unwind(AssertUnwindSafe(|| invoke(ctx)))?;
    let mut future = pin!(future);
    poll_fn(|cx| {
        match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            // A future that panicked must not be polled again, and it is not:
            // this resolves, and the caller re-raises.
            Err(payload) => Poll::Ready(Err(payload)),
        }
    })
    .await
}

fn decode_object_or_null<T>(value: Option<&Map<String, Value>>) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.cloned().map(Value::Object).unwrap_or(Value::Null))
        .map_err(|error| error.to_string())
}

/// Build the failure receipt this always built, and log that it happened.
///
/// `message` is the decoder's, and decoders quote what they rejected. It goes
/// into the receipt the caller receives -- their own input, returned to them --
/// and it is not passed to [`emit_rpc_failure`].
fn decode_failure(
    state: &AppState,
    call: &RpcV1Call,
    code: &'static str,
    message: String,
    rpc_trace_id: &'static str,
) -> RpcV1Receipt {
    emit_rpc_failure(
        state,
        call,
        RpcTelemetryErrorKind::Decode,
        code,
        rpc_trace_id,
    );

    let error = Map::from_iter([
        ("code".into(), Value::String(code.to_owned())),
        ("message".into(), Value::String(message)),
    ]);
    let mut receipt = RpcV1Receipt::failure(call.id.clone(), call.key.clone(), 400, error);
    receipt.trace_id = call.trace_id.clone();
    receipt.span_id = call.span_id.clone();
    receipt
}

/// The single point where a dispatch failure crosses the telemetry seam.
///
/// Fail-open all the way through: no sink is a no-op, and a sink that panics
/// is contained inside `emit_rpc_error_event`, so nothing here can change the
/// receipt that is about to be returned or the panic that is about to be
/// re-raised.
fn emit_rpc_failure(
    state: &AppState,
    call: &RpcV1Call,
    kind: RpcTelemetryErrorKind,
    code: &'static str,
    rpc_trace_id: &'static str,
) {
    emit_rpc_error_event(
        state.telemetry.as_deref(),
        RpcErrorEvent {
            key: &call.key,
            carrier: RpcTelemetryCarrier::Http,
            outcome: RpcTelemetryOutcome::Failed,
            kind,
            code,
            ores_trace_id: rpc_trace_id,
        },
    );
}

#[cfg(test)]
mod tests {
    use std::{future::Ready, sync::Arc, sync::Mutex};

    use http::HeaderMap;
    use ores_api_docs::{RpcEvent, RpcTelemetrySink};

    use super::*;
    use crate::model::{CreateUserOperation, ProofError, User};

    /// A dispatch id of the shape the generator emits, used only by these
    /// tests so an assertion can name the exact literal it expects.
    const TEST_RPC_TRACE_ID: &str = "ores-trace-fNnvkBkoN-NTrQTEC6-0r";

    #[derive(Default)]
    struct Recorder {
        errors: Mutex<Vec<String>>,
        codes: Mutex<Vec<(String, String, String, String)>>,
    }

    impl RpcTelemetrySink for Recorder {
        fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
            Ok(())
        }

        fn emit_error(&self, event: &RpcErrorEvent<'_>) -> Result<(), String> {
            self.errors
                .lock()
                .expect("recorder")
                .push(format!("{event:?}"));
            self.codes.lock().expect("recorder").push((
                event.key.to_owned(),
                event.kind.as_str().to_owned(),
                event.code.to_owned(),
                event.ores_trace_id.to_owned(),
            ));
            Ok(())
        }
    }

    impl Recorder {
        fn only_event(&self) -> (String, String, String, String) {
            let codes = self.codes.lock().expect("recorder");
            assert_eq!(
                codes.len(),
                1,
                "expected exactly one error event: {codes:?}"
            );
            codes[0].clone()
        }

        fn rendered(&self) -> String {
            self.errors.lock().expect("recorder").join("\n")
        }
    }

    fn observed() -> (Arc<Recorder>, AppState) {
        let recorder = Arc::new(Recorder::default());
        let sink: Arc<dyn RpcTelemetrySink> = recorder.clone();
        (recorder, AppState::with_telemetry(Some(sink)))
    }

    fn http_context() -> RpcV1HttpContext {
        RpcV1HttpContext::from_headers(HeaderMap::new())
    }

    /// A well-formed create_user call. The values in it are what the
    /// no-payload assertions look for in the recorded events.
    fn well_formed_call() -> RpcV1Call {
        let mut call = RpcV1Call::new("call-under-test", CreateUserOperation::KEY);
        call.headers = Some(Map::from_iter([(
            "x-ores-tenant".to_owned(),
            Value::String("tenant-secret-9".to_owned()),
        )]));
        call.body = OptionalJson::present(serde_json::json!({
            "id": "user-secret-9",
            "display_name": "Display Secret 9"
        }));
        call
    }

    async fn failing_handler(
        _ctx: TypedOperationContext<AppState, CreateUserOperation>,
    ) -> Result<OperationEnvelope<User>, OperationInvokeError<ProofError>> {
        Err(OperationInvokeError::Operation(ProofError {
            code: "user_not_found".into(),
            message: "user user-secret-9 was not found".into(),
        }))
    }

    async fn panicking_handler(
        _ctx: TypedOperationContext<AppState, CreateUserOperation>,
    ) -> Result<OperationEnvelope<User>, OperationInvokeError<ProofError>> {
        panic!("handler exploded on user-secret-9");
    }

    /// Panics where `panicking_handler` cannot: in the `FnOnce` body, before
    /// there is a future to poll at all.
    fn panicking_builder(
        _ctx: TypedOperationContext<AppState, CreateUserOperation>,
    ) -> Ready<Result<OperationEnvelope<User>, OperationInvokeError<ProofError>>> {
        panic!("dispatch exploded before the first poll");
    }

    fn without_panic_output<R>(body: impl FnOnce() -> R) -> std::thread::Result<R> {
        // The default hook would print a backtrace for a panic the test is
        // asserting on, which makes a passing run look like a failing one.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = catch_unwind(AssertUnwindSafe(body));
        std::panic::set_hook(previous);
        outcome
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    }

    #[tokio::test]
    async fn a_failing_handler_logs_one_event_and_the_error_still_propagates() {
        let (recorder, state) = observed();
        let receipt = invoke_json_rpc::<CreateUserOperation, _, _>(
            state,
            http_context(),
            well_formed_call(),
            TEST_RPC_TRACE_ID,
            failing_handler,
        )
        .await;

        assert_eq!(
            recorder.only_event(),
            (
                "demo.users.create_user".to_owned(),
                "operation".to_owned(),
                "operation_error".to_owned(),
                TEST_RPC_TRACE_ID.to_owned(),
            )
        );

        // Re-raised: the receipt is byte-for-byte the one this path produced
        // before the logging existed -- the externally tagged `Operation`
        // variant, with the operation's own code and message inside it.
        assert!(!receipt.ok);
        assert_eq!(receipt.id, "call-under-test");
        let error = receipt.error.expect("error receipt");
        assert_eq!(error["code"], Value::String("operation_error".into()));
        assert_eq!(error["kind"], Value::String("operation".into()));
        assert_eq!(
            error["detail"]["code"],
            Value::String("user_not_found".into())
        );
        assert_eq!(
            error["detail"]["message"],
            Value::String("user user-secret-9 was not found".into())
        );
    }

    #[tokio::test]
    async fn a_decode_failure_logs_one_event_and_still_answers_with_the_receipt() {
        let (recorder, state) = observed();
        let mut call = well_formed_call();
        call.body = OptionalJson::present(serde_json::json!({"id": 17}));

        let receipt = invoke_json_rpc::<CreateUserOperation, _, _>(
            state,
            http_context(),
            call,
            TEST_RPC_TRACE_ID,
            failing_handler,
        )
        .await;

        assert_eq!(
            recorder.only_event(),
            (
                "demo.users.create_user".to_owned(),
                "decode".to_owned(),
                "body_decode_failed".to_owned(),
                TEST_RPC_TRACE_ID.to_owned(),
            )
        );

        assert!(!receipt.ok);
        let error = receipt.error.expect("error receipt");
        assert_eq!(error["code"], Value::String("body_decode_failed".into()));
        // The decoder's message reaches the caller, as it always did.
        assert!(error["message"].as_str().expect("message").contains("17"));
        // And it goes no further.
        assert!(!recorder.rendered().contains("17"));
    }

    #[test]
    fn a_panicking_handler_is_logged_once_and_re_raised() {
        let (recorder, state) = observed();
        let rt = runtime();

        let outcome = without_panic_output(|| {
            rt.block_on(invoke_json_rpc::<CreateUserOperation, _, _>(
                state,
                http_context(),
                well_formed_call(),
                TEST_RPC_TRACE_ID,
                panicking_handler,
            ))
        });

        let payload = outcome.expect_err("the panic must not become a receipt");
        assert_eq!(
            payload.downcast_ref::<&str>().copied(),
            Some("handler exploded on user-secret-9"),
            "the original payload is re-raised, not a replacement"
        );

        assert_eq!(
            recorder.only_event(),
            (
                "demo.users.create_user".to_owned(),
                "panic".to_owned(),
                "handler_panicked".to_owned(),
                TEST_RPC_TRACE_ID.to_owned(),
            )
        );
    }

    #[test]
    fn a_panic_before_the_first_poll_is_also_logged_and_re_raised() {
        let (recorder, state) = observed();
        let rt = runtime();

        let outcome = without_panic_output(|| {
            rt.block_on(invoke_json_rpc::<CreateUserOperation, _, _>(
                state,
                http_context(),
                well_formed_call(),
                TEST_RPC_TRACE_ID,
                panicking_builder,
            ))
        });

        let payload = outcome.expect_err("the panic must not become a receipt");
        assert_eq!(
            payload.downcast_ref::<&str>().copied(),
            Some("dispatch exploded before the first poll")
        );
        assert_eq!(recorder.only_event().2, "handler_panicked");
    }

    #[test]
    fn no_event_carries_request_data() {
        let (recorder, state) = observed();
        let rt = runtime();

        let _ = without_panic_output(|| {
            rt.block_on(invoke_json_rpc::<CreateUserOperation, _, _>(
                state,
                http_context(),
                well_formed_call(),
                TEST_RPC_TRACE_ID,
                panicking_handler,
            ))
        });

        let rendered = recorder.rendered();
        for secret in [
            "user-secret-9",
            "Display Secret 9",
            "tenant-secret-9",
            "x-ores-tenant",
            "call-under-test",
            "exploded",
        ] {
            assert!(
                !rendered.contains(secret),
                "{secret} crossed the telemetry seam: {rendered}"
            );
        }
        // What it does carry: the key, the kind, the slug, the id.
        assert!(rendered.contains("demo.users.create_user"));
        assert!(rendered.contains(TEST_RPC_TRACE_ID));
    }

    #[tokio::test]
    async fn a_panicking_sink_cannot_turn_a_failure_into_something_else() {
        struct Boom;
        impl RpcTelemetrySink for Boom {
            fn emit(&self, _: &RpcEvent<'_>) -> Result<(), String> {
                Ok(())
            }
            fn emit_error(&self, _: &RpcErrorEvent<'_>) -> Result<(), String> {
                panic!("exporter is down");
            }
        }

        let sink: Arc<dyn RpcTelemetrySink> = Arc::new(Boom);
        let observed = invoke_json_rpc::<CreateUserOperation, _, _>(
            AppState::with_telemetry(Some(sink)),
            http_context(),
            well_formed_call(),
            TEST_RPC_TRACE_ID,
            failing_handler,
        )
        .await;
        let unobserved = invoke_json_rpc::<CreateUserOperation, _, _>(
            AppState::new(),
            http_context(),
            well_formed_call(),
            TEST_RPC_TRACE_ID,
            failing_handler,
        )
        .await;

        assert_eq!(
            observed.encode().expect("receipt"),
            unobserved.encode().expect("receipt")
        );
    }

    #[test]
    fn every_generated_dispatch_passes_a_well_formed_static_id() {
        // The generated rpc.rs files are the call sites that own these ids.
        // They are literals in the source, which is what makes them static.
        let sources = [
            include_str!("routes/v1/users/rpc.rs"),
            include_str!("routes/v1/users/[user_id]/rpc.rs"),
        ];
        let mut ids: Vec<&str> = Vec::new();
        for source in sources {
            for (start, _) in source.match_indices("\"ores-trace-") {
                let rest = &source[start + 1..];
                let end = rest.find('"').expect("a string literal");
                ids.push(&rest[..end]);
            }
        }
        assert_eq!(ids.len(), 3, "one id per generated dispatch: {ids:?}");
        for id in &ids {
            assert!(id.starts_with("ores-trace-"), "{id}");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "{id}"
            );
        }
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "a dispatch id is reused");
    }
}
