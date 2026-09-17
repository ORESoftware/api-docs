//! Operation-typed five-language RPC SDK projection.
//!
//! The normalized [`RpcOperationContract`] remains the input. TypeSpec and
//! authored JSON Schema stay the wire-shape authorities; this generator only
//! projects the schemas already present in that IR. The generic transport is an
//! implementation detail. Public SDK methods expose operation-specific request,
//! response-header, response-trailer, response-body, and error types.

use std::{collections::BTreeMap, fmt::Write as _};

use serde::Serialize;
use serde_json::Value;

use crate::{RpcClientAudience, RpcOperationContract};

const GENERATOR: &str = "ores-api-docs typed_rpc_client_bundle";
const RPC_HTTP_PATH: &str = "/v1/rpc";

#[derive(Clone, Debug, Serialize)]
pub struct TypedRpcClientBundleManifest {
    pub schema_version: u32,
    pub generated_by: &'static str,
    pub audience: &'static str,
    pub operations: Vec<String>,
    pub operation_contract_sha256: BTreeMap<String, String>,
    pub languages: [&'static str; 5],
    pub http_endpoint: &'static str,
}

#[derive(Clone, Debug)]
pub struct TypedRpcClientBundle {
    pub manifest: TypedRpcClientBundleManifest,
    pub rust: String,
    pub go: String,
    pub dart: String,
    pub typescript: String,
    pub gleam: String,
}

/// Generate a strictly typed public SDK surface from normalized operation IR.
///
/// The caller is expected to pass only one security scope at a time. Audience
/// filtering is repeated here as a fail-closed defense so a browser bundle
/// cannot accidentally contain a server-only operation.
pub fn typed_rpc_client_bundle(
    operations: &[RpcOperationContract],
    audience: RpcClientAudience,
) -> Result<TypedRpcClientBundle, String> {
    let audience_label = match audience {
        RpcClientAudience::Browser => "browser",
        RpcClientAudience::Server => "server",
    };
    let mut selected = operations
        .iter()
        .filter(|operation| operation.audiences.contains(&audience))
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.operation_key.cmp(&right.operation_key));
    if selected.is_empty() {
        return Err(format!(
            "typed RPC SDK contains no operations for {audience_label} audience"
        ));
    }

    let mut names = BTreeMap::<String, String>::new();
    let mut digests = BTreeMap::new();
    for operation in &selected {
        validate_operation(operation)?;
        let name = operation_pascal_name(&operation.operation_key)?;
        if let Some(previous) = names.insert(name.clone(), operation.operation_key.clone()) {
            return Err(format!(
                "operation keys {previous:?} and {:?} both normalize to generated type prefix {name:?}",
                operation.operation_key
            ));
        }
        digests.insert(
            operation.operation_key.clone(),
            operation.contract_sha256.clone(),
        );
    }

    let operation_keys = selected
        .iter()
        .map(|operation| operation.operation_key.clone())
        .collect::<Vec<_>>();
    let manifest = TypedRpcClientBundleManifest {
        schema_version: 1,
        generated_by: GENERATOR,
        audience: audience_label,
        operations: operation_keys,
        operation_contract_sha256: digests,
        languages: ["rust", "go", "dart", "typescript", "gleam"],
        http_endpoint: RPC_HTTP_PATH,
    };

    Ok(TypedRpcClientBundle {
        rust: emit_rust(&selected, audience_label)?,
        go: emit_go(&selected, audience_label)?,
        dart: emit_dart(&selected, audience_label)?,
        typescript: emit_typescript(&selected, audience_label)?,
        gleam: emit_gleam(&selected, audience_label)?,
        manifest,
    })
}

fn validate_operation(operation: &RpcOperationContract) -> Result<(), String> {
    if operation.operation_key.trim().is_empty() || !operation.operation_key.contains('.') {
        return Err(format!(
            "typed RPC operation key {:?} must be a stable dotted identity",
            operation.operation_key
        ));
    }
    if operation.contract_sha256.len() != 64
        || !operation
            .contract_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{} contract_sha256 must be a 64-character hexadecimal digest",
            operation.operation_key
        ));
    }
    for (label, schema) in operation_schemas(operation) {
        if let Some(schema) = schema {
            validate_schema(&operation.operation_key, label, schema)?;
        }
    }
    Ok(())
}

fn operation_schemas(
    operation: &RpcOperationContract,
) -> [(&'static str, Option<&Value>); 8] {
    [
        ("path", operation.request.path_schema.as_ref()),
        ("query", operation.request.query_schema.as_ref()),
        ("request_headers", operation.request.header_schema.as_ref()),
        ("request_body", operation.request.body_schema.as_ref()),
        ("response_headers", operation.response.header_schema.as_ref()),
        ("response_trailers", operation.response.trailer_schema.as_ref()),
        ("response_body", operation.response.body_schema.as_ref()),
        ("error", operation.response.error_schema.as_ref()),
    ]
}

fn validate_schema(operation: &str, label: &str, schema: &Value) -> Result<(), String> {
    let kind = schema_kind(schema).ok_or_else(|| {
        format!(
            "{operation} {label} schema must declare a supported JSON Schema type"
        )
    })?;
    match kind {
        "object" => {
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("{operation} {label} object schema must have properties"))?;
            for (field, property) in properties {
                validate_field_schema(operation, label, field, property)?;
            }
            Ok(())
        }
        "string" | "integer" | "number" | "boolean" => Ok(()),
        "array" => {
            let items = schema.get("items").ok_or_else(|| {
                format!("{operation} {label} array schema must declare items")
            })?;
            validate_scalar_or_array(operation, label, items)
        }
        other => Err(format!(
            "{operation} {label} schema type {other:?} is not supported by strict typed SDK generation"
        )),
    }
}

fn validate_field_schema(
    operation: &str,
    label: &str,
    field: &str,
    schema: &Value,
) -> Result<(), String> {
    validate_scalar_or_array(operation, &format!("{label}.{field}"), schema)
}

fn validate_scalar_or_array(operation: &str, label: &str, schema: &Value) -> Result<(), String> {
    match schema_kind(schema) {
        Some("string" | "integer" | "number" | "boolean") => Ok(()),
        Some("array") => {
            let items = schema
                .get("items")
                .ok_or_else(|| format!("{operation} {label} array schema must declare items"))?;
            match schema_kind(items) {
                Some("string" | "integer" | "number" | "boolean") => Ok(()),
                other => Err(format!(
                    "{operation} {label} array item type {other:?} is not supported; generate a named peer-authority DTO instead"
                )),
            }
        }
        other => Err(format!(
            "{operation} {label} field type {other:?} is not supported; generate a named peer-authority DTO instead"
        )),
    }
}

fn schema_kind(schema: &Value) -> Option<&str> {
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        return Some(kind);
    }
    schema
        .get("type")
        .and_then(Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .find(|kind| *kind != "null")
        })
}

fn nullable(schema: &Value) -> bool {
    schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some("null")))
}

#[derive(Clone, Debug)]
struct ObjectField<'a> {
    wire_name: &'a str,
    schema: &'a Value,
    required: bool,
}

fn object_fields<'a>(schema: &'a Value) -> Result<Vec<ObjectField<'a>>, String> {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "object schema must contain properties".to_owned())?;
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    Ok(properties
        .iter()
        .map(|(name, schema)| ObjectField {
            wire_name: name,
            schema,
            required: required.contains(&name.as_str()),
        })
        .collect())
}

fn operation_pascal_name(key: &str) -> Result<String, String> {
    let terminal = key
        .rsplit('.')
        .next()
        .ok_or_else(|| format!("invalid operation key {key:?}"))?;
    let name = pascal(terminal);
    if name.is_empty() {
        return Err(format!("operation key {key:?} cannot form a generated type name"));
    }
    Ok(name)
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

fn snake(value: &str) -> String {
    let mut out = String::new();
    let mut previous_separator = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && !previous_separator && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_separator = false;
        } else if !out.ends_with('_') {
            out.push('_');
            previous_separator = true;
        }
    }
    out.trim_matches('_').to_owned()
}

fn lower_camel(value: &str) -> String {
    let pascal = pascal(value);
    let mut chars = pascal.chars();
    let mut out = String::new();
    if let Some(first) = chars.next() {
        out.push(first.to_ascii_lowercase());
    }
    out.extend(chars);
    out
}

fn emit_typescript(
    operations: &[&RpcOperationContract],
    audience: &str,
) -> Result<String, String> {
    let mut out = format!(
        "/** @generated by {GENERATOR}; do not edit. */\nexport const RPC_HTTP_PATH = {RPC_HTTP_PATH:?} as const;\nexport const RPC_CLIENT_AUDIENCE = {audience:?} as const;\n\n"
    );
    out.push_str(
        "export interface RpcTransport {\n  call<TRequest, TResult>(key: string, request: TRequest): Promise<TResult>;\n}\n\nexport class TypedRpcClient {\n  private readonly transport: RpcTransport;\n\n  constructor(transport: RpcTransport) {\n    this.transport = transport;\n  }\n",
    );
    for operation in operations {
        let name = operation_pascal_name(&operation.operation_key)?;
        emit_ts_schema(&mut out, &format!("{name}Path"), operation.request.path_schema.as_ref())?;
        emit_ts_schema(&mut out, &format!("{name}Query"), operation.request.query_schema.as_ref())?;
        emit_ts_schema(
            &mut out,
            &format!("{name}RequestHeaders"),
            operation.request.header_schema.as_ref(),
        )?;
        emit_ts_schema(
            &mut out,
            &format!("{name}RequestBody"),
            operation.request.body_schema.as_ref(),
        )?;
        emit_ts_schema(
            &mut out,
            &format!("{name}ResponseHeaders"),
            operation.response.header_schema.as_ref(),
        )?;
        emit_ts_schema(
            &mut out,
            &format!("{name}ResponseTrailers"),
            operation.response.trailer_schema.as_ref(),
        )?;
        emit_ts_schema(
            &mut out,
            &format!("{name}ResponseBody"),
            operation.response.body_schema.as_ref(),
        )?;
        emit_ts_schema(&mut out, &format!("{name}Error"), operation.response.error_schema.as_ref())?;
        writeln!(
            out,
            "\nexport interface {name}Request {{\n  path: {name}Path;\n  query: {name}Query;\n  headers: {name}RequestHeaders;\n  body: {name}RequestBody;\n}}\n\nexport interface {name}Result {{\n  body: {name}ResponseBody;\n  headers: {name}ResponseHeaders;\n  trailers: {name}ResponseTrailers;\n}}"
        )
        .expect("write String");
        let method = lower_camel(
            operation
                .operation_key
                .rsplit('.')
                .next()
                .expect("validated key"),
        );
        writeln!(
            out,
            "\n  async {method}(request: {name}Request): Promise<{name}Result> {{\n    return this.transport.call<{name}Request, {name}Result>({key:?}, request);\n  }}",
            key = operation.operation_key,
        )
        .expect("write String");
    }
    out.push_str("}\n");
    Ok(out)
}

fn emit_ts_schema(out: &mut String, name: &str, schema: Option<&Value>) -> Result<(), String> {
    let Some(schema) = schema else {
        writeln!(out, "\nexport type {name} = undefined;").expect("write String");
        return Ok(());
    };
    if schema_kind(schema) == Some("object") {
        writeln!(out, "\nexport interface {name} {{").expect("write String");
        for field in object_fields(schema)? {
            let optional = if field.required && !nullable(field.schema) {
                ""
            } else {
                "?"
            };
            writeln!(
                out,
                "  {:?}{optional}: {};",
                field.wire_name,
                ts_type(field.schema)?
            )
            .expect("write String");
        }
        out.push_str("}\n");
    } else {
        writeln!(out, "\nexport type {name} = {};", ts_type(schema)?).expect("write String");
    }
    Ok(())
}

fn ts_type(schema: &Value) -> Result<String, String> {
    let base = match schema_kind(schema) {
        Some("string") => "string".to_owned(),
        Some("integer" | "number") => "number".to_owned(),
        Some("boolean") => "boolean".to_owned(),
        Some("array") => format!(
            "ReadonlyArray<{}>",
            ts_type(schema.get("items").ok_or("array items missing")?)?
        ),
        other => return Err(format!("unsupported TypeScript schema type {other:?}")),
    };
    Ok(if nullable(schema) {
        format!("{base} | null")
    } else {
        base
    })
}

fn emit_go(operations: &[&RpcOperationContract], audience: &str) -> Result<String, String> {
    let mut out = format!(
        "// Code generated by {GENERATOR}; DO NOT EDIT.\npackage rpc\n\nimport \"context\"\n\nconst HTTPPath = {RPC_HTTP_PATH:?}\nconst ClientAudience = {audience:?}\n\ntype Transport interface {{\n\tCall(ctx context.Context, key string, request any, result any) error\n}}\n\ntype TypedClient struct {{ transport Transport }}\n\nfunc NewTypedClient(transport Transport) *TypedClient {{ return &TypedClient{{transport: transport}} }}\n"
    );
    for operation in operations {
        let name = operation_pascal_name(&operation.operation_key)?;
        emit_go_schema(&mut out, &format!("{name}Path"), operation.request.path_schema.as_ref())?;
        emit_go_schema(&mut out, &format!("{name}Query"), operation.request.query_schema.as_ref())?;
        emit_go_schema(
            &mut out,
            &format!("{name}RequestHeaders"),
            operation.request.header_schema.as_ref(),
        )?;
        emit_go_schema(
            &mut out,
            &format!("{name}RequestBody"),
            operation.request.body_schema.as_ref(),
        )?;
        emit_go_schema(
            &mut out,
            &format!("{name}ResponseHeaders"),
            operation.response.header_schema.as_ref(),
        )?;
        emit_go_schema(
            &mut out,
            &format!("{name}ResponseTrailers"),
            operation.response.trailer_schema.as_ref(),
        )?;
        emit_go_schema(
            &mut out,
            &format!("{name}ResponseBody"),
            operation.response.body_schema.as_ref(),
        )?;
        emit_go_schema(&mut out, &format!("{name}Error"), operation.response.error_schema.as_ref())?;
        writeln!(
            out,
            "\ntype {name}Request struct {{\n\tPath {name}Path `json:\"path,omitempty\"`\n\tQuery {name}Query `json:\"query,omitempty\"`\n\tHeaders {name}RequestHeaders `json:\"headers,omitempty\"`\n\tBody {name}RequestBody `json:\"body,omitempty\"`\n}}\n\ntype {name}Result struct {{\n\tBody {name}ResponseBody `json:\"body\"`\n\tHeaders {name}ResponseHeaders `json:\"headers,omitempty\"`\n\tTrailers {name}ResponseTrailers `json:\"trailers,omitempty\"`\n}}\n\nfunc (c *TypedClient) {name}(ctx context.Context, request {name}Request) ({name}Result, error) {{\n\tvar result {name}Result\n\terr := c.transport.Call(ctx, {key:?}, request, &result)\n\treturn result, err\n}}",
            key = operation.operation_key,
        )
        .expect("write String");
    }
    Ok(out)
}

fn emit_go_schema(out: &mut String, name: &str, schema: Option<&Value>) -> Result<(), String> {
    let Some(schema) = schema else {
        writeln!(out, "\ntype {name} struct{{}}").expect("write String");
        return Ok(());
    };
    if schema_kind(schema) == Some("object") {
        writeln!(out, "\ntype {name} struct {{").expect("write String");
        for field in object_fields(schema)? {
            let mut ty = go_type(field.schema)?;
            if (!field.required || nullable(field.schema)) && !ty.starts_with("[]") {
                ty = format!("*{ty}");
            }
            writeln!(
                out,
                "\t{} {ty} `json:{:?}`",
                pascal(field.wire_name),
                if field.required && !nullable(field.schema) {
                    field.wire_name.to_owned()
                } else {
                    format!("{},omitempty", field.wire_name)
                }
            )
            .expect("write String");
        }
        out.push_str("}\n");
    } else {
        writeln!(out, "\ntype {name} {}", go_type(schema)?).expect("write String");
    }
    Ok(())
}

fn go_type(schema: &Value) -> Result<String, String> {
    match schema_kind(schema) {
        Some("string") => Ok("string".to_owned()),
        Some("integer") => Ok("int64".to_owned()),
        Some("number") => Ok("float64".to_owned()),
        Some("boolean") => Ok("bool".to_owned()),
        Some("array") => Ok(format!(
            "[]{}",
            go_type(schema.get("items").ok_or("array items missing")?)?
        )),
        other => Err(format!("unsupported Go schema type {other:?}")),
    }
}

fn emit_rust(operations: &[&RpcOperationContract], audience: &str) -> Result<String, String> {
    let mut out = format!(
        "// @generated by {GENERATOR}; do not edit.\nuse std::future::Future;\nuse serde::{{Deserialize, Serialize}};\n\npub const RPC_HTTP_PATH: &str = {RPC_HTTP_PATH:?};\npub const RPC_CLIENT_AUDIENCE: &str = {audience:?};\n\npub trait RpcTransport {{\n    type Error;\n    fn call<Req, Resp>(&self, key: &'static str, request: &Req) -> impl Future<Output = Result<Resp, Self::Error>> + Send\n    where\n        Req: Serialize + Sync,\n        Resp: for<'de> Deserialize<'de> + Send;\n}}\n\npub struct TypedRpcClient<T> {{ transport: T }}\n\nimpl<T> TypedRpcClient<T> {{\n    pub const fn new(transport: T) -> Self {{ Self {{ transport }} }}\n}}\n\nimpl<T: RpcTransport> TypedRpcClient<T> {{\n"
    );
    let mut declarations = String::new();
    for operation in operations {
        let name = operation_pascal_name(&operation.operation_key)?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}Path"),
            operation.request.path_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}Query"),
            operation.request.query_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}RequestHeaders"),
            operation.request.header_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}RequestBody"),
            operation.request.body_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}ResponseHeaders"),
            operation.response.header_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}ResponseTrailers"),
            operation.response.trailer_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}ResponseBody"),
            operation.response.body_schema.as_ref(),
        )?;
        emit_rust_schema(
            &mut declarations,
            &format!("{name}Error"),
            operation.response.error_schema.as_ref(),
        )?;
        writeln!(
            declarations,
            "\n#[derive(Clone, Debug, Serialize, Deserialize)]\npub struct {name}Request {{\n    pub path: {name}Path,\n    pub query: {name}Query,\n    pub headers: {name}RequestHeaders,\n    pub body: {name}RequestBody,\n}}\n\n#[derive(Clone, Debug, Serialize, Deserialize)]\npub struct {name}Result {{\n    pub body: {name}ResponseBody,\n    pub headers: {name}ResponseHeaders,\n    pub trailers: {name}ResponseTrailers,\n}}"
        )
        .expect("write String");
        let method = snake(
            operation
                .operation_key
                .rsplit('.')
                .next()
                .expect("validated key"),
        );
        writeln!(
            out,
            "    pub async fn {method}(&self, request: &{name}Request) -> Result<{name}Result, T::Error> {{\n        self.transport.call({key:?}, request).await\n    }}\n",
            key = operation.operation_key,
        )
        .expect("write String");
    }
    out.push_str("}\n\n");
    out.push_str(&declarations);
    Ok(out)
}

fn emit_rust_schema(out: &mut String, name: &str, schema: Option<&Value>) -> Result<(), String> {
    let Some(schema) = schema else {
        writeln!(
            out,
            "\n#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]\npub struct {name};"
        )
        .expect("write String");
        return Ok(());
    };
    if schema_kind(schema) == Some("object") {
        writeln!(
            out,
            "\n#[derive(Clone, Debug, Serialize, Deserialize)]\npub struct {name} {{"
        )
        .expect("write String");
        for field in object_fields(schema)? {
            let rust_name = rust_field_name(field.wire_name);
            if rust_name != field.wire_name {
                writeln!(out, "    #[serde(rename = {:?})]", field.wire_name)
                    .expect("write String");
            }
            let optional = !field.required || nullable(field.schema);
            if optional {
                out.push_str("    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n");
            }
            let base = rust_type(field.schema)?;
            writeln!(
                out,
                "    pub {rust_name}: {},",
                if optional { format!("Option<{base}>") } else { base }
            )
            .expect("write String");
        }
        out.push_str("}\n");
    } else {
        writeln!(out, "\npub type {name} = {};", rust_type(schema)?).expect("write String");
    }
    Ok(())
}

fn rust_type(schema: &Value) -> Result<String, String> {
    match schema_kind(schema) {
        Some("string") => Ok("String".to_owned()),
        Some("integer") => Ok("i64".to_owned()),
        Some("number") => Ok("f64".to_owned()),
        Some("boolean") => Ok("bool".to_owned()),
        Some("array") => Ok(format!(
            "Vec<{}>",
            rust_type(schema.get("items").ok_or("array items missing")?)?
        )),
        other => Err(format!("unsupported Rust schema type {other:?}")),
    }
}

fn rust_field_name(value: &str) -> String {
    let name = snake(value);
    match name.as_str() {
        "as" | "async" | "await" | "break" | "const" | "continue" | "crate" | "dyn"
        | "else" | "enum" | "extern" | "false" | "fn" | "for" | "if" | "impl"
        | "in" | "let" | "loop" | "match" | "mod" | "move" | "mut" | "pub" | "ref"
        | "return" | "self" | "static" | "struct" | "super" | "trait" | "true"
        | "type" | "unsafe" | "use" | "where" | "while" => format!("r#{name}"),
        _ => name,
    }
}

fn emit_dart(operations: &[&RpcOperationContract], audience: &str) -> Result<String, String> {
    let mut out = format!(
        "// @generated by {GENERATOR}; do not edit.\nconst rpcHttpPath = {RPC_HTTP_PATH:?};\nconst rpcClientAudience = {audience:?};\n\nabstract interface class RpcTransport {{\n  Future<T> call<T>(String key, Object request);\n}}\n\nclass TypedRpcClient {{\n  const TypedRpcClient(this.transport);\n  final RpcTransport transport;\n"
    );
    let mut declarations = String::new();
    for operation in operations {
        let name = operation_pascal_name(&operation.operation_key)?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}Path"),
            operation.request.path_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}Query"),
            operation.request.query_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}RequestHeaders"),
            operation.request.header_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}RequestBody"),
            operation.request.body_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}ResponseHeaders"),
            operation.response.header_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}ResponseTrailers"),
            operation.response.trailer_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}ResponseBody"),
            operation.response.body_schema.as_ref(),
        )?;
        emit_dart_schema(
            &mut declarations,
            &format!("{name}Error"),
            operation.response.error_schema.as_ref(),
        )?;
        writeln!(
            declarations,
            "\nclass {name}Request {{\n  const {name}Request({{required this.path, required this.query, required this.headers, required this.body}});\n  final {name}Path path;\n  final {name}Query query;\n  final {name}RequestHeaders headers;\n  final {name}RequestBody body;\n}}\n\nclass {name}Result {{\n  const {name}Result({{required this.body, required this.headers, required this.trailers}});\n  final {name}ResponseBody body;\n  final {name}ResponseHeaders headers;\n  final {name}ResponseTrailers trailers;\n}}"
        )
        .expect("write String");
        let method = lower_camel(
            operation
                .operation_key
                .rsplit('.')
                .next()
                .expect("validated key"),
        );
        writeln!(
            out,
            "\n  Future<{name}Result> {method}({name}Request request) =>\n      transport.call<{name}Result>({key:?}, request);",
            key = operation.operation_key,
        )
        .expect("write String");
    }
    out.push_str("}\n\n");
    out.push_str(&declarations);
    Ok(out)
}

fn emit_dart_schema(out: &mut String, name: &str, schema: Option<&Value>) -> Result<(), String> {
    let Some(schema) = schema else {
        writeln!(out, "\nclass {name} {{ const {name}(); }}").expect("write String");
        return Ok(());
    };
    if schema_kind(schema) == Some("object") {
        let fields = object_fields(schema)?;
        write!(out, "\nclass {name} {{\n  const {name}({{").expect("write String");
        for field in &fields {
            let dart_name = lower_camel(field.wire_name);
            if field.required && !nullable(field.schema) {
                write!(out, "required this.{dart_name},").expect("write String");
            } else {
                write!(out, "this.{dart_name},").expect("write String");
            }
        }
        out.push_str("});\n");
        for field in fields {
            let optional = !field.required || nullable(field.schema);
            writeln!(
                out,
                "  final {}{} {};",
                dart_type(field.schema)?,
                if optional { "?" } else { "" },
                lower_camel(field.wire_name)
            )
            .expect("write String");
        }
        out.push_str("}\n");
    } else {
        writeln!(
            out,
            "\ntypedef {name} = {};",
            dart_type(schema)?
        )
        .expect("write String");
    }
    Ok(())
}

fn dart_type(schema: &Value) -> Result<String, String> {
    match schema_kind(schema) {
        Some("string") => Ok("String".to_owned()),
        Some("integer") => Ok("int".to_owned()),
        Some("number") => Ok("double".to_owned()),
        Some("boolean") => Ok("bool".to_owned()),
        Some("array") => Ok(format!(
            "List<{}>",
            dart_type(schema.get("items").ok_or("array items missing")?)?
        )),
        other => Err(format!("unsupported Dart schema type {other:?}")),
    }
}

fn emit_gleam(operations: &[&RpcOperationContract], audience: &str) -> Result<String, String> {
    let mut out = format!(
        "// @generated by {GENERATOR}; do not edit.\npub const rpc_http_path = {RPC_HTTP_PATH:?}\npub const rpc_client_audience = {audience:?}\n"
    );
    for operation in operations {
        let name = operation_pascal_name(&operation.operation_key)?;
        emit_gleam_schema(&mut out, &format!("{name}Path"), operation.request.path_schema.as_ref())?;
        emit_gleam_schema(&mut out, &format!("{name}Query"), operation.request.query_schema.as_ref())?;
        emit_gleam_schema(
            &mut out,
            &format!("{name}RequestHeaders"),
            operation.request.header_schema.as_ref(),
        )?;
        emit_gleam_schema(
            &mut out,
            &format!("{name}RequestBody"),
            operation.request.body_schema.as_ref(),
        )?;
        emit_gleam_schema(
            &mut out,
            &format!("{name}ResponseHeaders"),
            operation.response.header_schema.as_ref(),
        )?;
        emit_gleam_schema(
            &mut out,
            &format!("{name}ResponseTrailers"),
            operation.response.trailer_schema.as_ref(),
        )?;
        emit_gleam_schema(
            &mut out,
            &format!("{name}ResponseBody"),
            operation.response.body_schema.as_ref(),
        )?;
        emit_gleam_schema(&mut out, &format!("{name}Error"), operation.response.error_schema.as_ref())?;
        writeln!(
            out,
            "\npub type {name}Request {{\n  {name}Request(path: {name}Path, query: {name}Query, headers: {name}RequestHeaders, body: {name}RequestBody)\n}}\n\npub type {name}Result {{\n  {name}Result(body: {name}ResponseBody, headers: {name}ResponseHeaders, trailers: {name}ResponseTrailers)\n}}"
        )
        .expect("write String");
        let function = snake(
            operation
                .operation_key
                .rsplit('.')
                .next()
                .expect("validated key"),
        );
        writeln!(
            out,
            "\npub fn {function}(send: fn(String, {name}Request) -> Result({name}Result, String), request: {name}Request) -> Result({name}Result, String) {{\n  send({key:?}, request)\n}}",
            key = operation.operation_key,
        )
        .expect("write String");
    }
    Ok(out)
}

fn emit_gleam_schema(out: &mut String, name: &str, schema: Option<&Value>) -> Result<(), String> {
    let Some(schema) = schema else {
        writeln!(out, "\npub type {name} {{ {name} }}").expect("write String");
        return Ok(());
    };
    if schema_kind(schema) == Some("object") {
        writeln!(out, "\npub type {name} {{\n  {name}(").expect("write String");
        let fields = object_fields(schema)?;
        for (index, field) in fields.iter().enumerate() {
            let optional = !field.required || nullable(field.schema);
            let mut ty = gleam_type(field.schema)?;
            if optional {
                ty = format!("Option({ty})");
            }
            writeln!(
                out,
                "    {}: {ty}{},",
                snake(field.wire_name),
                if index + 1 == fields.len() { "" } else { "" }
            )
            .expect("write String");
        }
        out.push_str("  )\n}\n");
    } else {
        writeln!(
            out,
            "\npub type {name} = {}",
            gleam_type(schema)?
        )
        .expect("write String");
    }
    Ok(())
}

fn gleam_type(schema: &Value) -> Result<String, String> {
    match schema_kind(schema) {
        Some("string") => Ok("String".to_owned()),
        Some("integer") => Ok("Int".to_owned()),
        Some("number") => Ok("Float".to_owned()),
        Some("boolean") => Ok("Bool".to_owned()),
        Some("array") => Ok(format!(
            "List({})",
            gleam_type(schema.get("items").ok_or("array items missing")?)?
        )),
        other => Err(format!("unsupported Gleam schema type {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RpcCodecSet, RpcHttpProjection, RpcOperationScope, RpcOperationSource, RpcPayloadCodec,
        RpcRequestShape, RpcResponseShape,
    };
    use serde_json::json;

    fn operation() -> RpcOperationContract {
        RpcOperationContract {
            schema_version: 2,
            operation_key: "demo.users.update_user".to_owned(),
            namespace: vec!["demo".to_owned(), "users".to_owned()],
            source: RpcOperationSource {
                route_file: "src/routes/users/route.rs".to_owned(),
                handler: "patch".to_owned(),
                operation: Some("update_user".to_owned()),
                invoker: Some("__ores_invoke_update_user".to_owned()),
                execution_model: "shared_operation".to_owned(),
                repository: None,
                commit_sha: None,
            },
            http: RpcHttpProjection {
                method: "PATCH".to_owned(),
                path: "/v1/users/{user_id}".to_owned(),
                rpc_transport_path: "/v1/rpc",
            },
            scope: RpcOperationScope::Regular,
            audiences: vec![RpcClientAudience::Browser, RpcClientAudience::Server],
            codecs: RpcCodecSet {
                allowed: vec![RpcPayloadCodec::Json],
                default: RpcPayloadCodec::Json,
            },
            request: RpcRequestShape {
                path_schema: Some(json!({
                    "type": "object",
                    "properties": {"user_id": {"type": "string"}},
                    "required": ["user_id"]
                })),
                query_schema: Some(json!({
                    "type": "object",
                    "properties": {"notify": {"type": "boolean"}}
                })),
                header_schema: Some(json!({
                    "type": "object",
                    "properties": {"x-ores-request-id": {"type": "string"}},
                    "required": ["x-ores-request-id"]
                })),
                body_schema: Some(json!({
                    "type": "object",
                    "properties": {"display_name": {"type": "string"}},
                    "required": ["display_name"]
                })),
            },
            response: RpcResponseShape {
                header_schema: Some(json!({
                    "type": "object",
                    "properties": {"etag": {"type": "string"}}
                })),
                trailer_schema: Some(json!({
                    "type": "object",
                    "properties": {"x-ores-checksum": {"type": "string"}},
                    "required": ["x-ores-checksum"]
                })),
                body_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "user_id": {"type": "string"},
                        "display_name": {"type": "string"}
                    },
                    "required": ["user_id", "display_name"]
                })),
                error_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "code": {"type": "string"},
                        "message": {"type": "string"}
                    },
                    "required": ["code", "message"]
                })),
            },
            contract_sha256:
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        }
    }

    #[test]
    fn emits_named_typed_methods_and_response_metadata_in_all_languages() {
        let bundle = typed_rpc_client_bundle(&[operation()], RpcClientAudience::Browser)
            .expect("typed bundle");
        assert_eq!(
            bundle.manifest.languages,
            ["rust", "go", "dart", "typescript", "gleam"]
        );
        assert!(bundle.rust.contains("update_user"));
        assert!(bundle.rust.contains("UpdateUserResponseTrailers"));
        assert!(bundle.go.contains("func (c *TypedClient) UpdateUser"));
        assert!(bundle.dart.contains("Future<UpdateUserResult> updateUser"));
        assert!(bundle.typescript.contains("async updateUser"));
        assert!(bundle.typescript.contains("UpdateUserResponseTrailers"));
        assert!(bundle.gleam.contains("pub fn update_user"));
    }

    #[test]
    fn public_signatures_do_not_fall_back_to_dynamic_json_containers() {
        let bundle = typed_rpc_client_bundle(&[operation()], RpcClientAudience::Browser)
            .expect("typed bundle");
        assert!(!bundle.typescript.contains("Record<string, unknown>"));
        assert!(!bundle.go.contains("map[string]any"));
        assert!(!bundle.rust.contains("serde_json::Value"));
        assert!(!bundle.gleam.contains("dynamic.Dynamic"));
    }

    #[test]
    fn unsupported_nested_object_fails_closed() {
        let mut operation = operation();
        operation.request.body_schema = Some(json!({
            "type": "object",
            "properties": {
                "profile": {
                    "type": "object",
                    "properties": {"name": {"type": "string"}}
                }
            }
        }));
        let error = typed_rpc_client_bundle(&[operation], RpcClientAudience::Browser)
            .expect_err("nested anonymous object must fail");
        assert!(error.contains("named peer-authority DTO"), "{error}");
    }

    #[test]
    fn browser_bundle_excludes_server_only_operation() {
        let mut operation = operation();
        operation.audiences = vec![RpcClientAudience::Server];
        let error = typed_rpc_client_bundle(&[operation], RpcClientAudience::Browser)
            .expect_err("browser bundle must not widen audience");
        assert!(error.contains("no operations for browser audience"));
    }
}
