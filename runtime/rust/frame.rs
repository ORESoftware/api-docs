//! The RIDL frame envelope: HTTP-free addressing for WebSocket and TCP.
//!
//! This is a standard-library-plus-serde port of `ridl/framing.py`. The
//! fixtures under `examples/frames/` are the byte contract shared by Rust,
//! Dart, Go, TypeScript, and Python. Nothing in this module performs I/O.

use std::collections::BTreeSet;

use serde_json::Value;

pub const FRAME_VERSION: u8 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const LENGTH_PREFIX_BYTES: usize = 4;

const FIELD_ORDER: [&str; 11] = [
    "v", "id", "t", "key", "method", "path", "query", "body", "code", "message", "meta",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Call,
    Data,
    End,
    Error,
    Cancel,
}

impl FrameKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Data => "data",
            Self::End => "end",
            Self::Error => "error",
            Self::Cancel => "cancel",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "call" => Self::Call,
            "data" => Self::Data,
            "end" => Self::End,
            "error" => Self::Error,
            "cancel" => Self::Cancel,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameError(pub String);

impl std::fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FrameError {}

fn err<T>(message: impl Into<String>) -> Result<T, FrameError> {
    Err(FrameError(message.into()))
}

/// One message on a framed transport.
///
/// `None` means the `body` member is absent; `Some(Value::Null)` means the
/// member is present with JSON `null`.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub v: u8,
    pub id: String,
    pub kind: FrameKind,
    pub key: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub meta: Vec<(String, String)>,
}

impl Frame {
    fn bare(kind: FrameKind, id: impl Into<String>) -> Self {
        Self {
            v: FRAME_VERSION,
            id: id.into(),
            kind,
            key: None,
            method: None,
            path: None,
            query: Vec::new(),
            body: None,
            code: None,
            message: None,
            meta: Vec::new(),
        }
    }

    #[must_use]
    pub fn call(
        id: impl Into<String>,
        key: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
        query: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Self {
        let mut frame = Self::bare(FrameKind::Call, id);
        frame.key = Some(key.into());
        frame.method = Some(method.into());
        frame.path = Some(path.into());
        frame.query = query;
        frame.body = body;
        frame
    }

    #[must_use]
    pub fn data(id: impl Into<String>, body: Value) -> Self {
        let mut frame = Self::bare(FrameKind::Data, id);
        frame.body = Some(body);
        frame
    }

    #[must_use]
    pub fn end(id: impl Into<String>) -> Self {
        Self::bare(FrameKind::End, id)
    }

    #[must_use]
    pub fn cancel(id: impl Into<String>) -> Self {
        Self::bare(FrameKind::Cancel, id)
    }

    #[must_use]
    pub fn error(
        id: impl Into<String>,
        code: impl Into<String>,
        message: Option<String>,
    ) -> Self {
        let mut frame = Self::bare(FrameKind::Error, id);
        frame.code = Some(code.into());
        frame.message = message;
        frame
    }

    /// Set one out-of-band string value. Reusing a name replaces its previous
    /// value so encoding can never create duplicate JSON object members.
    #[must_use]
    pub fn with_meta(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let name = name.into();
        let value = value.into();
        if let Some((_, existing)) = self.meta.iter_mut().find(|(key, _)| key == &name) {
            *existing = value;
        } else {
            self.meta.push((name, value));
        }
        self
    }

    #[must_use]
    pub fn meta_get(&self, name: &str) -> Option<&str> {
        self.meta
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn validate(&self) -> Result<(), FrameError> {
        if self.v != FRAME_VERSION {
            return err(format!("unsupported frame version {}", self.v));
        }
        if self.id.is_empty() || self.id.chars().count() > 128 {
            return err("id must be 1..128 characters");
        }
        match self.kind {
            FrameKind::Call => {
                if self.key.as_deref().unwrap_or_default().is_empty() {
                    return err("a call frame needs an operation key");
                }
                if self.method.as_deref().unwrap_or_default().is_empty() {
                    return err("a call frame needs a method");
                }
                if !self.path.as_deref().is_some_and(|path| path.starts_with('/')) {
                    return err("a call frame needs a path starting with /");
                }
            }
            _ => {
                if self.key.is_some()
                    || self.method.is_some()
                    || self.path.is_some()
                    || !self.query.is_empty()
                {
                    return err(format!(
                        "a {} frame carries no addressing fields",
                        self.kind.as_str()
                    ));
                }
            }
        }
        if self.kind == FrameKind::Data && self.body.is_none() {
            return err("a data frame needs a body");
        }
        if self.kind == FrameKind::Error {
            if self.code.as_deref().unwrap_or_default().is_empty() {
                return err("an error frame needs a code");
            }
        } else if self.code.is_some() || self.message.is_some() {
            return err(format!(
                "a {} frame carries no code or message",
                self.kind.as_str()
            ));
        }

        let mut names = BTreeSet::new();
        for (name, _) in &self.meta {
            if !names.insert(name.as_str()) {
                return err(format!("duplicate meta member {name}"));
            }
        }
        Ok(())
    }

    fn write_json(&self, output: &mut String) -> Result<(), FrameError> {
        fn push_string(output: &mut String, value: &str) -> Result<(), FrameError> {
            let encoded = serde_json::to_string(value)
                .map_err(|error| FrameError(format!("encode: {error}")))?;
            output.push_str(&encoded);
            Ok(())
        }

        fn push_value(output: &mut String, value: &Value) -> Result<(), FrameError> {
            let encoded = serde_json::to_string(value)
                .map_err(|error| FrameError(format!("encode: {error}")))?;
            output.push_str(&encoded);
            Ok(())
        }

        output.push('{');
        output.push_str("\"v\":");
        output.push_str(&self.v.to_string());
        output.push_str(",\"id\":");
        push_string(output, &self.id)?;
        output.push_str(",\"t\":");
        push_string(output, self.kind.as_str())?;

        if self.kind == FrameKind::Call {
            output.push_str(",\"key\":");
            push_string(output, self.key.as_deref().unwrap_or_default())?;
            output.push_str(",\"method\":");
            push_string(output, self.method.as_deref().unwrap_or_default())?;
            output.push_str(",\"path\":");
            push_string(output, self.path.as_deref().unwrap_or_default())?;
            if !self.query.is_empty() {
                output.push_str(",\"query\":[");
                for (index, (name, value)) in self.query.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push('[');
                    push_string(output, name)?;
                    output.push(',');
                    push_string(output, value)?;
                    output.push(']');
                }
                output.push(']');
            }
        }

        if let Some(body) = &self.body {
            output.push_str(",\"body\":");
            push_value(output, body)?;
        }

        if self.kind == FrameKind::Error {
            output.push_str(",\"code\":");
            push_string(output, self.code.as_deref().unwrap_or_default())?;
            if let Some(message) = &self.message {
                output.push_str(",\"message\":");
                push_string(output, message)?;
            }
        }

        if !self.meta.is_empty() {
            let mut meta: Vec<&(String, String)> = self.meta.iter().collect();
            meta.sort_by(|left, right| left.0.cmp(&right.0));
            output.push_str(",\"meta\":{");
            for (index, (name, value)) in meta.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                push_string(output, name)?;
                output.push(':');
                push_string(output, value)?;
            }
            output.push('}');
        }

        output.push('}');
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        self.validate()?;
        let mut text = String::new();
        self.write_json(&mut text)?;
        let bytes = text.into_bytes();
        if bytes.len() > MAX_FRAME_BYTES {
            return err(format!(
                "frame is {} bytes, over the {MAX_FRAME_BYTES} limit",
                bytes.len()
            ));
        }
        Ok(bytes)
    }

    pub fn encode_tcp(&self) -> Result<Vec<u8>, FrameError> {
        let payload = self.encode()?;
        let mut output = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
        output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        output.extend_from_slice(&payload);
        Ok(output)
    }

    pub fn decode(payload: &[u8]) -> Result<Self, FrameError> {
        if payload.len() > MAX_FRAME_BYTES {
            return err(format!(
                "frame is {} bytes, over the {MAX_FRAME_BYTES} limit",
                payload.len()
            ));
        }
        let value: Value = serde_json::from_slice(payload)
            .map_err(|error| FrameError(format!("frame is not JSON: {error}")))?;
        let Value::Object(object) = value else {
            return err("a frame must be a JSON object");
        };

        let mut unknown: Vec<&str> = object
            .keys()
            .map(String::as_str)
            .filter(|key| !FIELD_ORDER.contains(key))
            .collect();
        unknown.sort_unstable();
        if !unknown.is_empty() {
            return err(format!("unknown frame member(s): {}", unknown.join(", ")));
        }

        let raw_version = object
            .get("v")
            .and_then(Value::as_u64)
            .ok_or_else(|| FrameError("v has the wrong type".into()))?;
        let version = u8::try_from(raw_version)
            .map_err(|_| FrameError(format!("unsupported frame version {raw_version}")))?;
        let kind = required_string(&object, "t")
            .and_then(|value| {
                FrameKind::parse(value)
                    .ok_or_else(|| FrameError(format!("unknown frame type {value}")))
            })?;

        let mut query = Vec::new();
        if let Some(raw) = object.get("query") {
            let pairs = raw
                .as_array()
                .ok_or_else(|| FrameError("query must be an array of [name, value] pairs".into()))?;
            for pair in pairs {
                let values = pair.as_array().ok_or_else(|| {
                    FrameError("each query entry must be a [name, value] pair".into())
                })?;
                if values.len() != 2 {
                    return err("each query entry must be a [name, value] pair");
                }
                let name = values[0].as_str().ok_or_else(|| {
                    FrameError("each query entry must be a pair of strings".into())
                })?;
                let value = values[1].as_str().ok_or_else(|| {
                    FrameError("each query entry must be a pair of strings".into())
                })?;
                query.push((name.to_owned(), value.to_owned()));
            }
        }

        let mut meta = Vec::new();
        if let Some(raw) = object.get("meta") {
            let map = raw
                .as_object()
                .ok_or_else(|| FrameError("meta must be an object".into()))?;
            for (name, value) in map {
                let value = value
                    .as_str()
                    .ok_or_else(|| FrameError(format!("meta.{name} must be a string")))?;
                meta.push((name.clone(), value.to_owned()));
            }
            meta.sort_by(|left, right| left.0.cmp(&right.0));
        }

        let frame = Self {
            v: version,
            id: required_string(&object, "id")?.to_owned(),
            kind,
            key: optional_string(&object, "key")?,
            method: optional_string(&object, "method")?,
            path: optional_string(&object, "path")?,
            query,
            body: object.get("body").cloned(),
            code: optional_string(&object, "code")?,
            message: optional_string(&object, "message")?,
            meta,
        };
        frame.validate()?;
        Ok(frame)
    }
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, FrameError> {
    object
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| FrameError(format!("{name} has the wrong type")))
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<String>, FrameError> {
    match object.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| FrameError(format!("{name} has the wrong type"))),
    }
}

pub fn decode_stream(buffer: &[u8]) -> Result<(Vec<Frame>, usize), FrameError> {
    let mut frames = Vec::new();
    let mut offset = 0;
    while buffer.len() - offset >= LENGTH_PREFIX_BYTES {
        let length = u32::from_be_bytes(
            buffer[offset..offset + LENGTH_PREFIX_BYTES]
                .try_into()
                .expect("slice length checked"),
        ) as usize;
        if length > MAX_FRAME_BYTES {
            return err(format!(
                "declared frame length {length} is over the {MAX_FRAME_BYTES} limit"
            ));
        }
        let start = offset + LENGTH_PREFIX_BYTES;
        if buffer.len() - start < length {
            break;
        }
        frames.push(Frame::decode(&buffer[start..start + length])?);
        offset = start + length;
    }
    Ok((frames, offset))
}

#[derive(Debug, Default)]
pub struct Correlator {
    prefix: String,
    next: u64,
}

impl Correlator {
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            next: 0,
        }
    }

    pub fn try_take(&mut self) -> Result<String, FrameError> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| FrameError("correlation id counter exhausted".into()))?;
        if self.prefix.is_empty() {
            Ok(self.next.to_string())
        } else {
            Ok(format!("{}{}", self.prefix, self.next))
        }
    }

    #[must_use]
    pub fn take(&mut self) -> String {
        self.try_take()
            .expect("correlation id counter exhausted before process restart")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn call_frame_matches_the_canonical_bytes() {
        let frame = Frame::call(
            "1",
            "walk_matter",
            "POST",
            "/v1/matters/abc/walk",
            vec![("include".into(), "1".into())],
            Some(json!({"choice_id": "c"})),
        );
        assert_eq!(
            String::from_utf8(frame.encode().unwrap()).unwrap(),
            r#"{"v":1,"id":"1","t":"call","key":"walk_matter","method":"POST","path":"/v1/matters/abc/walk","query":[["include","1"]],"body":{"choice_id":"c"}}"#
        );
    }

    #[test]
    fn absent_and_null_bodies_stay_distinct() {
        let absent = Frame::decode(br#"{"v":1,"id":"1","t":"end"}"#).unwrap();
        let null = Frame::decode(br#"{"v":1,"id":"1","t":"data","body":null}"#).unwrap();
        assert!(absent.body.is_none());
        assert_eq!(null.body, Some(Value::Null));
    }

    #[test]
    fn version_narrowing_unknown_members_and_duplicate_meta_fail_closed() {
        assert!(Frame::decode(br#"{"v":257,"id":"1","t":"end"}"#).is_err());
        assert!(Frame::decode(
            br#"{"v":1,"id":"1","t":"end","deadline":"5s"}"#
        )
        .is_err());

        let mut duplicate = Frame::end("1");
        duplicate.meta = vec![("x".into(), "1".into()), ("x".into(), "2".into())];
        assert!(duplicate.validate().is_err());

        let replaced = Frame::end("1").with_meta("x", "1").with_meta("x", "2");
        assert_eq!(replaced.meta_get("x"), Some("2"));
        assert_eq!(replaced.meta.len(), 1);
    }

    #[test]
    fn a_corrupt_prefix_is_rejected_before_allocation() {
        let mut buffer = u32::MAX.to_be_bytes().to_vec();
        buffer.extend_from_slice(b"{}");
        assert!(decode_stream(&buffer).is_err());
    }

    #[test]
    fn a_partial_tail_is_retained() {
        let first = Frame::end("1").encode_tcp().unwrap();
        let second = Frame::cancel("2").encode_tcp().unwrap();
        let mut buffer = first;
        buffer.extend_from_slice(&second[..3]);
        let (frames, consumed) = decode_stream(&buffer).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(buffer.len() - consumed, 3);
    }

    #[test]
    fn every_conformance_fixture_round_trips() {
        let raw = include_str!("../../examples/frames/conformance.json");
        let document: Value = serde_json::from_str(raw).expect("fixtures parse");
        let cases = document["cases"].as_array().expect("cases array");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let encoded = case["encoded"].as_str().unwrap();
            let frame = Frame::decode(encoded.as_bytes())
                .unwrap_or_else(|error| panic!("{name}: decode failed: {error}"));
            assert_eq!(
                String::from_utf8(frame.encode().unwrap()).unwrap(),
                encoded,
                "{name}: canonical bytes"
            );
            assert_eq!(
                hex(&frame.encode_tcp().unwrap()[..LENGTH_PREFIX_BYTES]),
                case["tcp_prefix_hex"].as_str().unwrap(),
                "{name}: length prefix"
            );
        }
    }

    #[test]
    fn correlation_ids_are_monotonic() {
        let mut correlator = Correlator::new("c7-");
        assert_eq!(
            [correlator.take(), correlator.take()],
            ["c7-1", "c7-2"]
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
