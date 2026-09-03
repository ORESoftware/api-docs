#[derive(Clone, Debug, PartialEq)]
pub struct RpcV1Receipt {
    pub id: String,
    pub key: String,
    pub transport: Option<Transport>,
    pub ok: bool,
    pub status: Option<u16>,
    pub body: OptionalJson,
    pub error: Option<Map<String, Value>>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

impl RpcV1Receipt {
    #[must_use]
    pub fn success(id: impl Into<String>, key: impl Into<String>, body: OptionalJson) -> Self {
        Self {
            id: id.into(),
            key: key.into(),
            transport: None,
            ok: true,
            status: Some(200),
            body,
            error: None,
            trace_id: None,
            span_id: None,
        }
    }

    #[must_use]
    pub fn failure(
        id: impl Into<String>,
        key: impl Into<String>,
        status: u16,
        error: Map<String, Value>,
    ) -> Self {
        Self {
            id: id.into(),
            key: key.into(),
            transport: None,
            ok: false,
            status: Some(status),
            body: OptionalJson::absent(),
            error: Some(error),
            trace_id: None,
            span_id: None,
        }
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        validate_common(
            "rpc-receipt",
            &self.id,
            &self.key,
            self.trace_id.as_deref(),
            self.span_id.as_deref(),
        )?;
        if let Some(status) = self.status {
            if !(100..=599).contains(&status) {
                return instance("rpc-receipt", "status must be from 100 to 599");
            }
        }
        if self.ok {
            if self.error.is_some() {
                return instance("rpc-receipt", "a successful receipt must not carry error");
            }
            if self
                .status
                .is_some_and(|status| !(200..=399).contains(&status))
            {
                return instance(
                    "rpc-receipt",
                    "a successful receipt status must be from 200 to 399",
                );
            }
        } else {
            if self.body.is_present() {
                return instance("rpc-receipt", "an error receipt must not carry body");
            }
            if self.error.is_none() {
                return instance("rpc-receipt", "an error receipt needs error");
            }
            if self
                .status
                .is_some_and(|status| !(400..=599).contains(&status))
            {
                return instance(
                    "rpc-receipt",
                    "an error receipt status must be from 400 to 599",
                );
            }
        }
        validate_rpc_receipt(&self.to_value())
    }

    pub fn encode(&self) -> Result<Vec<u8>, SchemaError> {
        self.validate()?;
        encode_receipt(self)
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
        object.insert("op".into(), Value::String("receipt".into()));
        object.insert("id".into(), Value::String(self.id.clone()));
        object.insert("key".into(), Value::String(self.key.clone()));
        if let Some(value) = self.transport {
            object.insert("transport".into(), Value::String(value.as_str().into()));
        }
        object.insert("ok".into(), Value::Bool(self.ok));
        if let Some(value) = self.status {
            object.insert("status".into(), Value::from(value));
        }
        if let Some(value) = self.body.value() {
            object.insert("body".into(), value.clone());
        }
        if let Some(value) = &self.error {
            object.insert("error".into(), Value::Object(value.clone()));
        }
        if let Some(value) = &self.trace_id {
            object.insert("traceId".into(), Value::String(value.clone()));
        }
        if let Some(value) = &self.span_id {
            object.insert("spanId".into(), Value::String(value.clone()));
        }
        Value::Object(object)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RpcV1Envelope {
    Call(RpcV1Call),
    Receipt(RpcV1Receipt),
}

impl RpcV1Envelope {
    pub fn encode(&self) -> Result<Vec<u8>, SchemaError> {
        match self {
            Self::Call(value) => value.encode(),
            Self::Receipt(value) => value.encode(),
        }
    }

    pub fn to_length_prefixed(&self) -> Result<Vec<u8>, SchemaError> {
        encode_length_prefixed(&self.encode()?)
    }
}
