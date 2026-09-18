//! Type-state RPC call builders.
//!
//! Two surfaces, kept apart by type rather than by convention:
//!
//! * [`UnaryCall`] terminates in [`UnaryCall::make_call`] and has no `stream`;
//! * [`StreamCall`] terminates in [`StreamCall::stream`] and has no `make_call`.
//!
//! Each exclusive group in the option catalog is a type parameter that starts
//! at [`Unset`] and moves to [`Set`] when spent. The methods that spend a group
//! are implemented only for the `Unset` position, so calling one twice is a
//! compile error rather than a runtime assertion.
//!
//! The option methods themselves live in the generated sibling module; this
//! file owns the plan storage, the state markers, and the network boundary.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

pub use super::rpc_client_surface::{
    Backpressure, Compression, QueuePriority, SerialStrategy, CATALOG_VERSION, DEFAULT_RPC_PATH,
    REDACTED, REDACTED_HEADER_NAMES, REDACTED_HEADER_PATTERNS, REDACTED_URL_FIELDS,
};

/// Plan version emitted by every language client.
pub const PLAN_VERSION: &str = "1.0.0";

/// An exclusive group that has not been spent yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unset;
/// An exclusive group that has been spent; its methods are gone from the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Set;

/// Observer invoked on each retry attempt, with the attempt number and cause.
pub type RetryHook = Arc<dyn Fn(u8, &str) + Send + Sync>;
/// Observer invoked with transferred bytes and, when known, the total.
pub type ProgressHook = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Accumulated call configuration, shared by both surfaces.
///
/// Hooks are retained here rather than discarded: a request plan records only a
/// registration count, because a closure is not data, but a transport still has
/// to be able to invoke the callback the caller supplied.
#[derive(Clone)]
pub struct CallState {
    plan: Map<String, Value>,
    headers: Map<String, Value>,
    wire_headers: Map<String, Value>,
    dropped_headers: BTreeSet<String>,
    secret_headers: BTreeSet<String>,
    hook_counts: Map<String, Value>,
    retry_hooks: Vec<RetryHook>,
    progress_hooks: BTreeMap<String, Vec<ProgressHook>>,
    capabilities: BTreeSet<String>,
}

impl fmt::Debug for CallState {
    /// Closures have no useful representation, so hooks are shown as counts.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CallState")
            .field("plan", &self.plan)
            .field("headers", &self.headers)
            .field("wire_headers", &self.wire_headers)
            .field("dropped_headers", &self.dropped_headers)
            .field("secret_headers", &self.secret_headers)
            .field("hook_counts", &self.hook_counts)
            .field("retry_hooks", &self.retry_hooks.len())
            .field(
                "progress_hooks",
                &self
                    .progress_hooks
                    .iter()
                    .map(|(field, hooks)| (field.clone(), hooks.len()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl CallState {
    /// Retry observers, in registration order.
    #[must_use]
    pub fn retry_hooks(&self) -> &[RetryHook] {
        &self.retry_hooks
    }

    /// Progress observers for one plan field, in registration order.
    #[must_use]
    pub fn progress_hooks(&self, field: &str) -> &[ProgressHook] {
        self.progress_hooks.get(field).map_or(&[], Vec::as_slice)
    }
}

impl CallState {
    fn new(kind: &str, key: &str, rpc_path: &str) -> Self {
        let mut plan = Map::new();
        plan.insert("plan_version".to_owned(), json!(PLAN_VERSION));
        plan.insert("kind".to_owned(), json!(kind));
        plan.insert("key".to_owned(), json!(key));
        plan.insert("rpc_path".to_owned(), json!(rpc_path));
        plan.insert("serial_strategy".to_owned(), json!("json"));
        Self {
            plan,
            headers: Map::new(),
            wire_headers: Map::new(),
            dropped_headers: BTreeSet::new(),
            secret_headers: BTreeSet::new(),
            hook_counts: Map::new(),
            retry_hooks: Vec::new(),
            progress_hooks: BTreeMap::new(),
            capabilities: BTreeSet::new(),
        }
    }

    /// Canonical request plan. `serde_json::Map` is a `BTreeMap` unless the
    /// `preserve_order` feature is enabled, so keys serialize sorted and the
    /// bytes match the other language clients exactly.
    #[must_use]
    pub fn to_plan(&self) -> Value {
        let mut plan = self.plan.clone();
        for (field, count) in &self.hook_counts {
            plan.insert(field.clone(), count.clone());
        }
        let mut headers = self.headers.clone();
        for (name, value) in &self.wire_headers {
            headers.insert(name.clone(), value.clone());
        }
        for name in &self.dropped_headers {
            headers.remove(name);
        }
        // Final-boundary redaction. Option-level secret flags are not enough:
        // a caller can put a credential into any header through add_header, or
        // into a URL as userinfo. Every header is judged by name here, whatever
        // wrote it.
        for name in &self.secret_headers {
            if headers.contains_key(name) {
                headers.insert(name.clone(), json!(REDACTED));
            }
        }
        let names: Vec<String> = headers.keys().cloned().collect();
        for name in names {
            if header_is_sensitive(&name) {
                headers.insert(name, json!(REDACTED));
            }
        }
        if !headers.is_empty() {
            plan.insert("headers".to_owned(), Value::Object(headers));
        }
        for field in REDACTED_URL_FIELDS {
            if let Some(Value::String(url)) = plan.get(*field) {
                let stripped = strip_url_userinfo(url);
                plan.insert((*field).to_owned(), json!(stripped));
            }
        }
        Value::Object(plan)
    }

    /// Headers as they go on the wire, credentials intact.
    #[must_use]
    pub fn wire_headers(&self) -> Map<String, Value> {
        let mut headers = self.headers.clone();
        for (name, value) in &self.wire_headers {
            headers.insert(name.clone(), value.clone());
        }
        for name in &self.dropped_headers {
            headers.remove(name);
        }
        headers
    }
}

/// A unary call chain. Terminates in [`UnaryCall::make_call`].
#[derive(Debug, Clone)]
pub struct UnaryCall<Serial = Unset, Auth = Unset, Ip = Unset, Rate = Unset> {
    state: CallState,
    marker: PhantomData<(Serial, Auth, Ip, Rate)>,
}

/// A streaming call chain. Terminates in [`StreamCall::stream`].
#[derive(Debug, Clone)]
pub struct StreamCall<Serial = Unset, Auth = Unset, Ip = Unset, StreamRate = Unset> {
    state: CallState,
    marker: PhantomData<(Serial, Auth, Ip, StreamRate)>,
}

macro_rules! shared_impl {
    ($name:ident, $kind:literal, $($param:ident),+) => {
        impl $name {
            /// Start a chain for `key`, carried over `rpc_path`.
            #[must_use]
            pub fn new(key: &str, rpc_path: &str) -> Self {
                Self { state: CallState::new($kind, key, rpc_path), marker: PhantomData }
            }
        }

        impl<$($param),+> $name<$($param),+> {
            /// Canonical request plan. No network, no credentials.
            #[must_use]
            pub fn to_plan(&self) -> Value {
                self.state.to_plan()
            }

            /// Read-only view of the accumulated state.
            #[must_use]
            pub fn state(&self) -> &CallState {
                &self.state
            }

            /// Grant a capability that a gated option requires.
            #[must_use]
            pub fn with_capability(mut self, capability: &str) -> Self {
                self.state.capabilities.insert(capability.to_owned());
                self
            }

            /// True when a gated option may be applied.
            #[must_use]
            pub fn has_capability(&self, capability: &str) -> bool {
                self.state.capabilities.contains(capability)
            }

            /// Move to the next type-state. Data is unchanged; only the phantom
            /// parameters differ, so this is infallible and free.
            pub(crate) fn transmute<Next1, Next2, Next3, Next4>(self) -> $name<Next1, Next2, Next3, Next4> {
                $name { state: self.state, marker: PhantomData }
            }

            pub(crate) fn set_plan(&mut self, field: &str, value: Value) {
                self.state.plan.insert(field.to_owned(), value);
            }

            pub(crate) fn set_header(&mut self, name: &str, value: Value) {
                self.state.headers.insert(name.to_owned(), value);
            }

            pub(crate) fn set_wire_header(&mut self, name: &str, value: String) {
                self.state.wire_headers.insert(name.to_owned(), json!(value));
            }

            pub(crate) fn drop_header(&mut self, name: &str) {
                self.state.dropped_headers.insert(name.to_owned());
            }

            pub(crate) fn mark_secret_header(&mut self, name: &str) {
                self.state.secret_headers.insert(name.to_owned());
            }

            pub(crate) fn push_retry_hook(&mut self, field: &str, hook: RetryHook) {
                self.count_hook(field);
                self.state.retry_hooks.push(hook);
            }

            pub(crate) fn push_progress_hook(&mut self, field: &str, hook: ProgressHook) {
                self.count_hook(field);
                self.state
                    .progress_hooks
                    .entry(field.to_owned())
                    .or_default()
                    .push(hook);
            }

            fn count_hook(&mut self, field: &str) {
                let next = self
                    .state
                    .hook_counts
                    .get(field)
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    + 1;
                self.state.hook_counts.insert(field.to_owned(), json!(next));
            }

            pub(crate) fn set_section(&mut self, section: &str, name: &str, value: Value) {
                let entry = self
                    .state
                    .plan
                    .entry(section.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(object) = entry.as_object_mut() {
                    object.insert(name.to_owned(), value);
                }
            }

            pub(crate) fn set_body(&mut self, body: Value) {
                self.state.plan.insert("body".to_owned(), body);
            }

            pub(crate) fn set_body_field(&mut self, name: &str, value: Value) {
                let entry = self
                    .state
                    .plan
                    .entry("body".to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(object) = entry.as_object_mut() {
                    object.insert(name.to_owned(), value);
                }
            }
        }
    };
}

shared_impl!(UnaryCall, "unary", Serial, Auth, Ip, Rate);
shared_impl!(StreamCall, "stream", Serial, Auth, Ip, StreamRate);

/// # Type-state proofs
///
/// Each example below is compiled by `cargo test`. The `compile_fail` blocks
/// assert that the marked chain does **not** compile; if a narrowing regresses
/// and the chain becomes legal, the doctest fails.
///
/// A serialization strategy can only be chosen once:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::UnaryCall;
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .use_json()
///     .use_protobuf();
/// ```
///
/// Not even the same one twice:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::UnaryCall;
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .use_json()
///     .use_json();
/// ```
///
/// Selecting by value spends the same group:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::{SerialStrategy, UnaryCall};
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .use_serial_strategy(SerialStrategy::MessagePack)
///     .use_json();
/// ```
///
/// A per-call credential contradicts dropping authorization:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::UnaryCall;
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .omit_auth()
///     .with_bearer_token("t");
/// ```
///
/// The address family is pinned once:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::UnaryCall;
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .force_ipv4()
///     .force_ipv6();
/// ```
///
/// Throttling and debouncing are contradictory:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::UnaryCall;
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .throttle(100)
///     .debounce(100);
/// ```
///
/// A streaming chain has no unary terminal:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::StreamCall;
/// let plan = StreamCall::new("demo.events.watch_events", "/v1/rpc").make_call();
/// ```
///
/// Stream shaping is absent from the unary surface:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::{Backpressure, UnaryCall};
/// let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .with_backpressure(Backpressure::Buffer);
/// ```
///
/// Unary shaping is absent from the streaming surface:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::StreamCall;
/// let _ = StreamCall::new("demo.events.watch_events", "/v1/rpc").dry_run();
/// ```
///
/// What *does* compile: every group spent at most once, in any order.
///
/// ```
/// use ores_api_docs::rpc_fluent::{Compression, QueuePriority, SerialStrategy, UnaryCall};
/// use serde_json::json;
///
/// let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
///     .use_serial_strategy(SerialStrategy::MessagePack)
///     .omit_auth()
///     .force_ipv6()
///     .debounce(30)
///     .with_timeout(2_000)
///     .with_retries(3)
///     .with_retry_backoff(100, 2.0)
///     .add_path_field("user_id", json!("user-42"))
///     .queue_priority(QueuePriority::Three)
///     .compress(Compression::Gzip)
///     .to_plan();
///
/// assert_eq!(plan["serial_strategy"], "message_pack");
/// assert_eq!(plan["auth_mode"], "omitted");
/// assert_eq!(plan["ip_version"], "v6");
/// ```
pub mod type_state_proofs {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_carries_identity_and_the_default_strategy() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc").to_plan();
        assert_eq!(plan["plan_version"], PLAN_VERSION);
        assert_eq!(plan["kind"], "unary");
        assert_eq!(plan["key"], "demo.users.find_user");
        assert_eq!(plan["rpc_path"], "/v1/rpc");
        assert_eq!(plan["serial_strategy"], "json");
    }

    #[test]
    fn the_two_surfaces_declare_different_kinds() {
        assert_eq!(
            StreamCall::new("demo.events.watch_events", "/v1/rpc").to_plan()["kind"],
            "stream"
        );
    }

    #[test]
    fn a_credential_reaches_the_wire_but_not_the_plan() {
        let call = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .with_bearer_token("super-secret-value");

        let plan = call.to_plan();
        assert_eq!(plan["auth_mode"], "bearer_override");
        assert!(
            !plan.to_string().contains("super-secret-value"),
            "the plan must not carry the credential"
        );
        assert_eq!(plan["headers"]["authorization"], REDACTED);

        let wire = call.state().wire_headers();
        assert_eq!(wire["authorization"], "Bearer super-secret-value");
    }

    #[test]
    fn omit_auth_drops_a_default_authorization_header() {
        let mut call = UnaryCall::new("demo.users.find_user", "/v1/rpc");
        call.set_header("authorization", serde_json::json!("Bearer default"));
        let plan = call.omit_auth().to_plan();
        assert!(plan
            .get("headers")
            .and_then(|h| h.get("authorization"))
            .is_none());
    }

    #[test]
    fn serialization_choice_sets_content_negotiation_headers() {
        let call = UnaryCall::new("demo.users.find_user", "/v1/rpc").use_protobuf();
        let wire = call.state().wire_headers();
        assert_eq!(wire["content-type"], "application/x-protobuf");
        assert_eq!(wire["accept"], "application/x-protobuf");
    }

    #[test]
    fn plan_keys_serialize_sorted_so_languages_can_compare_bytes() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .with_timeout(50)
            .add_header("x-z", serde_json::json!("1"))
            .add_header("x-a", serde_json::json!("2"))
            .to_plan();
        let text = serde_json::to_string(&plan).expect("plan serializes");
        let keys: Vec<&str> = plan
            .as_object()
            .expect("plan is an object")
            .keys()
            .map(String::as_str)
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "plan keys must serialize in sorted order");
        assert!(text.find("\"x-a\"").unwrap() < text.find("\"x-z\"").unwrap());
    }

    #[test]
    #[should_panic(expected = "with_timeout: millis must be at most 600000")]
    fn declared_bounds_are_enforced_where_the_value_enters() {
        let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc").with_timeout(600_001);
    }

    #[test]
    fn hooks_serialize_as_counts_not_closures() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .on_retry(|_, _| {})
            .on_retry(|_, _| {})
            .to_plan();
        assert_eq!(plan["retry_hook_count"], 2);
    }
}

/// Does this header name carry a credential?
///
/// Matched case-insensitively against the contract's exact names and against
/// its substring patterns, so `X-Api-Key` and `x-tenant-api-key` are both
/// caught without enumerating every vendor spelling.
#[must_use]
pub fn header_is_sensitive(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    REDACTED_HEADER_NAMES.contains(&lowered.as_str())
        || REDACTED_HEADER_PATTERNS
            .iter()
            .any(|pattern| lowered.contains(pattern))
}

/// Remove `user:password@` from a URL without otherwise rewriting it.
///
/// Deliberately textual rather than URL-parsing: a plan must redact the same
/// bytes in every language, and parser normalization differs between them.
#[must_use]
pub fn strip_url_userinfo(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_owned();
    };
    let authority_start = scheme_end + 3;
    let rest = &url[authority_start..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let Some(at) = authority.rfind('@') else {
        return url.to_owned();
    };
    format!(
        "{}{}{}{}",
        &url[..authority_start],
        REDACTED,
        &authority[at..],
        &rest[authority_end..]
    )
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    #[test]
    fn caller_supplied_credential_headers_are_redacted() {
        let mut call = UnaryCall::new("demo.users.find_user", "/v1/rpc");
        for (name, value) in [
            ("authorization", "Bearer CALLER-SECRET"),
            ("Cookie", "session=COOKIE-SECRET"),
            ("x-api-key", "APIKEY-SECRET"),
            ("X-Tenant-Api-Key", "VENDOR-SECRET"),
            ("proxy-authorization", "Basic PROXY-SECRET"),
            ("x-refresh-token", "REFRESH-SECRET"),
        ] {
            call.set_header(&name.to_ascii_lowercase(), json!(value));
        }
        let plan = call.to_plan();
        let text = plan.to_string();
        for needle in [
            "CALLER-SECRET",
            "COOKIE-SECRET",
            "APIKEY-SECRET",
            "VENDOR-SECRET",
            "PROXY-SECRET",
            "REFRESH-SECRET",
        ] {
            assert!(
                !text.contains(needle),
                "{needle} leaked into the plan: {text}"
            );
        }
    }

    #[test]
    fn ordinary_headers_survive_redaction() {
        let mut call = UnaryCall::new("demo.users.find_user", "/v1/rpc");
        call.set_header("accept", json!("application/json"));
        call.set_header("x-request-id", json!("req-42"));
        call.set_header("x-api-version", json!("2026-09-18"));
        let plan = call.to_plan();
        assert_eq!(plan["headers"]["accept"], "application/json");
        assert_eq!(plan["headers"]["x-request-id"], "req-42");
        assert_eq!(plan["headers"]["x-api-version"], "2026-09-18");
    }

    #[test]
    fn proxy_userinfo_is_stripped_but_the_rest_of_the_url_is_kept() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .via_proxy("http://user:PROXY-PASSWORD@proxy.internal:8080/path?q=1")
            .to_plan();
        let proxy = plan["proxy_url"].as_str().expect("proxy_url");
        assert!(!proxy.contains("PROXY-PASSWORD"), "{proxy}");
        assert!(proxy.contains("proxy.internal:8080"), "{proxy}");
        assert!(proxy.contains("/path?q=1"), "{proxy}");
    }

    #[test]
    fn a_proxy_url_without_userinfo_is_untouched() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .via_proxy("http://proxy.internal:8080")
            .to_plan();
        assert_eq!(plan["proxy_url"], "http://proxy.internal:8080");
    }

    #[test]
    fn an_at_sign_in_the_path_is_not_mistaken_for_userinfo() {
        assert_eq!(
            strip_url_userinfo("http://proxy.internal/a@b"),
            "http://proxy.internal/a@b"
        );
    }

    #[test]
    fn redaction_survives_the_wire_boundary() {
        // The wire still needs the real values; only the plan is redacted.
        let mut call = UnaryCall::new("demo.users.find_user", "/v1/rpc");
        call.set_header("authorization", json!("Bearer CALLER-SECRET"));
        assert_eq!(
            call.state().wire_headers()["authorization"],
            "Bearer CALLER-SECRET"
        );
        assert_eq!(call.to_plan()["headers"]["authorization"], REDACTED);
    }
}
