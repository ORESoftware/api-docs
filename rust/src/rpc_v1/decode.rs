pub fn decode_rpc_v1_call(payload: &[u8]) -> Result<RpcV1Call, SchemaError> {
    let value = decode_object(payload, "rpc-call", &CALL_FIELDS)?;
    validate_rpc_call(&Value::Object(value.clone()))?;
    let version = required_u64(&value, "v", "rpc-call")?;
    if version != u64::from(RPC_V1_VERSION) {
        return instance("rpc-call", format!("unsupported RPC version {version}"));
    }
    if required_string(&value, "op", "rpc-call")? != "call" {
        return instance("rpc-call", "expected op call");
    }
    let call = RpcV1Call {
        id: required_string(&value, "id", "rpc-call")?.to_owned(),
        key: required_string(&value, "key", "rpc-call")?.to_owned(),
        transport: optional_transport(&value, "rpc-call")?,
        path: optional_object(&value, "path", "rpc-call")?,
        query: optional_object(&value, "query", "rpc-call")?,
        headers: optional_object(&value, "headers", "rpc-call")?,
        body: value
            .get("body")
            .cloned()
            .map_or_else(OptionalJson::absent, OptionalJson::present),
        trace_id: optional_string(&value, "traceId", "rpc-call")?,
        span_id: optional_string(&value, "spanId", "rpc-call")?,
    };
    call.validate()?;
    Ok(call)
}

pub fn decode_rpc_v1_receipt(payload: &[u8]) -> Result<RpcV1Receipt, SchemaError> {
    let value = decode_object(payload, "rpc-receipt", &RECEIPT_FIELDS)?;
    validate_rpc_receipt(&Value::Object(value.clone()))?;
    let version = required_u64(&value, "v", "rpc-receipt")?;
    if version != u64::from(RPC_V1_VERSION) {
        return instance("rpc-receipt", format!("unsupported RPC version {version}"));
    }
    if required_string(&value, "op", "rpc-receipt")? != "receipt" {
        return instance("rpc-receipt", "expected op receipt");
    }
    let status = match value.get("status") {
        None => None,
        Some(raw) => Some(
            raw.as_u64()
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| schema_error("rpc-receipt", "status has the wrong type"))?,
        ),
    };
    let receipt = RpcV1Receipt {
        id: required_string(&value, "id", "rpc-receipt")?.to_owned(),
        key: required_string(&value, "key", "rpc-receipt")?.to_owned(),
        transport: optional_transport(&value, "rpc-receipt")?,
        ok: value
            .get("ok")
            .and_then(Value::as_bool)
            .ok_or_else(|| schema_error("rpc-receipt", "ok has the wrong type"))?,
        status,
        body: value
            .get("body")
            .cloned()
            .map_or_else(OptionalJson::absent, OptionalJson::present),
        error: optional_object(&value, "error", "rpc-receipt")?,
        trace_id: optional_string(&value, "traceId", "rpc-receipt")?,
        span_id: optional_string(&value, "spanId", "rpc-receipt")?,
    };
    receipt.validate()?;
    Ok(receipt)
}

pub fn rpc_v1_call_from_ndjson(payload: &[u8]) -> Result<RpcV1Call, SchemaError> {
    decode_rpc_v1_call(single_ndjson(payload, "rpc-call")?)
}

pub fn rpc_v1_receipt_from_ndjson(payload: &[u8]) -> Result<RpcV1Receipt, SchemaError> {
    decode_rpc_v1_receipt(single_ndjson(payload, "rpc-receipt")?)
}

pub fn assert_rpc_v1_receipt_for_call(
    call: &RpcV1Call,
    receipt: &RpcV1Receipt,
) -> Result<(), SchemaError> {
    call.validate()?;
    receipt.validate()?;
    if receipt.id != call.id {
        return instance("rpc-receipt", "receipt id does not match call id");
    }
    if receipt.key != call.key {
        return instance("rpc-receipt", "receipt key does not match call key");
    }
    if let (Some(left), Some(right)) = (call.transport, receipt.transport) {
        if left != right {
            return instance(
                "rpc-receipt",
                "receipt transport does not match call transport",
            );
        }
    }
    Ok(())
}

pub fn split_rpc_v1_length_prefixed(buffer: &[u8]) -> Result<(Vec<&[u8]>, &[u8]), SchemaError> {
    split_length_prefixed(buffer)
}

#[derive(Debug, Default)]
pub struct RpcV1Correlator {
    prefix: String,
    next: u64,
}

impl RpcV1Correlator {
    pub fn new(prefix: impl Into<String>) -> Result<Self, SchemaError> {
        let prefix = prefix.into();
        if prefix.chars().count() >= 128 {
            return instance(
                "rpc-call",
                "correlation prefix must contain fewer than 128 Unicode scalars",
            );
        }
        Ok(Self { prefix, next: 0 })
    }

    pub fn take(&mut self) -> Result<String, SchemaError> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| schema_error("rpc-call", "correlation id counter exhausted"))?;
        let value = format!("{}{}", self.prefix, self.next);
        validate_string(&value, "correlation id", 128, "rpc-call")?;
        Ok(value)
    }
}
