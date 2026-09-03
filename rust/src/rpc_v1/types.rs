pub const RPC_V1_VERSION: u32 = 1;

const CALL_FIELDS: [&str; 10] = [
    "v", "op", "id", "key", "transport", "path", "query", "body", "traceId", "spanId",
];
const RECEIPT_FIELDS: [&str; 11] = [
    "v", "op", "id", "key", "transport", "ok", "status", "body", "error", "traceId",
    "spanId",
];

#[derive(Clone, Debug, PartialEq)]
pub struct OptionalJson {
    present: bool,
    value: Value,
}

impl OptionalJson {
    #[must_use]
    pub const fn absent() -> Self {
        Self { present: false, value: Value::Null }
    }

    #[must_use]
    pub const fn present(value: Value) -> Self {
        Self { present: true, value }
    }

    #[must_use]
    pub const fn is_present(&self) -> bool { self.present }

    #[must_use]
    pub fn value(&self) -> Option<&Value> { self.present.then_some(&self.value) }
}

impl Default for OptionalJson {
    fn default() -> Self { Self::absent() }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RpcV1Call {
    pub id: String,
    pub key: String,
    pub transport: Option<Transport>,
    pub path: Option<Map<String, Value>>,
    pub query: Option<Map<String, Value>>,
    pub body: OptionalJson,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

impl RpcV1Call {
    #[must_use]
    pub fn new(id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            id: id.into(), key: key.into(), transport: None, path: None, query: None,
            body: OptionalJson::absent(), trace_id: None, span_id: None,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        validate_common("rpc-call", &self.id, &self.key, self.trace_id.as_deref(), self.span_id.as_deref())?;
        validate_rpc_call(&self.to_value())
    }

    pub fn encode(&self) -> Result<Vec<u8>, SchemaError> {
        self.validate()?;
        encode_call(self)
    }

    pub fn to_ndjson(&self) -> Result<String, SchemaError> {
        let mut line = String::from_utf8(self.encode()?).expect("JSON is UTF-8");
        line.push('\n');
        Ok(line)
    }

    pub fn to_length_prefixed(&self) -> Result<Vec<u8>, SchemaError> {
        encode_length_prefixed(&self.encode()?)
    }

    fn to_value(&self) -> Value {
        let mut object = Map::new();
        object.insert("v".into(), Value::from(RPC_V1_VERSION));
        object.insert("op".into(), Value::String("call".into()));
        object.insert("id".into(), Value::String(self.id.clone()));
        object.insert("key".into(), Value::String(self.key.clone()));
        if let Some(value) = self.transport { object.insert("transport".into(), Value::String(value.as_str().into())); }
        if let Some(value) = &self.path { object.insert("path".into(), Value::Object(value.clone())); }
        if let Some(value) = &self.query { object.insert("query".into(), Value::Object(value.clone())); }
        if let Some(value) = self.body.value() { object.insert("body".into(), value.clone()); }
        if let Some(value) = &self.trace_id { object.insert("traceId".into(), Value::String(value.clone())); }
        if let Some(value) = &self.span_id { object.insert("spanId".into(), Value::String(value.clone())); }
        Value::Object(object)
    }
}
