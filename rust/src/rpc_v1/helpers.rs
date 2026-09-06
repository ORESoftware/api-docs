fn decode_object(
    payload: &[u8],
    name: &'static str,
    allowed: &[&str],
) -> Result<Map<String, Value>, SchemaError> {
    if payload.len() > MAX_FRAME_BYTES {
        return instance(
            name,
            format!(
                "frame is {} bytes, over the {MAX_FRAME_BYTES} limit",
                payload.len()
            ),
        );
    }
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| schema_error(name, format!("frame is not JSON: {error}")))?;
    let Value::Object(object) = value else {
        return instance(name, "frame must be a JSON object");
    };
    let mut unknown = object
        .keys()
        .map(String::as_str)
        .filter(|field| !allowed.contains(field))
        .collect::<Vec<_>>();
    unknown.sort_unstable();
    if !unknown.is_empty() {
        return instance(
            name,
            format!("unknown envelope member(s): {}", unknown.join(", ")),
        );
    }
    Ok(object)
}

fn single_ndjson<'a>(payload: &'a [u8], name: &'static str) -> Result<&'a [u8], SchemaError> {
    if payload.len() > MAX_FRAME_BYTES + 2 {
        return instance(name, "NDJSON frame exceeds the byte limit");
    }
    let line = if payload.ends_with(b"\r\n") {
        &payload[..payload.len() - 2]
    } else if payload.ends_with(b"\n") {
        &payload[..payload.len() - 1]
    } else {
        payload
    };
    if line.is_empty() {
        return instance(name, "NDJSON input is empty");
    }
    if line.len() > MAX_FRAME_BYTES {
        return instance(name, "NDJSON frame exceeds the byte limit");
    }
    if line.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return instance(name, "NDJSON input must contain exactly one JSON object");
    }
    Ok(line)
}

fn validate_common(
    name: &'static str,
    id: &str,
    key: &str,
    trace_id: Option<&str>,
    span_id: Option<&str>,
) -> Result<(), SchemaError> {
    validate_string(id, "id", 128, name)?;
    if !portable_key(key) {
        return instance(name, "key must be a portable RPC identifier");
    }
    if let Some(value) = trace_id {
        validate_string(value, "traceId", 64, name)?;
    }
    if let Some(value) = span_id {
        validate_string(value, "spanId", 32, name)?;
    }
    Ok(())
}

fn validate_string(
    value: &str,
    field: &str,
    max: usize,
    name: &'static str,
) -> Result<(), SchemaError> {
    if value.is_empty() || value.chars().count() > max {
        return instance(
            name,
            format!("{field} must be 1..{max} Unicode scalar values"),
        );
    }
    Ok(())
}

fn portable_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn required_u64(
    object: &Map<String, Value>,
    field: &str,
    name: &'static str,
) -> Result<u64, SchemaError> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| schema_error(name, format!("{field} has the wrong type")))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    name: &'static str,
) -> Result<&'a str, SchemaError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| schema_error(name, format!("{field} has the wrong type")))
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
    name: &'static str,
) -> Result<Option<String>, SchemaError> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| schema_error(name, format!("{field} has the wrong type"))),
    }
}

fn optional_object(
    object: &Map<String, Value>,
    field: &str,
    name: &'static str,
) -> Result<Option<Map<String, Value>>, SchemaError> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_object()
            .cloned()
            .map(Some)
            .ok_or_else(|| schema_error(name, format!("{field} must be a JSON object"))),
    }
}

fn optional_transport(
    object: &Map<String, Value>,
    name: &'static str,
) -> Result<Option<Transport>, SchemaError> {
    match object.get("transport") {
        None => Ok(None),
        Some(value) => {
            let value = value
                .as_str()
                .ok_or_else(|| schema_error(name, "transport has the wrong type"))?;
            Transport::parse(value)
                .map(Some)
                .ok_or_else(|| schema_error(name, format!("unknown transport {value}")))
        }
    }
}

fn encode_call(call: &RpcV1Call) -> Result<Vec<u8>, SchemaError> {
    let mut output = String::from("{\"v\":1,\"op\":\"call\",\"id\":");
    push_string(&mut output, &call.id, "rpc-call")?;
    output.push_str(",\"key\":");
    push_string(&mut output, &call.key, "rpc-call")?;
    if let Some(value) = call.transport {
        output.push_str(",\"transport\":");
        push_string(&mut output, value.as_str(), "rpc-call")?;
    }
    push_optional_object(&mut output, "path", call.path.as_ref(), "rpc-call")?;
    push_optional_object(&mut output, "query", call.query.as_ref(), "rpc-call")?;
    push_optional_object(&mut output, "headers", call.headers.as_ref(), "rpc-call")?;
    push_optional_json(&mut output, "body", &call.body, "rpc-call")?;
    push_optional_string(&mut output, "traceId", call.trace_id.as_deref(), "rpc-call")?;
    push_optional_string(&mut output, "spanId", call.span_id.as_deref(), "rpc-call")?;
    output.push('}');
    bounded(output.into_bytes(), "rpc-call")
}

fn encode_receipt(receipt: &RpcV1Receipt) -> Result<Vec<u8>, SchemaError> {
    let mut output = String::from("{\"v\":1,\"op\":\"receipt\",\"id\":");
    push_string(&mut output, &receipt.id, "rpc-receipt")?;
    output.push_str(",\"key\":");
    push_string(&mut output, &receipt.key, "rpc-receipt")?;
    if let Some(value) = receipt.transport {
        output.push_str(",\"transport\":");
        push_string(&mut output, value.as_str(), "rpc-receipt")?;
    }
    output.push_str(",\"ok\":");
    output.push_str(if receipt.ok { "true" } else { "false" });
    if let Some(value) = receipt.status {
        output.push_str(",\"status\":");
        output.push_str(&value.to_string());
    }
    push_optional_json(&mut output, "body", &receipt.body, "rpc-receipt")?;
    push_optional_object(&mut output, "error", receipt.error.as_ref(), "rpc-receipt")?;
    push_optional_string(
        &mut output,
        "traceId",
        receipt.trace_id.as_deref(),
        "rpc-receipt",
    )?;
    push_optional_string(
        &mut output,
        "spanId",
        receipt.span_id.as_deref(),
        "rpc-receipt",
    )?;
    output.push('}');
    bounded(output.into_bytes(), "rpc-receipt")
}

fn push_string(output: &mut String, value: &str, name: &'static str) -> Result<(), SchemaError> {
    output.push_str(
        &serde_json::to_string(value)
            .map_err(|error| schema_error(name, format!("string encode failed: {error}")))?,
    );
    Ok(())
}
fn push_optional_string(
    output: &mut String,
    field: &str,
    value: Option<&str>,
    name: &'static str,
) -> Result<(), SchemaError> {
    if let Some(value) = value {
        output.push_str(",\"");
        output.push_str(field);
        output.push_str("\":");
        push_string(output, value, name)?;
    }
    Ok(())
}
fn push_optional_object(
    output: &mut String,
    field: &str,
    value: Option<&Map<String, Value>>,
    name: &'static str,
) -> Result<(), SchemaError> {
    if let Some(value) = value {
        output.push_str(",\"");
        output.push_str(field);
        output.push_str("\":");
        output.push_str(
            &serde_json::to_string(value)
                .map_err(|error| schema_error(name, format!("JSON encode failed: {error}")))?,
        );
    }
    Ok(())
}
fn push_optional_json(
    output: &mut String,
    field: &str,
    value: &OptionalJson,
    name: &'static str,
) -> Result<(), SchemaError> {
    if let Some(value) = value.value() {
        output.push_str(",\"");
        output.push_str(field);
        output.push_str("\":");
        output.push_str(
            &serde_json::to_string(value)
                .map_err(|error| schema_error(name, format!("JSON encode failed: {error}")))?,
        );
    }
    Ok(())
}
fn bounded(bytes: Vec<u8>, name: &'static str) -> Result<Vec<u8>, SchemaError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return instance(
            name,
            format!(
                "frame is {} bytes, over the {MAX_FRAME_BYTES} limit",
                bytes.len()
            ),
        );
    }
    Ok(bytes)
}
fn schema_error(name: &'static str, detail: impl Into<String>) -> SchemaError {
    SchemaError::Instance {
        name,
        detail: detail.into(),
    }
}
fn instance<T>(name: &'static str, detail: impl Into<String>) -> Result<T, SchemaError> {
    Err(schema_error(name, detail))
}
