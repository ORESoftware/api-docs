//! Namespace-aware structured RPC client projection.
//!
//! v3 preserves the v2 transport and typed-schema semantics while changing the
//! artifact shape: transport/envelope code is returned separately from one
//! typed source unit per handlers-authoritative operation. Consumers such as
//! `ores-stack` can therefore map semantic RPC namespaces onto deterministic
//! filesystem namespaces without parsing a monolithic generated source file.

use serde::Serialize;

use crate::{
    client_codegen_v2::{rpc_client_bundle_v2, RpcClientBundleV2Manifest},
    contract_sha256,
    typed_rpc_sdk_codegen::{typed_sdk_sources, RUST_OPERATION_MARKER},
    RouteMap, RpcOperationContract, RpcStreamMode,
};
const TYPED_MARKER: &str =
    "\n// Typed operation facades from handlers-authoritative normalized IR.\n";

#[derive(Clone, Debug)]
pub struct RpcClientTransportSourcesV3 {
    pub rust: String,
    pub go: String,
    pub dart: String,
    pub typescript: String,
    pub gleam: String,
}

#[derive(Clone, Debug)]
pub struct RpcOperationClientSourcesV3 {
    /// Stable dotted wire identity.
    pub operation_key: String,
    /// Namespace segments after the service prefix and before the operation.
    /// Example: `sonus_auris.admin.version.get_version` -> `["admin", "version"]`.
    pub namespace: Vec<String>,
    /// Handlers-authoritative operation function name, e.g. `get_version`.
    pub operation_name: String,
    /// Handlers-authoritative stream mode. Consumers use this to select the
    /// unary runtime wrapper vs the centralized framed stream package.
    pub stream: RpcStreamMode,
    pub rust: String,
    pub go: String,
    pub dart: String,
    pub typescript: String,
    pub gleam: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RpcClientBundleV3Manifest {
    pub schema_version: u32,
    pub generated_by: &'static str,
    pub service: String,
    pub audience: String,
    pub contract_sha256: String,
    pub operations: Vec<String>,
    pub languages: [&'static str; 5],
    pub http_endpoint: &'static str,
    pub layout: &'static str,
}

#[derive(Clone, Debug)]
pub struct RpcClientBundleV3 {
    pub manifest: RpcClientBundleV3Manifest,
    pub transport: RpcClientTransportSourcesV3,
    pub operations: Vec<RpcOperationClientSourcesV3>,
}

/// Produce transport-only roots plus one deterministic typed source unit per
/// semantic operation. The caller owns final language-specific path/module
/// plumbing; the semantic namespace metadata here is derived only from the
/// stable dotted RPC key.
pub fn rpc_client_bundle_v3(
    map: &RouteMap,
    operations: &[RpcOperationContract],
    dto_module: &str,
    audience: &str,
) -> Result<RpcClientBundleV3, String> {
    // v2 owns the shared unary transport/root source and typed schema projection.
    // Validate stream mode before entering that unary-only layer, and use unary
    // clones only as an internal schema/transport projection. The handlers-
    // authoritative stream mode is retained on the v3 operation unit below.
    let mut transport_contracts = Vec::with_capacity(operations.len());
    for operation in operations {
        match operation.stream {
            RpcStreamMode::Unary => transport_contracts.push(operation.clone()),
            RpcStreamMode::ServerStream => {
                validate_server_stream_contract(operation)?;
                let mut transport_contract = operation.clone();
                transport_contract.stream = RpcStreamMode::Unary;
                transport_contracts.push(transport_contract);
            }
            RpcStreamMode::ClientStream | RpcStreamMode::Bidi => {
                return Err(format!(
                    "{}: generated client operations do not support {} yet; typed outbound stream writes remain fail-closed",
                    operation.operation_key,
                    operation.stream.as_str()
                ));
            }
        }
    }
    let v2 = rpc_client_bundle_v2(map, &transport_contracts, dto_module, audience)?;
    let digest = contract_sha256(map);
    let mut go_transport = transport_prefix(&v2.go, TYPED_MARKER, "go")?;
    go_transport.push_str(
        "\n// Stable raw-outcome bridge used by typed namespace packages. The bridge\n\
         // carries only standard-library JSON types so generated namespace packages do\n\
         // not need a hard-coded import path back to the runtime package.\n\
         func (c *Client) CallJSONOutcome(ctx context.Context, key string, path map[string]any, query map[string]any, headers map[string]any, body any, traceID string, spanID string) (json.RawMessage, json.RawMessage, error) {\n\
         \traw, rpcCtx, err := c.SendRaw(ctx, key, CallArgs{Path: path, Query: query, Headers: headers, Body: body, TraceID: traceID, SpanID: spanID})\n\
         \tif err != nil { return nil, nil, err }\n\
         \tencodedCtx, err := json.Marshal(rpcCtx)\n\
         \tif err != nil { return nil, nil, err }\n\
         \treturn raw, encodedCtx, nil\n\
         }\n\
         func (c *Client) CallJSONRaw(ctx context.Context, key string, path map[string]any, query map[string]any, headers map[string]any, body any, traceID string, spanID string) (json.RawMessage, error) {\n\
         \traw, encodedCtx, err := c.CallJSONOutcome(ctx, key, path, query, headers, body, traceID, spanID)\n\
         \tif err != nil { return nil, err }\n\
         \tvar rpcCtx RpcContext\n\
         \tif err := json.Unmarshal(encodedCtx, &rpcCtx); err != nil { return nil, err }\n\
         \tif !rpcCtx.OK { return nil, &RpcRemoteError{Context: rpcCtx} }\n\
         \treturn raw, nil\n\
         }\n",
    );
    let transport = RpcClientTransportSourcesV3 {
        rust: transport_prefix(&v2.rust, RUST_OPERATION_MARKER, "rust")?,
        go: go_transport,
        dart: transport_prefix(&v2.dart, TYPED_MARKER, "dart")?,
        typescript: transport_prefix(&v2.typescript, TYPED_MARKER, "typescript")?,
        gleam: transport_prefix(&v2.gleam, TYPED_MARKER, "gleam")?,
    };

    let mut operation_sources = Vec::with_capacity(operations.len());
    for operation in operations {
        operation.validate()?;
        let operation_name = operation.source.operation.clone().ok_or_else(|| {
            format!(
                "{}: structured RPC client generation requires source.operation from handlers.rs",
                operation.operation_key
            )
        })?;
        let namespace = semantic_namespace(operation)?;
        match operation.stream {
            RpcStreamMode::Unary | RpcStreamMode::ServerStream => {}
            RpcStreamMode::ClientStream | RpcStreamMode::Bidi => {
                return Err(format!(
                    "{}: generated client operations do not support {} yet; typed outbound stream writes remain fail-closed",
                    operation.operation_key,
                    operation.stream.as_str()
                ));
            }
        }
        if operation.stream == RpcStreamMode::ServerStream {
            validate_server_stream_contract(operation)?;
        }

        // The existing typed schema emitter is deliberately unary-only. For a
        // server-stream operation we borrow only its schema/type projection by
        // cloning the contract as unary, then replace the named operation
        // facade with the framed-stream builder below. No unary transport call
        // from this temporary projection is exposed in the v3 operation unit.
        let schema_contract = if operation.stream == RpcStreamMode::ServerStream {
            let mut schema_contract = operation.clone();
            schema_contract.stream = RpcStreamMode::Unary;
            Some(schema_contract)
        } else {
            None
        };
        let codegen_contract = schema_contract.as_ref().unwrap_or(operation);
        let single = typed_sdk_sources(
            map,
            std::slice::from_ref(codegen_contract),
            audience,
            &digest,
        )?;

        let (rust, go, dart, typescript, gleam) = if operation.stream == RpcStreamMode::ServerStream
        {
            (
                rust_server_stream_operation_source(operation, &single.rust)?,
                go_server_stream_operation_source(operation, &single.go)?,
                dart_server_stream_operation_source(operation, &single.dart)?,
                typescript_server_stream_operation_source(operation, &single.typescript)?,
                gleam_server_stream_operation_source(operation, &single.gleam)?,
            )
        } else {
            (
                rust_operation_source(&single.rust)?,
                go_operation_source(operation, &single.go)?,
                dart_operation_source(operation, &single.dart)?,
                typescript_operation_source(operation, &single.typescript)?,
                trim_typed_marker(&single.gleam).to_owned(),
            )
        };

        operation_sources.push(RpcOperationClientSourcesV3 {
            operation_key: operation.operation_key.clone(),
            namespace,
            operation_name: operation_name.clone(),
            stream: operation.stream,
            rust,
            go,
            dart,
            typescript,
            gleam,
        });
    }
    operation_sources.sort_by(|left, right| left.operation_key.cmp(&right.operation_key));

    let RpcClientBundleV2Manifest {
        service,
        audience,
        contract_sha256,
        operations,
        languages,
        http_endpoint,
        ..
    } = v2.manifest;
    Ok(RpcClientBundleV3 {
        manifest: RpcClientBundleV3Manifest {
            schema_version: 3,
            generated_by: "ores-api-docs rpc_client_bundle_v3",
            service,
            audience,
            contract_sha256,
            operations: sorted_operation_keys(operations),
            languages,
            http_endpoint,
            layout: "namespace-files/v1",
        },
        transport,
        operations: operation_sources,
    })
}

fn validate_server_stream_contract(operation: &RpcOperationContract) -> Result<(), String> {
    let rpc_path = operation.rpc_transport_path.trim();
    if rpc_path.is_empty() || !rpc_path.starts_with('/') {
        return Err(format!(
            "{}: server_stream client generation requires a canonical absolute RPC transport path; route.rs HTTP projection metadata is not the RPC transport authority",
            operation.operation_key
        ));
    }
    if operation.request.path_schema.is_some()
        || operation.request.query_schema.is_some()
        || operation.request.header_schema.is_some()
        || operation.request.body_schema.is_some()
    {
        return Err(format!(
            "{}: server_stream request sections are not yet portable across all five centralized stream clients; only NoSection streams are generated",
            operation.operation_key
        ));
    }
    Ok(())
}

fn rust_server_stream_operation_source(
    operation: &RpcOperationContract,
    source: &str,
) -> Result<String, String> {
    let unary = rust_operation_source(source)?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    let boundary = format!("pub type {pascal}RpcError");
    let (types, _) = unary.split_once(&boundary).ok_or_else(|| {
        format!(
            "{}: Rust typed source is missing operation facade boundary",
            operation.operation_key
        )
    })?;
    Ok(format!(
        "{types}\npub fn {operation_name}<'a, S>(client: &'a ::ores_api_docs_client::OresRpcStreamClient<S>) -> Result<::ores_api_docs_client::RpcStreamCallBuilder<'a, S, {pascal}Response, fn(::serde_json::Value) -> Result<{pascal}Response, String>>, ::ores_api_docs_client::RpcStreamPrepareError>\nwhere\n    S: ::ores_api_docs_client::FramedRpcStream,\n{{\n    fn decode(value: ::serde_json::Value) -> Result<{pascal}Response, String> {{\n        ::serde_json::from_value(value).map_err(|error| error.to_string())\n    }}\n    client.prepare(\n        {key:?},\n        ::ores_api_docs_client::RpcStreamRequest {{\n            method: {method:?}.to_owned(),\n            path: {path:?}.to_owned(),\n            ..::ores_api_docs_client::RpcStreamRequest::default()\n        }},\n        decode as fn(::serde_json::Value) -> Result<{pascal}Response, String>,\n    )\n}}\n",
        key = operation.operation_key,
        method = "POST",
        path = operation.rpc_transport_path,
    ))
}

fn typescript_server_stream_operation_source(
    operation: &RpcOperationContract,
    source: &str,
) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let (types, _) = source
        .split_once("export class TypedRpcClient {")
        .ok_or_else(|| {
            format!(
                "{}: TypeScript typed source is missing TypedRpcClient boundary",
                operation.operation_key
            )
        })?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    let camel = camel(operation_name);
    Ok(format!(
        "import type {{ OresRpcStreamClient, RpcStreamCallBuilder }} from \"@oresoftware/api-docs/stream-rpc\";\n\n{types}\nexport function {camel}(client: OresRpcStreamClient<{key:?}>): RpcStreamCallBuilder<{pascal}Response> {{\n  return client.prepare<{pascal}Response>({key:?}, {{ method: {method:?}, path: {path:?} }}, (value) => value as {pascal}Response);\n}}\n",
        key = operation.operation_key,
        method = "POST",
        path = operation.rpc_transport_path,
    ))
}

fn dart_server_stream_operation_source(
    operation: &RpcOperationContract,
    source: &str,
) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let (types, _) = source.split_once("class TypedRpcClient {").ok_or_else(|| {
        format!(
            "{}: Dart typed source is missing TypedRpcClient boundary",
            operation.operation_key
        )
    })?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    let camel = camel(operation_name);
    Ok(format!(
        "import 'package:ores_api_docs/ores_api_docs.dart';\n\n{types}\nRpcStreamCallBuilder<{pascal}Response> {camel}(OresRpcStreamClient client) {{\n  return client.prepare<{pascal}Response>(\n    {key:?},\n    const RpcStreamRequest(method: {method:?}, path: {path:?}),\n    (raw) => {pascal}Response.fromJson((raw as Map).cast<String, Object?>()),\n  );\n}}\n",
        key = operation.operation_key,
        method = "POST",
        path = operation.rpc_transport_path,
    ))
}

fn go_server_stream_operation_source(
    operation: &RpcOperationContract,
    source: &str,
) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let (types, _) = source.split_once("func (c *Client) ").ok_or_else(|| {
        format!(
            "{}: Go typed source is missing Client method boundary",
            operation.operation_key
        )
    })?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    Ok(format!(
        "import oresapidocs \"github.com/oresoftware/api-docs/clients/go\"\n\n{types}\nfunc {pascal}(client *oresapidocs.OresRPCStreamClient) (*oresapidocs.RPCStreamCallBuilder[{pascal}Response], error) {{\n\treturn oresapidocs.PrepareRPCStream[{pascal}Response](client, {key:?}, oresapidocs.RPCStreamRequest{{Method: {method:?}, Path: {path:?}}}, func(raw json.RawMessage) ({pascal}Response, error) {{\n\t\tvar out {pascal}Response\n\t\terr := json.Unmarshal(raw, &out)\n\t\treturn out, err\n\t}})\n}}\n",
        key = operation.operation_key,
        method = "POST",
        path = operation.rpc_transport_path,
    ))
}

fn gleam_server_stream_operation_source(
    operation: &RpcOperationContract,
    source: &str,
) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let boundary = format!("pub fn {operation_name}(");
    let (types, _) = source.split_once(&boundary).ok_or_else(|| {
        format!(
            "{}: Gleam typed source is missing operation facade boundary",
            operation.operation_key
        )
    })?;
    let pascal = pascal(operation_name);
    Ok(format!(
        "import ores_api_docs/stream_rpc\n{types}\npub fn {operation_name}(transport: stream_rpc.Transport({pascal}Response), id: String) -> stream_rpc.StreamBuilder({pascal}Response) {{\n  stream_rpc.prepare(transport, id, {key:?}, {method:?}, {path:?})\n}}\n",
        key = operation.operation_key,
        method = "POST",
        path = operation.rpc_transport_path,
    ))
}

fn rust_operation_source(source: &str) -> Result<String, String> {
    let (_, suffix) = source.split_once(RUST_OPERATION_MARKER).ok_or_else(|| {
        "rust: typed source is missing the operation-module boundary; refusing to duplicate the root client"
            .to_owned()
    })?;
    let mut operation = String::from(
        "// Root TypedRpcClient and TypedRpcCallError are supplied by the sibling runtime module; ores-stack owns the final module imports.\n",
    );
    operation.push_str(suffix);
    Ok(operation)
}

fn typescript_operation_source(
    operation: &RpcOperationContract,
    source: &str,
) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let (types, _) = source
        .split_once("export class TypedRpcClient {")
        .ok_or_else(|| {
            format!(
                "{}: TypeScript typed source is missing TypedRpcClient boundary",
                operation.operation_key
            )
        })?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    let camel = camel(operation_name);
    let error = if operation.response.error_schema.is_some() {
        format!("{pascal}Error")
    } else {
        "RpcJsonObject".to_owned()
    };
    Ok(format!(
        "{types}\nexport function {camel}(client: RpcClient, input: {pascal}Input): RpcCallBuilder<{pascal}Response, {error}> {{\n  return client.prepare<{pascal}Response, {error}>({key:?}, input);\n}}\n",
        key = operation.operation_key,
    ))
}

fn dart_operation_source(operation: &RpcOperationContract, source: &str) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let (types, _) = source.split_once("class TypedRpcClient {").ok_or_else(|| {
        format!(
            "{}: Dart typed source is missing TypedRpcClient boundary",
            operation.operation_key
        )
    })?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    let camel = camel(operation_name);
    Ok(format!(
        "{types}\nRpcCallBuilder<{pascal}Response> {camel}(OresRpcClient client, {pascal}Input input) {{\n  return client.prepare<{pascal}Response>({key:?}, path: input.pathJson, query: input.queryJson, headers: input.headersJson, body: input.bodyJson, traceId: input.traceId, spanId: input.spanId, decoder: (raw) => {pascal}Response.fromJson((raw as Map).cast<String, Object?>()));\n}}\n",
        key = operation.operation_key,
    ))
}

fn go_operation_source(operation: &RpcOperationContract, source: &str) -> Result<String, String> {
    let source = trim_typed_marker(source);
    let (types, _) = source.split_once("func (c *Client) ").ok_or_else(|| {
        format!(
            "{}: Go typed source is missing Client method boundary",
            operation.operation_key
        )
    })?;
    let operation_name = operation
        .source
        .operation
        .as_deref()
        .ok_or_else(|| format!("{}: source.operation missing", operation.operation_key))?;
    let pascal = pascal(operation_name);
    let transport = format!("{pascal}RpcTransport");
    let call = format!("{pascal}Call");
    let context = format!("{pascal}RpcContext");
    let mapper = format!("to{pascal}Map");
    let path = go_section_expr(operation.request.path_schema.is_some(), "Path", &mapper);
    let query = go_section_expr(operation.request.query_schema.is_some(), "Query", &mapper);
    let headers = go_section_expr(
        operation.request.header_schema.is_some(),
        "Headers",
        &mapper,
    );
    let body = if operation.request.body_schema.is_some() {
        "input.Body"
    } else {
        "nil"
    };
    Ok(format!(
        "{types}\n\
         type {transport} interface {{\n\
         \tCallJSONOutcome(ctx context.Context, key string, path map[string]any, query map[string]any, headers map[string]any, body any, traceID string, spanID string) (json.RawMessage, json.RawMessage, error)\n\
         }}\n\n\
         type {context} struct {{\n\
         \tOK bool `json:\"ok\"`\n\
         \tStatus int `json:\"status\"`\n\
         \tID string `json:\"id\"`\n\
         \tKey string `json:\"key\"`\n\
         \tTransport string `json:\"transport\"`\n\
         \tHeaders map[string]any `json:\"headers\"`\n\
         \tTrailers map[string]any `json:\"trailers\"`\n\
         \tErrors []json.RawMessage `json:\"errors\"`\n\
         \tTraceID string `json:\"traceId,omitempty\"`\n\
         \tTraceIDs []string `json:\"traceIds\"`\n\
         \tSpanID string `json:\"spanId,omitempty\"`\n\
         }}\n\n\
         type {call} struct {{\n\
         \tclient {transport}\n\
         \tpath map[string]any\n\
         \tquery map[string]any\n\
         \theaders map[string]any\n\
         \tbody any\n\
         \ttraceID string\n\
         \tspanID string\n\
         \tbuildErr error\n\
         }}\n\n\
         func {pascal}(client {transport}, input {pascal}Input) *{call} {{\n\
         \treturn &{call}{{client: client, path: {path}, query: {query}, headers: {headers}, body: {body}, traceID: input.TraceID, spanID: input.SpanID}}\n\
         }}\n\n\
         func (c *{call}) AddHeader(name string, value any) *{call} {{\n\
         \tif c.headers == nil {{ c.headers = map[string]any{{}} }}\n\
         \tc.headers[name] = value\n\
         \treturn c\n\
         }}\n\
         func (c *{call}) AddHeaders(values map[string]any) *{call} {{\n\
         \tfor name, value := range values {{ c.AddHeader(name, value) }}\n\
         \treturn c\n\
         }}\n\
         func (c *{call}) AddPathField(name string, value any) *{call} {{\n\
         \tif c.path == nil {{ c.path = map[string]any{{}} }}\n\
         \tc.path[name] = value\n\
         \treturn c\n\
         }}\n\
         func (c *{call}) AddQueryField(name string, value any) *{call} {{\n\
         \tif c.query == nil {{ c.query = map[string]any{{}} }}\n\
         \tc.query[name] = value\n\
         \treturn c\n\
         }}\n\
         func (c *{call}) AddBodyField(name string, value any) *{call} {{\n\
         \tif c.buildErr != nil {{ return c }}\n\
         \tbody, ok := c.body.(map[string]any)\n\
         \tif c.body == nil {{ body = map[string]any{{}}; ok = true }} else if !ok {{\n\
         \t\traw, err := json.Marshal(c.body); if err != nil {{ c.buildErr = err; return c }}\n\
         \t\tif err := json.Unmarshal(raw, &body); err != nil {{ c.buildErr = err; return c }}\n\
         \t}}\n\
         \tbody[name] = value\n\
         \tc.body = body\n\
         \treturn c\n\
         }}\n\
         func (c *{call}) WithBody(value any) *{call} {{ c.body = value; return c }}\n\
         func (c *{call}) MakeCall(ctx context.Context) (*{pascal}Response, {context}, error) {{\n\
         \tvar rpcCtx {context}\n\
         \tif c.buildErr != nil {{ return nil, rpcCtx, c.buildErr }}\n\
         \traw, encodedCtx, err := c.client.CallJSONOutcome(ctx, {key:?}, c.path, c.query, c.headers, c.body, c.traceID, c.spanID)\n\
         \tif err != nil {{ return nil, rpcCtx, err }}\n\
         \tif err := json.Unmarshal(encodedCtx, &rpcCtx); err != nil {{ return nil, rpcCtx, err }}\n\
         \tif len(raw) == 0 {{ return nil, rpcCtx, nil }}\n\
         \tvar out {pascal}Response\n\
         \tif err := json.Unmarshal(raw, &out); err != nil {{ return nil, rpcCtx, err }}\n\
         \treturn &out, rpcCtx, nil\n\
         }}\n\n\
         func {mapper}(value any) map[string]any {{\n\
         \tif value == nil {{ return nil }}\n\
         \traw, err := json.Marshal(value); if err != nil {{ return nil }}\n\
         \tvar out map[string]any; if json.Unmarshal(raw, &out) != nil {{ return nil }}; return out\n\
         }}\n",
        key = operation.operation_key,
    ))
}

fn go_section_expr(present: bool, field: &str, mapper: &str) -> String {
    if present {
        format!("{mapper}(input.{field})")
    } else {
        "nil".to_owned()
    }
}

fn semantic_namespace(operation: &RpcOperationContract) -> Result<Vec<String>, String> {
    let mut segments = operation
        .operation_key
        .split('.')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return Err(format!(
            "{}: client generation requires at least service.operation dotted identity",
            operation.operation_key
        ));
    }
    segments.pop();
    // The first segment is the service/product prefix. It remains in the wire
    // key but is redundant inside that service's client package filesystem.
    segments.remove(0);
    if segments.iter().any(|segment| !portable_segment(segment)) {
        return Err(format!(
            "{}: namespace segments must be lowercase snake_case identifiers",
            operation.operation_key
        ));
    }
    Ok(segments)
}

fn portable_segment(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn sorted_operation_keys(mut operations: Vec<String>) -> Vec<String> {
    operations.sort();
    operations
}

fn transport_prefix(source: &str, marker: &str, language: &str) -> Result<String, String> {
    let (prefix, _) = source.split_once(marker).ok_or_else(|| {
        format!(
            "{language}: v2 generated source did not contain the expected typed-facade marker; refusing to guess transport boundaries"
        )
    })?;
    let mut prefix = prefix.to_owned();
    if !prefix.ends_with('\n') {
        prefix.push('\n');
    }
    Ok(prefix)
}

fn trim_typed_marker(source: &str) -> &str {
    source.strip_prefix(TYPED_MARKER).unwrap_or(source)
}

fn pascal(value: &str) -> String {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let mut out = String::new();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
            }
            out.extend(chars);
            out
        })
        .collect()
}

fn camel(value: &str) -> String {
    let pascal = pascal(value);
    let mut chars = pascal.chars();
    let mut out = String::new();
    if let Some(first) = chars.next() {
        out.push(first.to_ascii_lowercase());
    }
    out.extend(chars);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RpcClientAudience, RpcCodecSet, RpcHttpProjection, RpcOperationScope, RpcOperationSource,
        RpcPayloadCodec, RpcRequestShape, RpcResponseShape, RpcStreamMode,
    };
    use serde_json::json;

    fn operation(key: &str, name: &str) -> RpcOperationContract {
        RpcOperationContract {
            schema_version: crate::RPC_OPERATION_CONTRACT_SCHEMA_VERSION,
            operation_key: key.to_owned(),
            namespace: key
                .split('.')
                .take(key.split('.').count() - 1)
                .map(str::to_owned)
                .collect(),
            source: RpcOperationSource {
                route_file: Some("src/routes/version/route.rs".to_owned()),
                handlers_file: None,
                handler: "get".to_owned(),
                operation: Some(name.to_owned()),
                invoker: Some(format!("__ores_invoke_{name}")),
                execution_model: "shared_operation".to_owned(),
                repository: None,
                commit_sha: None,
            },
            rpc_transport_path: "/v1/rpc",
            http: Some(RpcHttpProjection {
                method: "GET".to_owned(),
                path: "/v1/version".to_owned(),
            }),
            scope: if key.contains(".admin.") {
                RpcOperationScope::Admin
            } else {
                RpcOperationScope::Regular
            },
            stream: RpcStreamMode::Unary,
            audiences: vec![RpcClientAudience::Server],
            codecs: RpcCodecSet {
                allowed: vec![RpcPayloadCodec::Json],
                default: RpcPayloadCodec::Json,
            },
            request: RpcRequestShape {
                path_schema: None,
                query_schema: None,
                header_schema: None,
                body_schema: None,
            },
            response: RpcResponseShape {
                header_schema: Some(json!({
                    "type": "object",
                    "properties": {"etag": {"type": "string"}}
                })),
                trailer_schema: Some(json!({
                    "type": "object",
                    "properties": {"x-ores-checksum": {"type": "string"}}
                })),
                body_schema: Some(json!({
                    "type": "object",
                    "properties": {"version": {"type": "string"}},
                    "required": ["version"]
                })),
                error_schema: Some(json!({
                    "type": "object",
                    "properties": {"code": {"type": "string"}},
                    "required": ["code"]
                })),
            },
            contract_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
        }
    }

    #[test]
    fn derives_semantic_namespace_without_service_prefix() {
        let admin = operation("sonus_auris.admin.version.get_version", "get_version");
        assert_eq!(
            semantic_namespace(&admin).unwrap(),
            vec!["admin".to_owned(), "version".to_owned()]
        );
        let regular = operation("sonus_auris.version.get_version", "get_version");
        assert_eq!(
            semantic_namespace(&regular).unwrap(),
            vec!["version".to_owned()]
        );
        let flat = operation("sonus_auris.get_version", "get_version");
        assert!(semantic_namespace(&flat).unwrap().is_empty());
    }

    #[test]
    fn semantic_namespace_rejects_non_snake_case_segments() {
        let hyphenated = operation("sonus_auris.user-profile.get_version", "get_version");
        let error = semantic_namespace(&hyphenated).expect_err("hyphenated namespace must fail");
        assert!(error.contains("snake_case"));
    }

    #[test]
    fn go_operation_symbols_are_unique_within_one_namespace_package() {
        let first = operation("sonus_auris.version.get_version", "get_version");
        let second = operation("sonus_auris.version.list_versions", "list_versions");
        let first_source = format!(
            "{TYPED_MARKER}type GetVersionInput struct {{}}\ntype GetVersionResponse struct {{}}\nfunc (c *Client) GetVersion"
        );
        let second_source = format!(
            "{TYPED_MARKER}type ListVersionsInput struct {{}}\ntype ListVersionsResponse struct {{}}\nfunc (c *Client) ListVersions"
        );
        let first_out = go_operation_source(&first, &first_source).expect("first Go operation");
        let second_out = go_operation_source(&second, &second_source).expect("second Go operation");

        assert!(first_out.contains("type GetVersionRpcTransport interface"));
        assert!(first_out.contains("CallJSONOutcome("));
        assert!(first_out.contains("func (c *GetVersionCall) MakeCall("));
        assert!(first_out.contains("func toGetVersionMap("));
        assert!(second_out.contains("type ListVersionsRpcTransport interface"));
        assert!(second_out.contains("CallJSONOutcome("));
        assert!(second_out.contains("func toListVersionsMap("));
        assert!(!first_out.contains("type RpcTransport interface"));
        assert!(!first_out.contains("out any"));
        assert!(!second_out.contains("out any"));
        assert!(!second_out.contains("func toMap("));
    }

    #[test]
    fn manifest_operation_keys_are_sorted_deterministically() {
        assert_eq!(
            sorted_operation_keys(vec![
                "sonus_auris.version.list_versions".to_owned(),
                "sonus_auris.admin.version.get_admin_version".to_owned(),
                "sonus_auris.version.get_version".to_owned(),
            ]),
            vec![
                "sonus_auris.admin.version.get_admin_version".to_owned(),
                "sonus_auris.version.get_version".to_owned(),
                "sonus_auris.version.list_versions".to_owned(),
            ]
        );
    }

    #[test]
    fn transport_prefix_fails_closed_without_marker() {
        let error = transport_prefix("transport only", TYPED_MARKER, "typescript")
            .expect_err("missing marker must fail");
        assert!(error.contains("refusing to guess transport boundaries"));
    }
}
