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
    LIST_VALUED_HEADERS, REDACTED, REDACTED_HEADER_NAMES, REDACTED_HEADER_PATTERNS,
    REDACTED_QUERY_NAMES, REDACTED_URL_FIELDS, REDACTED_URL_USERINFO,
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
    /// Shows the REDACTED plan, never the fields it is assembled from.
    ///
    /// `{:?}` is how a call ends up in a log line, a panic message or a
    /// `tracing` field, and the raw fields hold everything redaction exists to
    /// hide: the bearer token in `wire_headers`, a credential in a query field,
    /// userinfo in a proxy URL. Printing them made `debug!(?call)` a credential
    /// leak. Headers, query and hook counts are all in the plan already, so
    /// nothing is lost. Closures have no useful representation and are shown as
    /// counts.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CallState")
            .field("plan", &self.to_plan())
            .field("dropped_headers", &self.dropped_headers)
            .field("secret_headers", &self.secret_headers)
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
        let mut plan = self.assemble();
        Self::redact_with(&self.secret_headers, &mut plan);
        canonicalize_numbers(Value::Object(plan))
    }

    /// Identity of this call for caching and in-flight deduplication.
    ///
    /// **Never key a cache on [`CallState::to_plan`].** A plan is redacted by
    /// design, so two callers holding different credentials produce identical
    /// plans, and a plan-keyed cache serves one principal's response to
    /// another. That happened in the TypeScript client twice: first through the
    /// bearer token, then — after headers were added to the key — through a
    /// credential carried in a query field, which redaction collapses just the
    /// same.
    ///
    /// This is the *pre-redaction* document, so everything redaction can
    /// collapse is present by construction: headers, query fields, URL
    /// userinfo, and any class of field redaction learns to hide later. It
    /// holds raw credentials. Keep it in memory, hash it if it must be stored,
    /// and never log or serialize it.
    #[must_use]
    pub fn execution_identity(&self) -> String {
        canonical_plan_string(&self.wire_plan())
    }

    /// The call as it goes on the wire, credentials intact: what a transport
    /// *executes*, where [`CallState::to_plan`] is what anyone *logs*.
    ///
    /// A transport handed only the plan cannot do its job. `via_proxy` with
    /// credentials in the URL reached it as `http://redacted@proxy`, so the
    /// proxy answered 407 and nothing said why. Same fields as the plan, same
    /// order, nothing redacted — never log or persist it.
    #[must_use]
    pub fn wire_plan(&self) -> Value {
        canonicalize_numbers(Value::Object(self.assemble()))
    }

    /// The complete call document, before any redaction.
    ///
    /// [`CallState::to_plan`] and [`CallState::execution_identity`] both start
    /// here, which is what keeps them from drifting: redaction is a pure
    /// function applied afterwards, so it can only remove information the
    /// identity already has.
    fn assemble(&self) -> Map<String, Value> {
        let mut plan = self.plan.clone();
        for (field, count) in &self.hook_counts {
            plan.insert(field.clone(), count.clone());
        }
        let headers = self.wire_headers();
        if !headers.is_empty() {
            plan.insert("headers".to_owned(), Value::Object(headers));
        }
        plan
    }

    /// Final-boundary redaction. Option-level secret flags are not enough: a
    /// caller can put a credential into any header through add_header, into a
    /// query field, or into a URL as userinfo. Every value is judged by the
    /// name of the field carrying it, whatever wrote it; headers a secret
    /// option declared are redacted as well, whatever they are called.
    fn redact_with(secret_headers: &BTreeSet<String>, plan: &mut Map<String, Value>) {
        if let Some(Value::Object(headers)) = plan.get_mut("headers") {
            for name in secret_headers {
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
        }
        for field in REDACTED_URL_FIELDS {
            if let Some(Value::String(url)) = plan.get(*field) {
                let stripped = strip_url_userinfo(url);
                plan.insert((*field).to_owned(), json!(stripped));
            }
        }
        if let Some(Value::Object(query)) = plan.get_mut("query") {
            let names: Vec<String> = query.keys().cloned().collect();
            for name in names {
                if query_field_is_sensitive(&name) {
                    query.insert(name, json!(REDACTED));
                }
            }
        }
    }

    /// Headers as they go on the wire, credentials intact.
    #[must_use]
    pub fn wire_headers(&self) -> Map<String, Value> {
        let mut headers = self.headers.clone();
        for (name, value) in &self.wire_headers {
            // An option's directive joins the caller's own rather than
            // replacing it: `add_header("cache-control", "max-age=0")` followed
            // by `require_fresh()` sends both.
            let merged = match (headers.get(name), value) {
                (Some(Value::String(existing)), Value::String(added))
                    if header_is_list_valued(name) =>
                {
                    json!(merge_directives(existing, added))
                }
                _ => value.clone(),
            };
            headers.insert(name.clone(), merged);
        }
        for name in &self.dropped_headers {
            headers.remove(name);
        }
        headers
    }

    /// Record a header written by an option. A list-valued header accumulates.
    fn write_wire_header(&mut self, name: &str, value: String) {
        let value = match self.wire_headers.get(name) {
            Some(Value::String(existing)) if header_is_list_valued(name) => {
                merge_directives(existing, &value)
            }
            _ => value,
        };
        self.wire_headers.insert(name.to_owned(), json!(value));
    }
}

/// Is this header a comma-separated directive list, per the contract?
#[must_use]
pub fn header_is_list_valued(name: &str) -> bool {
    LIST_VALUED_HEADERS.contains(&name.to_ascii_lowercase().as_str())
}

/// Union of two comma-separated directive lists: trimmed, de-duplicated and
/// sorted, so the result does not depend on the order the parts were written.
#[must_use]
pub fn merge_directives(existing: &str, added: &str) -> String {
    let directives: BTreeSet<&str> = existing
        .split(',')
        .chain(added.split(','))
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .collect();
    directives.into_iter().collect::<Vec<_>>().join(", ")
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
                self.state.write_wire_header(name, value);
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

/// Network adapter for the unary surface.
///
/// This crate opens no sockets, so the transport is injected, the same way
/// `TypedApiTransport` is. The builder owns what a call *is*; the transport
/// owns how it travels.
pub trait UnaryTransport {
    type Error;

    /// Send one fully built call and resolve to its receipt.
    fn send(
        &self,
        call: &CallState,
    ) -> impl std::future::Future<Output = Result<Value, Self::Error>>;
}

/// Network adapter for the streaming surface.
pub trait StreamTransport {
    type Error;
    /// Whatever the carrier yields: an async stream, a channel, a handle.
    type Stream;

    /// Open one fully built streaming call.
    fn open(
        &self,
        call: &CallState,
    ) -> impl std::future::Future<Output = Result<Self::Stream, Self::Error>>;
}

impl<Serial, Auth, Ip, Rate> UnaryCall<Serial, Auth, Ip, Rate> {
    /// Execute the call. The sole network boundary of the unary surface.
    ///
    /// Defined on `UnaryCall` only. `StreamCall` has no such method, so a
    /// streaming chain cannot be terminated as a unary call.
    ///
    /// # Errors
    /// Whatever the transport reports.
    pub async fn make_call<T: UnaryTransport>(self, transport: &T) -> Result<Value, T::Error> {
        transport.send(&self.state).await
    }
}

impl<Serial, Auth, Ip, StreamRate> StreamCall<Serial, Auth, Ip, StreamRate> {
    /// Open the stream. The sole network boundary of the streaming surface.
    ///
    /// Defined on `StreamCall` only. `UnaryCall` has no such method, so a unary
    /// chain cannot be opened as a stream.
    ///
    /// # Errors
    /// Whatever the transport reports.
    pub async fn stream<T: StreamTransport>(self, transport: &T) -> Result<T::Stream, T::Error> {
        transport.open(&self.state).await
    }
}

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
/// A streaming chain has no unary terminal. The positive twin below differs
/// only in the builder type, so the failure here can only be the missing
/// method — `compile_fail` passes on *any* error, and without the twin a typo
/// would count as proof:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::{CallState, StreamCall, UnaryTransport};
/// use serde_json::Value;
/// struct T;
/// impl UnaryTransport for T {
///     type Error = ();
///     async fn send(&self, _call: &CallState) -> Result<Value, ()> { Ok(Value::Null) }
/// }
/// async fn run() { let _ = StreamCall::new("demo.events.watch_events", "/v1/rpc").make_call(&T).await; }
/// ```
///
/// ```
/// use ores_api_docs::rpc_fluent::{CallState, UnaryCall, UnaryTransport};
/// use serde_json::Value;
/// struct T;
/// impl UnaryTransport for T {
///     type Error = ();
///     async fn send(&self, _call: &CallState) -> Result<Value, ()> { Ok(Value::Null) }
/// }
/// async fn run() { let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc").make_call(&T).await; }
/// ```
///
/// And a unary chain cannot be opened as a stream:
///
/// ```compile_fail
/// use ores_api_docs::rpc_fluent::{CallState, StreamTransport, UnaryCall};
/// struct T;
/// impl StreamTransport for T {
///     type Error = ();
///     type Stream = ();
///     async fn open(&self, _call: &CallState) -> Result<(), ()> { Ok(()) }
/// }
/// async fn run() { let _ = UnaryCall::new("demo.users.find_user", "/v1/rpc").stream(&T).await; }
/// ```
///
/// ```
/// use ores_api_docs::rpc_fluent::{CallState, StreamCall, StreamTransport};
/// struct T;
/// impl StreamTransport for T {
///     type Error = ();
///     type Stream = ();
///     async fn open(&self, _call: &CallState) -> Result<(), ()> { Ok(()) }
/// }
/// async fn run() { let _ = StreamCall::new("demo.events.watch_events", "/v1/rpc").stream(&T).await; }
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
    let normalized = normalize_field_name(name);
    REDACTED_HEADER_NAMES.contains(&normalized.as_str())
        || REDACTED_HEADER_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(pattern))
}

/// Lowercase and map `_` to `-`, so `Access_Token` and `access-token` are
/// one name. Every language client normalizes identically.
fn normalize_field_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// Does this query-field name carry a credential?
#[must_use]
pub fn query_field_is_sensitive(name: &str) -> bool {
    let normalized = normalize_field_name(name);
    REDACTED_QUERY_NAMES.contains(&normalized.as_str())
        || REDACTED_HEADER_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(pattern))
}

/// Give every number one spelling.
///
/// Plans are compared across languages as bytes. JavaScript has a single number
/// type and serializes `2.0` as `2`, while serde_json keeps `2.0` for an f64,
/// so the "same" plan differed by bytes between the two clients and the
/// conformance test only agreed because it re-serialized the Rust plan through
/// JavaScript first. An integral float inside the exactly-representable range
/// is therefore written as an integer, which is the spelling both produce.
fn canonicalize_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            if let Some(float) = number.as_f64() {
                const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
                if number.is_f64() && float.fract() == 0.0 && float.abs() <= MAX_SAFE {
                    #[allow(clippy::cast_possible_truncation)]
                    return json!(float as i64);
                }
            }
            Value::Number(number)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize_numbers).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, canonicalize_numbers(item)))
                .collect(),
        ),
        other => other,
    }
}

/// The exact bytes a plan is compared by: compact JSON, sorted keys, one
/// spelling per number.
#[must_use]
pub fn canonical_plan_string(plan: &Value) -> String {
    serde_json::to_string(plan).unwrap_or_default()
}

/// Replace `user:password@` in a URL without otherwise rewriting it.
///
/// The replacement is [`REDACTED_URL_USERINFO`], not [`REDACTED`]: RFC 3986
/// userinfo admits only unreserved, pct-encoded and sub-delim characters, so
/// `[redacted]` would turn every redacted proxy URL into an invalid URI.
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
        REDACTED_URL_USERINFO,
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

#[cfg(test)]
mod audit_tests {
    use super::*;
    use std::sync::Mutex;

    /// Records what reached the wire, so a test can tell the plan from the call.
    struct Recording(Mutex<Vec<Map<String, Value>>>);

    impl UnaryTransport for Recording {
        type Error = std::convert::Infallible;

        async fn send(&self, call: &CallState) -> Result<Value, Self::Error> {
            self.0.lock().expect("lock").push(call.wire_headers());
            Ok(json!({ "v": 1, "op": "receipt", "ok": true }))
        }
    }

    struct Opening;

    impl StreamTransport for Opening {
        type Error = std::convert::Infallible;
        type Stream = &'static str;

        async fn open(&self, call: &CallState) -> Result<Self::Stream, Self::Error> {
            assert_eq!(call.to_plan()["kind"], "stream");
            Ok("opened")
        }
    }

    #[tokio::test]
    async fn make_call_hands_the_transport_real_credentials_not_the_redacted_plan() {
        let transport = Recording(Mutex::new(Vec::new()));
        let call = UnaryCall::new("demo.users.find_user", "/v1/rpc").with_bearer_token("REAL");
        assert_eq!(call.to_plan()["headers"]["authorization"], REDACTED);

        let receipt = call.make_call(&transport).await.expect("infallible");
        assert_eq!(receipt["ok"], true);

        let sent = transport.0.lock().expect("lock");
        assert_eq!(sent.len(), 1, "make_call is the single network boundary");
        assert_eq!(sent[0]["authorization"], "Bearer REAL");
    }

    #[tokio::test]
    async fn stream_opens_the_streaming_surface() {
        let opened = StreamCall::new("demo.events.watch_events", "/v1/rpc")
            .with_stream_buffer(64)
            .stream(&Opening)
            .await
            .expect("infallible");
        assert_eq!(opened, "opened");
    }

    #[test]
    fn credential_bearing_query_fields_are_redacted_by_name() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .add_query_field("access_token", json!("QUERY-SECRET"))
            .add_query_field("Api-Key", json!("KEY-SECRET"))
            .add_query_field("sig", json!("SIG-SECRET"))
            .add_query_field("page", json!(2))
            .to_plan();
        let text = plan.to_string();
        for needle in ["QUERY-SECRET", "KEY-SECRET", "SIG-SECRET"] {
            assert!(!text.contains(needle), "{needle} leaked: {text}");
        }
        assert_eq!(plan["query"]["page"], 2, "ordinary query fields survive");
    }

    #[test]
    fn underscore_and_dash_spellings_are_one_name() {
        assert!(header_is_sensitive("X_Api_Key"));
        assert!(header_is_sensitive("x-api-key"));
        assert!(query_field_is_sensitive("ACCESS_TOKEN"));
        assert!(!query_field_is_sensitive("page"));
        assert!(!header_is_sensitive("x-api-version"));
    }

    #[test]
    fn an_integral_float_has_exactly_one_spelling() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .with_retries(1)
            .with_retry_backoff(100, 2.0)
            .to_plan();
        let bytes = canonical_plan_string(&plan);
        assert!(bytes.contains(r#""factor":2}"#), "{bytes}");
        assert!(
            !bytes.contains("2.0"),
            "JavaScript would write 2, so Rust must too"
        );
    }

    #[test]
    fn a_fractional_float_is_left_alone() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .with_retries(1)
            .with_retry_backoff(100, 1.5)
            .to_plan();
        assert!(canonical_plan_string(&plan).contains(r#""factor":1.5"#));
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    type Build = fn(&str) -> UnaryCall;

    /// One entry per class of value that redaction collapses. Each builds the
    /// same call for two different principals.
    fn collapsible_classes() -> Vec<(&'static str, Build)> {
        fn base() -> UnaryCall {
            UnaryCall::new("demo.users.find_user", "/v1/rpc")
        }
        vec![
            ("bearer option", |who| {
                let call: UnaryCall<_, _, _, _> = base().with_bearer_token(who).transmute();
                call
            }),
            ("authorization header", |who| {
                base().add_header("authorization", json!(format!("Bearer {who}")))
            }),
            ("cookie header", |who| {
                base().add_header("cookie", json!(format!("session={who}")))
            }),
            ("vendor api-key header", |who| {
                base().add_header("x-tenant-api-key", json!(who))
            }),
            ("access_token query field", |who| {
                base().add_query_field("access_token", json!(who))
            }),
            ("signature query field", |who| {
                base().add_query_field("sig", json!(who))
            }),
            ("proxy URL userinfo", |who| {
                base().via_proxy(&format!("http://{who}:pw@proxy.internal:8080"))
            }),
        ]
    }

    #[test]
    fn redaction_collapses_principals_and_the_identity_does_not() {
        for (class, build) in collapsible_classes() {
            let alice = build("ALICE");
            let bob = build("BOB");

            // The premise: the plans really are indistinguishable. If this ever
            // fails, the class is no longer redacted and the test below it
            // proves nothing.
            assert_eq!(
                alice.to_plan(),
                bob.to_plan(),
                "{class}: plans must be identical once redacted"
            );
            assert_ne!(
                alice.state().execution_identity(),
                bob.state().execution_identity(),
                "{class}: the execution identity must still tell the principals apart, \
                 or a cache keyed on it serves one user's response to another"
            );
        }
    }

    #[test]
    fn the_same_principal_has_a_stable_identity() {
        for (class, build) in collapsible_classes() {
            assert_eq!(
                build("ALICE").state().execution_identity(),
                build("ALICE").state().execution_identity(),
                "{class}: an identity that changes between equal calls defeats caching"
            );
        }
    }

    #[test]
    fn the_identity_is_a_superset_of_the_plan() {
        // Structural guarantee: both start from the same assembled document and
        // redaction only removes. So every field the plan has, the identity has.
        let call = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .with_timeout(250)
            .add_query_field("page", json!(2))
            .add_query_field("access_token", json!("T"))
            .add_header("authorization", json!("Bearer T"));
        let plan = call.to_plan();
        let identity: Value =
            serde_json::from_str(&call.state().execution_identity()).expect("identity is JSON");
        for key in plan.as_object().expect("plan object").keys() {
            assert!(
                identity.get(key).is_some(),
                "identity is missing plan field {key}"
            );
        }
        assert_eq!(identity["query"]["access_token"], "T");
        assert_eq!(plan["query"]["access_token"], REDACTED);
    }

    #[test]
    fn debug_output_never_shows_a_credential() {
        // `{:?}` is how a call reaches a log line or a panic message. It once
        // printed the raw fields, so `debug!(?call)` logged the bearer token.
        for (class, build) in collapsible_classes() {
            let call = build("S3CRET-PRINCIPAL");
            // The premise: the secret really is in there to be leaked.
            assert!(
                call.state()
                    .execution_identity()
                    .contains("S3CRET-PRINCIPAL"),
                "{class}: the fixture does not carry its secret"
            );
            for shown in [
                format!("{call:?}"),
                format!("{:?}", call.state()),
                format!("{call:#?}"),
            ] {
                assert!(
                    !shown.contains("S3CRET-PRINCIPAL"),
                    "{class}: Debug output leaks the credential: {shown}"
                );
            }
        }
        // Still useful: the redacted plan is all there.
        let shown = format!(
            "{:?}",
            UnaryCall::new("demo.users.find_user", "/v1/rpc").with_timeout(250)
        );
        assert!(
            shown.contains("timeout_millis") && shown.contains("demo.users.find_user"),
            "{shown}"
        );
    }

    #[test]
    fn a_transport_is_given_the_credentials_it_has_to_execute_with() {
        let call = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .via_proxy("http://user:pw@proxy.internal:8080")
            .add_query_field("access_token", json!("T0KEN"));
        let wire = call.state().wire_plan();
        let plan = call.to_plan();
        // What a transport executes. Handed only the plan, a proxying transport
        // saw http://redacted@proxy.internal and the proxy answered 407.
        assert_eq!(wire["proxy_url"], "http://user:pw@proxy.internal:8080");
        assert_eq!(wire["query"]["access_token"], "T0KEN");
        // What anyone logs: the same fields, nothing secret.
        assert_eq!(plan["proxy_url"], "http://redacted@proxy.internal:8080");
        assert_eq!(plan["query"]["access_token"], REDACTED);
        let keys = |value: &Value| {
            value
                .as_object()
                .expect("object")
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&wire), keys(&plan));
        // And the identity is exactly the wire plan, canonically serialized.
        assert_eq!(
            call.state().execution_identity(),
            canonical_plan_string(&wire)
        );
    }

    #[test]
    fn a_redacted_proxy_url_is_still_a_valid_uri() {
        let plan = UnaryCall::new("demo.users.find_user", "/v1/rpc")
            .via_proxy("http://user:pw@proxy.internal:8080/p?q=1")
            .to_plan();
        let proxy = plan["proxy_url"].as_str().expect("proxy_url");
        assert_eq!(proxy, "http://redacted@proxy.internal:8080/p?q=1");
        // RFC 3986 userinfo: unreserved / pct-encoded / sub-delims / ":".
        let userinfo = proxy
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('@').next())
            .expect("userinfo");
        assert!(
            userinfo
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=:%".contains(c)),
            "{userinfo:?} is not valid RFC 3986 userinfo"
        );
    }
}
