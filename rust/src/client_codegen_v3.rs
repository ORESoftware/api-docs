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
    typed_rpc_sdk_codegen::typed_sdk_sources,
    RouteMap, RpcOperationContract,
};

const RUST_TYPED_MARKER: &str =
    "\n// Typed canonical POST /v1/rpc facade from handlers-authoritative normalized IR.\n";
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
    let v2 = rpc_client_bundle_v2(map, operations, dto_module, audience)?;
    let digest = contract_sha256(map);
    let mut go_transport = transport_prefix(&v2.go, TYPED_MARKER, "go")?;
    go_transport.push_str(
        "\n// Stable built-in-type bridge used by namespace packages.\n\
         func (c *Client) CallJSON(ctx context.Context, key string, path map[string]any, query map[string]any, headers map[string]any, body any, traceID string, spanID string, out any) error {\n\
         \treturn c.Call(ctx, key, CallArgs{Path: path, Query: query, Headers: headers, Body: body, TraceID: traceID, SpanID: spanID}, out)\n\
         }\n",
    );
    let transport = RpcClientTransportSourcesV3 {
        rust: transport_prefix(&v2.rust, RUST_TYPED_MARKER, "rust")?,
        go: go_transport,
        dart: transport_prefix(&v2.dart, TYPED_MARKER, "dart")?,
        typescript: transport_prefix(&v2.typescript, TYPED_MARKER, "typescript")?,
        gleam: transport_prefix(&v2.gleam, TYPED_MARKER, "gleam")?,
    };

    let mut operation_sources = Vec::with_capacity(operations.len());
    for operation in operations {
        let operation_name = operation.source.operation.clone().ok_or_else(|| {
            format!(
                "{}: structured RPC client generation requires source.operation from handlers.rs",
                operation.operation_key
            )
        })?;
        let namespace = semantic_namespace(operation)?;
        let single = typed_sdk_sources(map, std::slice::from_ref(operation), audience, &digest)?;
        operation_sources.push(RpcOperationClientSourcesV3 {
            operation_key: operation.operation_key.clone(),
            namespace,
            operation_name: operation_name.clone(),
            rust: single.rust,
            go: go_operation_source(operation, &single.go)?,
            dart: dart_operation_source(operation, &single.dart)?,
            typescript: typescript_operation_source(operation, &single.typescript)?,
            gleam: trim_typed_marker(&single.gleam).to_owned(),
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
            operations,
            languages,
            http_endpoint,
            layout: "namespace-files/v1",
        },
        transport,
        operations: operation_sources,
    })
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
    Ok(format!(
        "{types}\nexport async function {camel}(client: RpcClient, input: {pascal}Input): Promise<{pascal}Response> {{\n  return (await client.call({key:?}, input)) as {pascal}Response;\n}}\n",
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
        "{types}\nFuture<{pascal}Response> {camel}(OresRpcClient client, {pascal}Input input) async {{\n  final raw = await client.call({key:?}, path: input.pathJson, query: input.queryJson, headers: input.headersJson, body: input.bodyJson, traceId: input.traceId, spanId: input.spanId);\n  return {pascal}Response.fromJson((raw as Map).cast<String, Object?>());\n}}\n",
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
    let path = go_section_expr(operation.request.path_schema.is_some(), "Path");
    let query = go_section_expr(operation.request.query_schema.is_some(), "Query");
    let headers = go_section_expr(operation.request.header_schema.is_some(), "Headers");
    let body = if operation.request.body_schema.is_some() {
        "input.Body"
    } else {
        "nil"
    };
    Ok(format!(
        "{types}\ntype RpcTransport interface {{\n\tCallJSON(ctx context.Context, key string, path map[string]any, query map[string]any, headers map[string]any, body any, traceID string, spanID string, out any) error\n}}\n\nfunc {pascal}(ctx context.Context, client RpcTransport, input {pascal}Input) ({pascal}Response, error) {{\n\tvar out {pascal}Response\n\terr := client.CallJSON(ctx, {key:?}, {path}, {query}, {headers}, {body}, input.TraceID, input.SpanID, &out)\n\treturn out, err\n}}\n\nfunc toMap(value any) map[string]any {{\n\tif value == nil {{ return nil }}\n\traw, err := json.Marshal(value); if err != nil {{ return nil }}\n\tvar out map[string]any; if json.Unmarshal(raw, &out) != nil {{ return nil }}; return out\n}}\n",
        key = operation.operation_key,
    ))
}

fn go_section_expr(present: bool, field: &str) -> String {
    if present {
        format!("toMap(input.{field})")
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
    if segments.len() < 3 {
        return Err(format!(
            "{}: namespaced client generation requires service.namespace.operation dotted identity",
            operation.operation_key
        ));
    }
    segments.pop();
    // The first segment is the service/product prefix. It remains in the wire
    // key but is redundant inside that service's client package filesystem.
    segments.remove(0);
    if segments.iter().any(|segment| !portable_segment(segment)) {
        return Err(format!(
            "{}: namespace segments must be lowercase portable identifiers",
            operation.operation_key
        ));
    }
    Ok(segments)
}

fn portable_segment(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
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
        RpcPayloadCodec, RpcRequestShape, RpcResponseShape,
    };
    use serde_json::json;

    fn operation(key: &str, name: &str) -> RpcOperationContract {
        RpcOperationContract {
            schema_version: 2,
            operation_key: key.to_owned(),
            namespace: key
                .split('.')
                .take(key.split('.').count() - 1)
                .map(str::to_owned)
                .collect(),
            source: RpcOperationSource {
                route_file: "src/routes/version/route.rs".to_owned(),
                handler: "get".to_owned(),
                operation: Some(name.to_owned()),
                invoker: Some(format!("__ores_invoke_{name}")),
                execution_model: "shared_operation".to_owned(),
                repository: None,
                commit_sha: None,
            },
            http: RpcHttpProjection {
                method: "GET".to_owned(),
                path: "/v1/version".to_owned(),
                rpc_transport_path: "/v1/rpc",
            },
            scope: if key.contains(".admin.") {
                RpcOperationScope::Admin
            } else {
                RpcOperationScope::Regular
            },
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
    }

    #[test]
    fn transport_prefix_fails_closed_without_marker() {
        let error = transport_prefix("transport only", TYPED_MARKER, "typescript")
            .expect_err("missing marker must fail");
        assert!(error.contains("refusing to guess transport boundaries"));
    }
}
