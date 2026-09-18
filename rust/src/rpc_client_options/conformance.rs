//! Cross-language chain conformance.
//!
//! A chain is an ordered list of catalog options with concrete arguments. The
//! Rust builder replays each chain and records the resulting plan; every other
//! language client replays the same chain and must produce byte-identical JSON.
//! That makes "the clients agree" a checked fact rather than a claim in prose.
//!
//! Chains are authored here rather than derived, because the interesting cases
//! are orderings and interactions that a per-option sweep would never produce.

use crate::rpc_fluent::{
    Backpressure, Compression, QueuePriority, SerialStrategy, StreamCall, UnaryCall,
};
use serde_json::{json, Value};

#[derive(Debug, serde::Serialize)]
pub struct ChainCase {
    pub chain_id: String,
    pub surface: &'static str,
    pub key: &'static str,
    /// Replay steps: `[method_id, ...args]`, using catalog option ids.
    pub steps: Vec<Value>,
    pub rationale: &'static str,
    /// The plan Rust produced. Other languages must match this exactly.
    pub plan: Value,
}

const UNARY_KEY: &str = "demo.users.find_user";
const STREAM_KEY: &str = "demo.events.watch_events";
const RPC_PATH: &str = "/v1/rpc";

pub fn cases() -> Vec<ChainCase> {
    let mut cases = Vec::new();

    cases.push(ChainCase {
        chain_id: "unary.defaults".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![],
        rationale: "An unconfigured unary chain still states its strategy explicitly.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH).to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "unary.resilience".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![
            json!(["with_timeout", 2500]),
            json!(["with_retries", 3]),
            json!(["with_retry_backoff", 100, 2.0]),
            json!(["on_retry"]),
        ],
        rationale: "Retry budget, backoff schedule and a hook coexist; the hook is a count.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH)
            .with_timeout(2500)
            .with_retries(3)
            .with_retry_backoff(100, 2.0)
            .on_retry(|_, _| {})
            .to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "unary.serialization_then_everything_else".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![
            json!(["use_message_pack"]),
            json!(["omit_auth"]),
            json!(["force_ipv6"]),
            json!(["queue_priority", 3]),
            json!(["compress", "gzip"]),
            json!(["concurrency_key", "user-reads"]),
            json!(["add_path_field", "user_id", "user-42"]),
            json!(["add_query_field", "verbose", true]),
        ],
        rationale: "Spending three exclusive groups leaves the rest of the surface reachable.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH)
            .use_message_pack()
            .omit_auth()
            .force_ipv6()
            .queue_priority(QueuePriority::Three)
            .compress(Compression::Gzip)
            .concurrency_key("user-reads")
            .add_path_field("user_id", json!("user-42"))
            .add_query_field("verbose", json!(true))
            .to_plan(),
    });

    // Order must not matter: the same options applied in reverse must produce
    // the same plan, or the clients cannot be compared by bytes at all.
    cases.push(ChainCase {
        chain_id: "unary.order_independence".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![
            json!(["add_query_field", "verbose", true]),
            json!(["add_path_field", "user_id", "user-42"]),
            json!(["concurrency_key", "user-reads"]),
            json!(["compress", "gzip"]),
            json!(["queue_priority", 3]),
            json!(["force_ipv6"]),
            json!(["omit_auth"]),
            json!(["use_message_pack"]),
        ],
        rationale: "The reverse of unary.serialization_then_everything_else; plans must be equal.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH)
            .add_query_field("verbose", json!(true))
            .add_path_field("user_id", json!("user-42"))
            .concurrency_key("user-reads")
            .compress(Compression::Gzip)
            .queue_priority(QueuePriority::Three)
            .force_ipv6()
            .omit_auth()
            .use_message_pack()
            .to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "unary.credential_is_redacted".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![json!(["with_bearer_token", "super-secret-value"])],
        rationale: "auth_mode records the override; the plan carries a redaction, never the token.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH)
            .with_bearer_token("super-secret-value")
            .to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "unary.cache_and_freshness".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![
            json!(["with_cache_ttl", 60]),
            json!(["stale_while_revalidate", 300]),
            json!(["require_fresh"]),
            json!(["skip_cloudflare_cache"]),
        ],
        rationale: "Cache directives layer, and each contributes its documented headers.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH)
            .with_cache_ttl(60)
            .stale_while_revalidate(300)
            .require_fresh()
            .skip_cloudflare_cache()
            .to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "unary.observability".to_owned(),
        surface: "unary",
        key: UNARY_KEY,
        steps: vec![
            json!(["with_trace_id", "4bf92f3577b34da6a3ce929d0e0e4736"]),
            json!(["with_span_id", "00f067aa0ba902b7"]),
            json!(["dry_run"]),
            json!(["debug"]),
        ],
        rationale: "Trace propagation and dry-run are independent of each other.",
        plan: UnaryCall::new(UNARY_KEY, RPC_PATH)
            .with_trace_id("4bf92f3577b34da6a3ce929d0e0e4736")
            .with_span_id("00f067aa0ba902b7")
            .dry_run()
            .debug()
            .to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "stream.defaults".to_owned(),
        surface: "stream",
        key: STREAM_KEY,
        steps: vec![],
        rationale: "An unconfigured streaming chain declares the stream kind.",
        plan: StreamCall::new(STREAM_KEY, RPC_PATH).to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "stream.backpressure_and_shaping".to_owned(),
        surface: "stream",
        key: STREAM_KEY,
        steps: vec![
            json!(["use_json"]),
            json!(["with_backpressure", "drop_oldest"]),
            json!(["with_stream_buffer", 256]),
            json!(["with_stream_idle_timeout", 30000]),
            json!(["sample_each", 100]),
        ],
        rationale: "Inbound shaping, buffering and idle detection are separate knobs.",
        plan: StreamCall::new(STREAM_KEY, RPC_PATH)
            .use_json()
            .with_backpressure(Backpressure::DropOldest)
            .with_stream_buffer(256)
            .with_stream_idle_timeout(30_000)
            .sample_each(100)
            .to_plan(),
    });

    cases.push(ChainCase {
        chain_id: "stream.retry_and_transport".to_owned(),
        surface: "stream",
        key: STREAM_KEY,
        steps: vec![
            json!(["use_serial_strategy", "protobuf"]),
            json!(["with_retries", 2]),
            json!(["keep_alive", false]),
            json!(["via_proxy", "http://127.0.0.1:8080"]),
            json!(["queue_priority", 0]),
        ],
        rationale: "A stream can be retried and pinned to a transport path like a unary call.",
        plan: StreamCall::new(STREAM_KEY, RPC_PATH)
            .use_serial_strategy(SerialStrategy::Protobuf)
            .with_retries(2)
            .keep_alive(false)
            .via_proxy("http://127.0.0.1:8080")
            .queue_priority(QueuePriority::Zero)
            .to_plan(),
    });

    cases.sort_by(|a, b| a.chain_id.cmp(&b.chain_id));
    cases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_order_does_not_change_the_plan() {
        let all = cases();
        let forward = all
            .iter()
            .find(|c| c.chain_id == "unary.serialization_then_everything_else")
            .expect("forward chain");
        let reverse = all
            .iter()
            .find(|c| c.chain_id == "unary.order_independence")
            .expect("reverse chain");
        assert_eq!(
            forward.plan, reverse.plan,
            "applying the same options in reverse must produce the same plan"
        );
    }

    #[test]
    fn no_chain_plan_carries_a_credential() {
        for case in cases() {
            assert!(
                !case.plan.to_string().contains("super-secret-value"),
                "{} leaked a credential into its plan",
                case.chain_id
            );
        }
    }

    #[test]
    fn every_chain_plan_satisfies_the_authored_schema() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root");
        let schema: Value = serde_json::from_str(
            &std::fs::read_to_string(
                root.join(crate::rpc_client_options::AUTHORED_PLAN_SCHEMA_PATH),
            )
            .expect("authored plan schema is readable"),
        )
        .expect("authored plan schema is valid JSON");
        let validator = jsonschema::validator_for(&schema).expect("schema compiles");

        for case in cases() {
            let errors: Vec<String> = validator
                .iter_errors(&case.plan)
                .map(|error| error.to_string())
                .collect();
            assert!(
                errors.is_empty(),
                "{} produced a plan the authored schema rejects: {}",
                case.chain_id,
                errors.join("; ")
            );
        }
    }
}
