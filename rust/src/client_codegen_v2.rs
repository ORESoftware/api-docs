//! Canonical five-language RPC client projection.
//!
//! This v2 bundle deliberately leaves the existing four-runtime compatibility
//! bundle intact. `ores-stack rpc sync` can migrate to this surface while older
//! consumers continue to build. Rust, Go, and Gleam are server callers; Dart is
//! a browser/mobile caller; TypeScript can be emitted into either audience after
//! the owning build tool has filtered the route map.

use serde::Serialize;

use crate::{contract_sha256, rpc_client_bundle, RouteMap};

const GENERATOR: &str = "ores-api-docs rpc_client_bundle_v2";
const RPC_HTTP_PATH: &str = "/v1/rpc";

#[derive(Clone, Debug, Serialize)]
pub struct RpcClientBundleV2Manifest {
    pub schema_version: u32,
    pub generated_by: &'static str,
    pub service: String,
    pub audience: String,
    pub contract_sha256: String,
    pub operations: Vec<String>,
    pub languages: [&'static str; 5],
    pub http_endpoint: &'static str,
}

#[derive(Clone, Debug)]
pub struct RpcClientBundleV2 {
    pub manifest: RpcClientBundleV2Manifest,
    pub rust: String,
    pub go: String,
    pub dart: String,
    pub typescript: String,
    pub gleam: String,
}

/// Generate the canonical five-language v1-over-HTTP client bundle.
///
/// The input map must already be filtered for the target audience. This keeps
/// browser/server admission in the build layer and prevents a generated browser
/// package from merely runtime-rejecting server-only operations.
pub fn rpc_client_bundle_v2(
    map: &RouteMap,
    dto_module: &str,
    audience: &str,
) -> Result<RpcClientBundleV2, String> {
    if audience.trim().is_empty() {
        return Err("client audience must not be empty".to_owned());
    }
    let compatibility = rpc_client_bundle(map, dto_module, audience)?;
    let digest = contract_sha256(map);
    let operations = map.map.keys().cloned().collect::<Vec<_>>();
    let manifest = RpcClientBundleV2Manifest {
        schema_version: 2,
        generated_by: GENERATOR,
        service: map.service.clone(),
        audience: audience.to_owned(),
        contract_sha256: digest.clone(),
        operations,
        languages: ["rust", "go", "dart", "typescript", "gleam"],
        http_endpoint: RPC_HTTP_PATH,
    };
    Ok(RpcClientBundleV2 {
        manifest,
        rust: rust_client(compatibility.rust),
        go: go_client(map, audience, &digest),
        dart: compatibility.dart,
        typescript: compatibility.typescript,
        gleam: gleam_client(map, audience, &digest),
    })
}

fn rust_client(compatibility: String) -> String {
    format!(
        r#"{compatibility}

// Canonical v1-over-HTTP RPC transport. Route marker types above remain useful
// for compile-time Path/Query/Headers/Body/Response typing, but this helper does
// not use their REST method/path projection: RPC always uses POST /v1/rpc.
static __ORES_RPC_SEQUENCE: ::std::sync::atomic::AtomicU64 =
    ::std::sync::atomic::AtomicU64::new(0);

pub async fn rpc_http_call<C>(
    client: &::ores_rpc_calls_http_tcp_pool::HttpRpcClient,
    request: &::ores_rpc_calls_http_tcp_pool::RpcRequest<C>,
) -> ::core::result::Result<
    C::Response,
    ::ores_rpc_calls_http_tcp_pool::RpcError,
>
where
    C: ::ores_rpc_calls_http_tcp_pool::RpcCall,
{{
    use ::ores_rpc_calls_http_tcp_pool::{{HttpMethod, PlainHttpRequest, RpcError}};
    use ::serde_json::{{Map, Value}};

    let key = C::RPC_KEY.unwrap_or(C::KEY);
    let sequence = __ORES_RPC_SEQUENCE.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed);
    let nanos = ::std::time::SystemTime::now()
        .duration_since(::std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let id = format!("rust-{{nanos}}-{{sequence}}");

    let mut envelope = Map::new();
    envelope.insert("v".into(), Value::from(1));
    envelope.insert("op".into(), Value::String("call".into()));
    envelope.insert("id".into(), Value::String(id.clone()));
    envelope.insert("key".into(), Value::String(key.to_owned()));
    envelope.insert("transport".into(), Value::String("http".into()));

    let path = ::serde_json::to_value(&request.path)?;
    if !path.is_null() {{
        envelope.insert("path".into(), path);
    }}
    let query = ::serde_json::to_value(&request.query)?;
    if !query.is_null() {{
        envelope.insert("query".into(), query);
    }}
    let headers = ::serde_json::to_value(&request.headers)?;
    if !headers.is_null() {{
        envelope.insert("headers".into(), headers);
    }}
    if C::HAS_BODY {{
        envelope.insert("body".into(), ::serde_json::to_value(&request.body)?);
    }}
    if let Some(trace_id) = &request.trace_id {{
        envelope.insert("traceId".into(), Value::String(trace_id.clone()));
    }}
    if let Some(span_id) = &request.span_id {{
        envelope.insert("spanId".into(), Value::String(span_id.clone()));
    }}

    let mut outbound = PlainHttpRequest::new(C::SERVICE, key, HttpMethod::Post, RPC_HTTP_PATH)
        .with_json_body(Value::Object(envelope));
    outbound.request_id = request.request_id.clone();
    outbound.traceparent = request.traceparent.clone();
    outbound.tracestate = request.tracestate.clone();
    outbound.deadline_unix_ms = request.deadline_unix_ms;

    let response = client.send_plain(&outbound).await?;
    let receipt: Value = response.json()?;
    let object = receipt
        .as_object()
        .ok_or_else(|| RpcError::Protocol("RPC receipt must be a JSON object".into()))?;
    if object.get("v").and_then(Value::as_u64) != Some(1)
        || object.get("op").and_then(Value::as_str) != Some("receipt")
    {{
        return Err(RpcError::Protocol("invalid RPC receipt version/op".into()));
    }}
    if object.get("id").and_then(Value::as_str) != Some(id.as_str())
        || object.get("key").and_then(Value::as_str) != Some(key)
    {{
        return Err(RpcError::Protocol("RPC receipt correlation mismatch".into()));
    }}

    let ok = object
        .get("ok")
        .and_then(Value::as_bool)
        .ok_or_else(|| RpcError::Protocol("RPC receipt is missing boolean ok".into()))?;
    if !ok {{
        let status = object
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .or(Some(response.status));
        let error = object
            .get("error")
            .cloned()
            .unwrap_or_else(|| ::serde_json::json!({{"message": "remote RPC failed"}}));
        return Err(RpcError::Remote {{
            key: key.to_owned(),
            status,
            error,
        }});
    }}

    let body = object.get("body").cloned().unwrap_or(Value::Null);
    ::serde_json::from_value(body).map_err(RpcError::from)
}}
"#,
    )
}

fn go_client(map: &RouteMap, audience: &str, digest: &str) -> String {
    let allowed = map
        .map
        .keys()
        .map(|key| format!("\t{key:?}: {{}},"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"// Code generated by {generator}; DO NOT EDIT.
package rpc

import (
    "bytes"
    "context"
    "encoding/json"
    "fmt"
    "io"
    "net/http"
    "net/url"
    "strings"
    "sync/atomic"
)

const ContractSHA256 = {digest:?}
const ClientAudience = {audience:?}
const Service = {service:?}
const HTTPPath = {rpc_path:?}

var operations = map[string]struct{{}}{{
{allowed}
}}

type CallArgs struct {{
    Path map[string]any `json:"path,omitempty"`
    Query map[string]any `json:"query,omitempty"`
    Headers map[string]any `json:"headers,omitempty"`
    Body any `json:"body,omitempty"`
    TraceID string `json:"traceId,omitempty"`
    SpanID string `json:"spanId,omitempty"`
}}

type Receipt struct {{
    V int `json:"v"`
    Op string `json:"op"`
    ID string `json:"id"`
    Key string `json:"key"`
    Transport string `json:"transport,omitempty"`
    OK bool `json:"ok"`
    Status int `json:"status,omitempty"`
    Body json.RawMessage `json:"body,omitempty"`
    Error map[string]any `json:"error,omitempty"`
    TraceID string `json:"traceId,omitempty"`
    SpanID string `json:"spanId,omitempty"`
}}

type Client struct {{
    BaseURL *url.URL
    HTTP *http.Client
    sequence atomic.Uint64
}}

func NewClient(baseURL string, client *http.Client) (*Client, error) {{
    parsed, err := url.Parse(baseURL)
    if err != nil {{ return nil, err }}
    if client == nil {{ client = http.DefaultClient }}
    return &Client{{BaseURL: parsed, HTTP: client}}, nil
}}

func (c *Client) Call(ctx context.Context, key string, args CallArgs, out any) error {{
    if _, ok := operations[key]; !ok {{
        return fmt.Errorf("RPC operation not generated for this audience: %s", key)
    }}
    id := fmt.Sprintf("go-%d", c.sequence.Add(1))
    envelope := map[string]any{{
        "v": 1, "op": "call", "id": id, "key": key, "transport": "http",
    }}
    if args.Path != nil {{ envelope["path"] = args.Path }}
    if args.Query != nil {{ envelope["query"] = args.Query }}
    if args.Headers != nil {{ envelope["headers"] = args.Headers }}
    if args.Body != nil {{ envelope["body"] = args.Body }}
    if args.TraceID != "" {{ envelope["traceId"] = args.TraceID }}
    if args.SpanID != "" {{ envelope["spanId"] = args.SpanID }}
    encoded, err := json.Marshal(envelope)
    if err != nil {{ return err }}
    endpoint := *c.BaseURL
    endpoint.Path = strings.TrimRight(endpoint.Path, "/") + HTTPPath
    request, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint.String(), bytes.NewReader(encoded))
    if err != nil {{ return err }}
    request.Header.Set("content-type", "application/json")
    response, err := c.HTTP.Do(request)
    if err != nil {{ return err }}
    defer response.Body.Close()
    raw, err := io.ReadAll(response.Body)
    if err != nil {{ return err }}
    var receipt Receipt
    if err := json.Unmarshal(raw, &receipt); err != nil {{ return err }}
    if receipt.ID != id || receipt.Key != key {{ return fmt.Errorf("RPC receipt correlation mismatch") }}
    if !receipt.OK {{ return fmt.Errorf("RPC %s failed with status %d", key, receipt.Status) }}
    if out != nil && len(receipt.Body) != 0 {{ return json.Unmarshal(receipt.Body, out) }}
    return nil
}}
"#,
        generator = GENERATOR,
        digest = digest,
        audience = audience,
        service = map.service,
        rpc_path = RPC_HTTP_PATH,
        allowed = allowed,
    )
}

fn gleam_client(map: &RouteMap, audience: &str, digest: &str) -> String {
    let allowed = map
        .map
        .keys()
        .map(|key| format!("    {key:?} -> True"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"// @generated by {generator}; do not edit.
import gleam/dynamic
import gleam/http
import gleam/http/request
import gleam/json

pub const rpc_contract_sha256 = {digest:?}
pub const rpc_client_audience = {audience:?}
pub const rpc_service = {service:?}
pub const rpc_http_path = {rpc_path:?}

pub type CallArgs {{
  CallArgs(
    path: List(#(String, json.Json)),
    query: List(#(String, json.Json)),
    headers: List(#(String, json.Json)),
    body: Option(json.Json),
    trace_id: Option(String),
    span_id: Option(String),
  )
}}

pub type Receipt {{
  Receipt(
    id: String,
    key: String,
    ok: Bool,
    status: Option(Int),
    body: Option(dynamic.Dynamic),
  )
}}

pub fn operation_allowed(key: String) -> Bool {{
  case key {{
{allowed}
    _ -> False
  }}
}}

/// Transport implementations remain injectable so server callers can use the
/// project's preferred HTTP pool/TLS stack while the generated operation set,
/// endpoint, correlation rules, and contract digest stay fixed.
pub type Transport {{
  Transport(fn(String, String) -> Result(String, String))
}}

pub fn call(
  transport: Transport,
  base_url: String,
  id: String,
  key: String,
  args: CallArgs,
) -> Result(dynamic.Dynamic, String) {{
  case operation_allowed(key) {{
    False -> Error("RPC operation not generated for this audience")
    True -> {{
      let CallArgs(path, query, headers, body, trace_id, span_id) = args
      let members = [
        #("v", json.int(1)),
        #("op", json.string("call")),
        #("id", json.string(id)),
        #("key", json.string(key)),
        #("transport", json.string("http")),
      ]
      let members = case path {{ [] -> members; _ -> [#("path", json.object(path)), ..members] }}
      let members = case query {{ [] -> members; _ -> [#("query", json.object(query)), ..members] }}
      let members = case headers {{ [] -> members; _ -> [#("headers", json.object(headers)), ..members] }}
      let members = case body {{ None -> members; Some(value) -> [#("body", value), ..members] }}
      let members = case trace_id {{ None -> members; Some(value) -> [#("traceId", json.string(value)), ..members] }}
      let members = case span_id {{ None -> members; Some(value) -> [#("spanId", json.string(value)), ..members] }}
      let Transport(send) = transport
      use response <- result.try(send(base_url <> rpc_http_path, json.to_string(json.object(members))))
      use decoded <- result.try(json.parse(response, dynamic.dynamic))
      Ok(decoded)
    }}
  }}
}}
"#,
        generator = GENERATOR,
        digest = digest,
        audience = audience,
        service = map.service,
        rpc_path = RPC_HTTP_PATH,
        allowed = allowed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_map() -> RouteMap {
        RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"demo-api",
              "map":{
                "ping":{
                  "path":"/v1/ping",
                  "methods":["GET"],
                  "rpc_key":"demo.health.ping",
                  "authorization":{"mode":"public"},
                  "transports":["http"]
                }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("sample route map failed: {error}"))
    }

    #[test]
    fn emits_canonical_five_language_bundle() {
        let bundle = rpc_client_bundle_v2(&sample_map(), "crate::dto", "public")
            .unwrap_or_else(|error| panic!("bundle generation failed: {error}"));
        assert_eq!(
            bundle.manifest.languages,
            ["rust", "go", "dart", "typescript", "gleam"]
        );
        assert_eq!(bundle.manifest.http_endpoint, "/v1/rpc");
        assert!(bundle.rust.contains("pub async fn rpc_http_call<C>("));
        assert!(bundle.rust.contains("HttpMethod::Post, RPC_HTTP_PATH"));
        assert!(bundle.rust.contains("client.send_plain(&outbound).await"));
        assert!(bundle.go.contains("http.MethodPost"));
        assert!(bundle.dart.contains("baseUri.resolve(rpcHttpPath)"));
        assert!(bundle.typescript.contains("new URL(RPC_HTTP_PATH"));
        assert!(bundle.gleam.contains("rpc_http_path"));
    }
}
