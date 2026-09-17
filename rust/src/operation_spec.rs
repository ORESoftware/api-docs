//! Compile-time operation contract and request-scoped typed cache.
//!
//! `OperationSpec` is the bridge between backend route contracts and generated
//! client SDK signatures. HTTP middleware and generated RPC adapters decode and
//! validate request sections once, cache them here, and the authored operation
//! reads them through `TypedOperationContext<S, O>` without re-reading a body
//! stream.

use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::RpcPayloadCodec;

/// Marker used by generated operation specs for sections that do not exist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct NoSection;

/// Compile-time contract for one semantic operation.
///
/// Generated `*-interfaces` / RPC intermediary libraries implement this trait
/// from the normalized operation IR. Client generators consume the same IR, so
/// these associated types and client payload/header/query types cannot drift
/// independently.
pub trait OperationSpec: Send + Sync + 'static {
    type Path: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type Query: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type RequestHeaders: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type RequestBody: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type ResponseBody: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type ResponseHeaders: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type ResponseTrailers: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type Error: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    const KEY: &'static str;
    const CODECS: &'static [RpcPayloadCodec];
    const DEFAULT_CODEC: RpcPayloadCodec;
}

#[derive(Debug, Error)]
pub enum OperationRequestError {
    #[error("operation request section {section} was not populated")]
    MissingSection { section: &'static str },
    #[error("operation request section {section} has the wrong cached Rust type")]
    WrongCachedType { section: &'static str },
    #[error("operation request body has no raw bytes for lazy decoding")]
    MissingRawBody,
    #[error("operation request body codec {0:?} is not supported by this decoder")]
    UnsupportedCodec(RpcPayloadCodec),
    #[error("operation request body decode failed: {0}")]
    Decode(String),
}

/// Request-scoped typed input cache for one logical operation invocation.
///
/// This is not an HTTP request and does not perform transport I/O. HTTP adapters
/// populate it from the already-admitted incoming request/extractors; RPC
/// adapters populate it from the already-decoded `RpcV1Call`.
#[derive(Clone)]
pub struct OperationRequestData {
    inner: Arc<OperationRequestDataInner>,
}

struct OperationRequestDataInner {
    sections: RwLock<BTreeMap<&'static str, Arc<dyn Any + Send + Sync>>>,
    raw_body: RwLock<Option<Arc<[u8]>>>,
    codec: RwLock<RpcPayloadCodec>,
    semantic_input: RwLock<Value>,
}

impl Default for OperationRequestData {
    fn default() -> Self {
        Self::new(RpcPayloadCodec::Json)
    }
}

impl OperationRequestData {
    #[must_use]
    pub fn new(codec: RpcPayloadCodec) -> Self {
        Self {
            inner: Arc::new(OperationRequestDataInner {
                sections: RwLock::new(BTreeMap::new()),
                raw_body: RwLock::new(None),
                codec: RwLock::new(codec),
                semantic_input: RwLock::new(Value::Object(serde_json::Map::new())),
            }),
        }
    }

    /// Construct the typed input cache for one operation using its declared
    /// default codec, and seed only request sections whose associated type is
    /// exactly `NoSection`.
    ///
    /// Real path/query/header/body DTOs remain absent until an HTTP adapter or
    /// RPC decoder supplies the value from the original transport input. This
    /// removes both the repeated default codec and `insert_*::<O>(NoSection)`
    /// boilerplate without defaulting any semantic request data.
    #[must_use]
    pub fn for_operation<O: OperationSpec>() -> Self {
        Self::for_operation_with_codec::<O>(O::DEFAULT_CODEC)
    }

    /// Same as `for_operation`, but for a transport that has already negotiated
    /// an explicit codec for this invocation.
    #[must_use]
    pub fn for_operation_with_codec<O: OperationSpec>(codec: RpcPayloadCodec) -> Self {
        let request = Self::new(codec);
        request.seed_no_sections::<O>();
        request
    }

    fn seed_no_sections<O: OperationSpec>(&self) {
        self.seed_no_section_if::<O::Path>("path");
        self.seed_no_section_if::<O::Query>("query");
        self.seed_no_section_if::<O::RequestHeaders>("headers");
        self.seed_no_section_if::<O::RequestBody>("body");
    }

    fn seed_no_section_if<T>(&self, section: &'static str)
    where
        T: Send + Sync + 'static,
    {
        if TypeId::of::<T>() == TypeId::of::<NoSection>() {
            self.insert(section, NoSection);
        }
    }

    #[must_use]
    pub fn codec(&self) -> RpcPayloadCodec {
        *self
            .inner
            .codec
            .read()
            .expect("operation request codec lock poisoned")
    }

    pub fn set_codec(&self, codec: RpcPayloadCodec) {
        *self
            .inner
            .codec
            .write()
            .expect("operation request codec lock poisoned") = codec;
    }

    pub fn set_raw_body(&self, body: impl Into<Arc<[u8]>>) {
        *self
            .inner
            .raw_body
            .write()
            .expect("operation raw body lock poisoned") = Some(body.into());
    }

    #[must_use]
    pub fn raw_body(&self) -> Option<Arc<[u8]>> {
        self.inner
            .raw_body
            .read()
            .expect("operation raw body lock poisoned")
            .clone()
    }

    /// Canonical JSON-shaped request view used by shared policy and audit.
    /// Generated adapters populate this from already validated typed sections;
    /// authored operations use the typed accessors instead.
    pub fn set_semantic_input(&self, input: Value) {
        *self
            .inner
            .semantic_input
            .write()
            .expect("operation semantic input lock poisoned") = input;
    }

    #[must_use]
    pub fn semantic_input(&self) -> Value {
        self.inner
            .semantic_input
            .read()
            .expect("operation semantic input lock poisoned")
            .clone()
    }

    pub fn insert<T>(&self, section: &'static str, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.inner
            .sections
            .write()
            .expect("operation request section lock poisoned")
            .insert(section, Arc::new(value));
    }

    pub fn insert_arc<T>(&self, section: &'static str, value: Arc<T>)
    where
        T: Send + Sync + 'static,
    {
        self.inner
            .sections
            .write()
            .expect("operation request section lock poisoned")
            .insert(section, value);
    }

    /// Operation-typed setters are the normal adapter/middleware API. These
    /// prevent a route or generated `rpc.rs` from caching a DTO for a different
    /// operation under the right section name.
    pub fn insert_path<O: OperationSpec>(&self, value: O::Path) {
        self.insert("path", value);
    }

    pub fn insert_query<O: OperationSpec>(&self, value: O::Query) {
        self.insert("query", value);
    }

    pub fn insert_headers<O: OperationSpec>(&self, value: O::RequestHeaders) {
        self.insert("headers", value);
    }

    pub fn insert_body<O: OperationSpec>(&self, value: O::RequestBody) {
        self.insert("body", value);
    }

    pub fn get<T>(&self, section: &'static str) -> Result<Arc<T>, OperationRequestError>
    where
        T: Send + Sync + 'static,
    {
        let value = self
            .inner
            .sections
            .read()
            .expect("operation request section lock poisoned")
            .get(section)
            .cloned()
            .ok_or(OperationRequestError::MissingSection { section })?;
        value
            .downcast::<T>()
            .map_err(|_| OperationRequestError::WrongCachedType { section })
    }

    /// JSON lazy fallback for middleware stacks that did not pre-deserialize the
    /// request body. Binary codecs are intentionally delegated to generated
    /// operation codec bridges so Protobuf/MessagePack cannot be silently
    /// treated as JSON.
    pub fn get_or_decode_json<T>(
        &self,
        section: &'static str,
    ) -> Result<Arc<T>, OperationRequestError>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        if let Ok(value) = self.get::<T>(section) {
            return Ok(value);
        }
        if self.codec() != RpcPayloadCodec::Json {
            return Err(OperationRequestError::UnsupportedCodec(self.codec()));
        }
        let raw = self
            .raw_body()
            .ok_or(OperationRequestError::MissingRawBody)?;
        let decoded = serde_json::from_slice::<T>(&raw)
            .map_err(|error| OperationRequestError::Decode(error.to_string()))?;
        let value = Arc::new(decoded);
        self.insert_arc(section, value.clone());
        Ok(value)
    }

    #[must_use]
    pub fn contains_type<T>(&self, section: &'static str) -> bool
    where
        T: Send + Sync + 'static,
    {
        self.inner
            .sections
            .read()
            .expect("operation request section lock poisoned")
            .get(section)
            .is_some_and(|value| value.as_ref().type_id() == TypeId::of::<T>())
    }
}

/// Typed view over one request cache. The phantom operation parameter is what
/// makes `body()` return exactly the backend request type generated for O.
#[derive(Clone)]
pub struct TypedOperationRequest<O: OperationSpec> {
    data: OperationRequestData,
    _operation: PhantomData<fn() -> O>,
}

impl<O: OperationSpec> TypedOperationRequest<O> {
    #[must_use]
    pub fn new(data: OperationRequestData) -> Self {
        Self {
            data,
            _operation: PhantomData,
        }
    }

    #[must_use]
    pub fn data(&self) -> &OperationRequestData {
        &self.data
    }

    pub fn path(&self) -> Result<Arc<O::Path>, OperationRequestError> {
        self.data.get("path")
    }

    pub fn query(&self) -> Result<Arc<O::Query>, OperationRequestError> {
        self.data.get("query")
    }

    pub fn headers(&self) -> Result<Arc<O::RequestHeaders>, OperationRequestError> {
        self.data.get("headers")
    }

    pub fn body(&self) -> Result<Arc<O::RequestBody>, OperationRequestError> {
        self.data.get_or_decode_json("body")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
    struct CreateBody {
        display_name: String,
    }

    struct CreateUser;

    impl OperationSpec for CreateUser {
        type Path = NoSection;
        type Query = NoSection;
        type RequestHeaders = NoSection;
        type RequestBody = CreateBody;
        type ResponseBody = NoSection;
        type ResponseHeaders = NoSection;
        type ResponseTrailers = NoSection;
        type Error = NoSection;

        const KEY: &'static str = "demo.users.create_user";
        const CODECS: &'static [RpcPayloadCodec] = &[RpcPayloadCodec::Json];
        const DEFAULT_CODEC: RpcPayloadCodec = RpcPayloadCodec::Json;
    }

    #[test]
    fn typed_constructor_uses_default_codec_and_seeds_only_no_section_slots() {
        let data = OperationRequestData::for_operation::<CreateUser>();
        assert_eq!(data.codec(), RpcPayloadCodec::Json);
        assert!(data.contains_type::<NoSection>("path"));
        assert!(data.contains_type::<NoSection>("query"));
        assert!(data.contains_type::<NoSection>("headers"));
        assert!(!data.contains_type::<NoSection>("body"));
        assert!(!data.contains_type::<CreateBody>("body"));
    }

    #[test]
    fn cached_typed_body_is_returned_without_redecoding() {
        let data = OperationRequestData::for_operation::<CreateUser>();
        data.insert_body::<CreateUser>(CreateBody {
            display_name: "cached".into(),
        });
        data.set_raw_body(br#"{"display_name":"raw"}"#.as_slice());
        let request = TypedOperationRequest::<CreateUser>::new(data);
        assert_eq!(request.body().expect("body").display_name, "cached");
    }

    #[test]
    fn json_body_can_be_lazily_decoded_once() {
        let data = OperationRequestData::for_operation::<CreateUser>();
        data.set_raw_body(br#"{"display_name":"Alex"}"#.as_slice());
        let request = TypedOperationRequest::<CreateUser>::new(data.clone());
        assert_eq!(request.body().expect("body").display_name, "Alex");
        assert!(data.contains_type::<CreateBody>("body"));
    }
}
