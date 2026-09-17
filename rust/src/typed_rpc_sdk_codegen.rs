//! Typed RPC SDK facades generated from normalized operation IR.
//!
//! The low-level transport adapters remain reusable implementation details. This
//! module emits the public, operation-specific surface: named methods sourced
//! from `handlers.rs` plus schema-derived request/response/error DTOs. Unsupported
//! JSON Schema constructs fail generation instead of degrading to dynamic types.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::{RouteMap, RpcOperationContract};

pub(crate) struct TypedSdkSources {
    pub rust: String,
    pub go: String,
    pub dart: String,
    pub typescript: String,
    pub gleam: String,
}

#[derive(Clone, Debug)]
enum SchemaKind {
    String,
    Integer,
    Number,
    Boolean,
    Array(Box<Schema>),
    Map(Box<Schema>),
    Object(Vec<Field>),
    StringEnum(Vec<String>),
}

#[derive(Clone, Debug)]
struct Schema {
    nullable: bool,
    kind: SchemaKind,
}

#[derive(Clone, Debug)]
struct Field {
    wire: String,
    required: bool,
    schema: Schema,
}

#[derive(Clone, Debug)]
struct Operation<'a> {
    contract: &'a RpcOperationContract,
    rust_fn: String,
    pascal: String,
    camel: String,
    request: Sections,
    response: Sections,
}

#[derive(Clone, Debug, Default)]
struct Sections {
    path: Option<Schema>,
    query: Option<Schema>,
    headers: Option<Schema>,
    body: Option<Schema>,
    response_headers: Option<Schema>,
    response_trailers: Option<Schema>,
    response: Option<Schema>,
    error: Option<Schema>,
}

pub(crate) fn typed_sdk_sources(
    map: &RouteMap,
    contracts: &[RpcOperationContract],
    audience: &str,
    digest: &str,
) -> Result<TypedSdkSources, String> {
    let operations = normalize_operations(map, contracts)?;
    Ok(TypedSdkSources {
        rust: emit_rust(map, &operations, audience, digest)?,
        go: emit_go(&operations)?,
        dart: emit_dart(&operations)?,
        typescript: emit_typescript(&operations)?,
        gleam: emit_gleam(&operations)?,
    })
}

fn normalize_operations<'a>(
    map: &RouteMap,
    contracts: &'a [RpcOperationContract],
) -> Result<Vec<Operation<'a>>, String> {
    let mut keys = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut out = Vec::with_capacity(contracts.len());
    for contract in contracts {
        if contract.source.execution_model != "shared_operation" {
            return Err(format!(
                "{}: typed SDK generation requires shared_operation execution model, got {:?}",
                contract.operation_key, contract.source.execution_model
            ));
        }
        let rust_fn = contract.source.operation.as_deref().ok_or_else(|| {
            format!(
                "{}: typed SDK generation requires source.operation from handlers.rs",
                contract.operation_key
            )
        })?;
        if !is_identifier(rust_fn) {
            return Err(format!(
                "{}: handlers.rs operation name {rust_fn:?} is not a portable identifier",
                contract.operation_key
            ));
        }
        if !keys.insert(contract.operation_key.clone()) {
            return Err(format!(
                "duplicate typed SDK operation key {:?}",
                contract.operation_key
            ));
        }
        if !names.insert(rust_fn.to_owned()) {
            return Err(format!(
                "duplicate typed SDK handlers.rs operation name {rust_fn:?}"
            ));
        }
        let present = map.map.values().any(|entry| {
            entry.rpc_key.as_deref() == Some(contract.operation_key.as_str())
                || (entry.rpc_key.is_none() && map.map.contains_key(&contract.operation_key))
        });
        if !present {
            return Err(format!(
                "{}: normalized operation is not present in the audience-filtered route map",
                contract.operation_key
            ));
        }
        let request = Sections {
            path: parse_optional(contract.request.path_schema.as_ref(), "request.path")?,
            query: parse_optional(contract.request.query_schema.as_ref(), "request.query")?,
            headers: parse_optional(contract.request.header_schema.as_ref(), "request.headers")?,
            body: parse_optional(contract.request.body_schema.as_ref(), "request.body")?,
            ..Sections::default()
        };
        let response = Sections {
            response_headers: parse_optional(
                contract.response.header_schema.as_ref(),
                "response.headers",
            )?,
            response_trailers: parse_optional(
                contract.response.trailer_schema.as_ref(),
                "response.trailers",
            )?,
            response: parse_optional(contract.response.body_schema.as_ref(), "response.body")?,
            error: parse_optional(contract.response.error_schema.as_ref(), "response.error")?,
            ..Sections::default()
        };
        out.push(Operation {
            contract,
            rust_fn: rust_fn.to_owned(),
            pascal: pascal(rust_fn),
            camel: camel(rust_fn),
            request,
            response,
        });
    }
    out.sort_by(|left, right| {
        left.contract
            .operation_key
            .cmp(&right.contract.operation_key)
    });
    Ok(out)
}

fn parse_optional(value: Option<&Value>, label: &str) -> Result<Option<Schema>, String> {
    value.map(|value| parse_schema(value, label)).transpose()
}

fn parse_schema(value: &Value, label: &str) -> Result<Schema, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label}: JSON Schema must be an object"))?;
    for keyword in [
        "$ref", "oneOf", "anyOf", "allOf", "not", "if", "then", "else",
    ] {
        if object.contains_key(keyword) {
            return Err(format!(
                "{label}: JSON Schema keyword {keyword:?} is not supported by typed RPC SDK generation yet"
            ));
        }
    }

    let mut nullable = false;
    let kind_name = match object.get("type") {
        Some(Value::String(kind)) => kind.as_str(),
        Some(Value::Array(kinds)) => {
            let mut concrete = None;
            for kind in kinds {
                let kind = kind
                    .as_str()
                    .ok_or_else(|| format!("{label}: schema type array must contain strings"))?;
                if kind == "null" {
                    nullable = true;
                } else if concrete.replace(kind).is_some() {
                    return Err(format!(
                        "{label}: union schema types other than nullable T are not supported"
                    ));
                }
            }
            concrete.ok_or_else(|| format!("{label}: nullable schema must include a concrete type"))?
        }
        None if object.contains_key("properties") => "object",
        None if object.contains_key("enum") => "string",
        None => {
            return Err(format!(
                "{label}: schema must declare a concrete type; untyped JSON is forbidden in generated RPC SDKs"
            ))
        }
        Some(_) => return Err(format!("{label}: schema type must be a string or string array")),
    };

    let kind = if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .ok_or_else(|| format!("{label}: enum must be an array"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{label}: only string enums are supported"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.is_empty() {
            return Err(format!("{label}: enum must not be empty"));
        }
        SchemaKind::StringEnum(values)
    } else {
        match kind_name {
            "string" => SchemaKind::String,
            "integer" => SchemaKind::Integer,
            "number" => SchemaKind::Number,
            "boolean" => SchemaKind::Boolean,
            "array" => {
                let items = object
                    .get("items")
                    .ok_or_else(|| format!("{label}: array schema requires items"))?;
                SchemaKind::Array(Box::new(parse_schema(items, &format!("{label}.items"))?))
            }
            "object" => parse_object(object, label)?,
            other => return Err(format!("{label}: unsupported JSON Schema type {other:?}")),
        }
    };
    Ok(Schema { nullable, kind })
}

fn parse_object(
    object: &serde_json::Map<String, Value>,
    label: &str,
) -> Result<SchemaKind, String> {
    let properties = object.get("properties").and_then(Value::as_object);
    let additional = object.get("additionalProperties");
    if properties.is_none() {
        return match additional {
            Some(Value::Object(schema)) => Ok(SchemaKind::Map(Box::new(parse_schema(
                &Value::Object(schema.clone()),
                &format!("{label}.additionalProperties"),
            )?))),
            Some(Value::Bool(false)) => Ok(SchemaKind::Object(Vec::new())),
            _ => Err(format!(
                "{label}: open/untyped object schemas are forbidden; declare properties or a typed additionalProperties schema"
            )),
        };
    }
    if !matches!(additional, None | Some(Value::Bool(false))) {
        return Err(format!(
            "{label}: objects with both fixed properties and additionalProperties are not supported yet"
        ));
    }
    let required = object
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let mut fields = Vec::new();
    for (wire, schema) in properties.expect("checked") {
        fields.push(Field {
            wire: wire.clone(),
            required: required.contains(wire.as_str()),
            schema: parse_schema(schema, &format!("{label}.properties.{wire}"))?,
        });
    }
    Ok(SchemaKind::Object(fields))
}

fn emit_typescript(operations: &[Operation<'_>]) -> Result<String, String> {
    let mut out =
        String::from("\n// Typed operation facades from handlers-authoritative normalized IR.\n");
    for operation in operations {
        emit_ts_section_types(&mut out, operation)?;
    }
    out.push_str("\nexport class TypedRpcClient {\n  constructor(private readonly transport: RpcClient) {}\n");
    for operation in operations {
        let input = format!("{}Input", operation.pascal);
        let response = format!("{}Response", operation.pascal);
        out.push_str(&format!(
            "  async {}(input: {}): Promise<{}> {{\n    return (await this.transport.call({:?}, input)) as {};\n  }}\n",
            operation.camel, input, response, operation.contract.operation_key, response
        ));
    }
    out.push_str("}\n");
    Ok(out)
}

fn emit_ts_section_types(out: &mut String, operation: &Operation<'_>) -> Result<(), String> {
    let mut input_fields = Vec::new();
    for (suffix, field, schema) in request_sections(operation) {
        let name = format!("{}{}", operation.pascal, suffix);
        emit_ts_type(out, &name, schema)?;
        input_fields.push(format!("  {field}: {name};"));
    }
    input_fields.push("  traceId?: string;".to_owned());
    input_fields.push("  spanId?: string;".to_owned());
    out.push_str(&format!(
        "export interface {}Input {{\n{}\n}}\n",
        operation.pascal,
        input_fields.join("\n")
    ));
    let response = operation.response.response.as_ref().ok_or_else(|| {
        format!(
            "{}: normalized IR is missing response.body schema",
            operation.contract.operation_key
        )
    })?;
    emit_ts_type(out, &format!("{}Response", operation.pascal), response)?;
    if let Some(error) = operation.response.error.as_ref() {
        emit_ts_type(out, &format!("{}Error", operation.pascal), error)?;
    }
    if let Some(headers) = operation.response.response_headers.as_ref() {
        emit_ts_type(
            out,
            &format!("{}ResponseHeaders", operation.pascal),
            headers,
        )?;
    }
    if let Some(trailers) = operation.response.response_trailers.as_ref() {
        emit_ts_type(
            out,
            &format!("{}ResponseTrailers", operation.pascal),
            trailers,
        )?;
    }
    Ok(())
}

fn emit_ts_type(out: &mut String, name: &str, schema: &Schema) -> Result<(), String> {
    match &schema.kind {
        SchemaKind::Object(fields) => {
            for field in fields {
                emit_ts_nested(out, name, field)?;
            }
            out.push_str(&format!("export interface {name} {{\n"));
            for field in fields {
                let ty = ts_type(&field.schema, &format!("{name}{}", pascal(&field.wire)))?;
                let optional = if field.required { "" } else { "?" };
                out.push_str(&format!("  {:?}{optional}: {ty};\n", field.wire));
            }
            out.push_str("}\n");
        }
        SchemaKind::StringEnum(values) => {
            out.push_str(&format!(
                "export type {name} = {};\n",
                values
                    .iter()
                    .map(|value| format!("{value:?}"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        _ => out.push_str(&format!(
            "export type {name} = {};\n",
            ts_type(schema, name)?
        )),
    }
    Ok(())
}

fn emit_ts_nested(out: &mut String, parent: &str, field: &Field) -> Result<(), String> {
    let nested = format!("{parent}{}", pascal(&field.wire));
    match &field.schema.kind {
        SchemaKind::Object(_) | SchemaKind::StringEnum(_) => {
            emit_ts_type(out, &nested, &field.schema)
        }
        SchemaKind::Array(item)
            if matches!(item.kind, SchemaKind::Object(_) | SchemaKind::StringEnum(_)) =>
        {
            emit_ts_type(out, &format!("{nested}Item"), item)
        }
        _ => Ok(()),
    }
}

fn ts_type(schema: &Schema, hint: &str) -> Result<String, String> {
    let base = match &schema.kind {
        SchemaKind::String => "string".to_owned(),
        SchemaKind::Integer | SchemaKind::Number => "number".to_owned(),
        SchemaKind::Boolean => "boolean".to_owned(),
        SchemaKind::StringEnum(_) | SchemaKind::Object(_) => hint.to_owned(),
        SchemaKind::Array(item) => format!("Array<{}>", ts_type(item, &format!("{hint}Item"))?),
        SchemaKind::Map(value) => format!(
            "Record<string, {}>",
            ts_type(value, &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("{base} | null")
    } else {
        base
    })
}

fn emit_go(operations: &[Operation<'_>]) -> Result<String, String> {
    let mut out =
        String::from("\n// Typed operation facades from handlers-authoritative normalized IR.\n");
    for operation in operations {
        emit_go_section_types(&mut out, operation)?;
        let sections = request_sections(operation);
        let section_expr = |field: &str, present: &str| {
            if sections.iter().any(|(_, candidate, _)| *candidate == field) {
                present.to_owned()
            } else {
                "nil".to_owned()
            }
        };
        let path = section_expr("path", "toMap(input.Path)");
        let query = section_expr("query", "toMap(input.Query)");
        let headers = section_expr("headers", "toMap(input.Headers)");
        let body = section_expr("body", "input.Body");
        out.push_str(&format!(
            "func (c *Client) {}(ctx context.Context, input {}Input) ({}Response, error) {{\n\tvar out {}Response\n\terr := c.Call(ctx, {:?}, CallArgs{{Path: {path}, Query: {query}, Headers: {headers}, Body: {body}, TraceID: input.TraceID, SpanID: input.SpanID}}, &out)\n\treturn out, err\n}}\n",
            operation.pascal,
            operation.pascal,
            operation.pascal,
            operation.pascal,
            operation.contract.operation_key,
        ));
    }
    out.push_str(
        "func toMap(value any) map[string]any {\n\tif value == nil { return nil }\n\traw, err := json.Marshal(value); if err != nil { return nil }\n\tvar out map[string]any; if json.Unmarshal(raw, &out) != nil { return nil }; return out\n}\n",
    );
    Ok(out)
}

fn emit_go_section_types(out: &mut String, operation: &Operation<'_>) -> Result<(), String> {
    let mut fields = Vec::new();
    for (suffix, field, schema) in request_sections(operation) {
        let name = format!("{}{}", operation.pascal, suffix);
        emit_go_type(out, &name, schema)?;
        fields.push(format!("\t{} {} `json:\"-\"`", pascal(field), name));
    }
    fields.push("\tTraceID string `json:\"-\"`".to_owned());
    fields.push("\tSpanID string `json:\"-\"`".to_owned());
    out.push_str(&format!(
        "type {}Input struct {{\n{}\n}}\n",
        operation.pascal,
        fields.join("\n")
    ));
    let response = operation.response.response.as_ref().ok_or_else(|| {
        format!(
            "{}: normalized IR is missing response.body schema",
            operation.contract.operation_key
        )
    })?;
    emit_go_type(out, &format!("{}Response", operation.pascal), response)?;
    if let Some(error) = operation.response.error.as_ref() {
        emit_go_type(out, &format!("{}Error", operation.pascal), error)?;
    }
    Ok(())
}

fn emit_go_type(out: &mut String, name: &str, schema: &Schema) -> Result<(), String> {
    match &schema.kind {
        SchemaKind::Object(fields) => {
            for field in fields {
                let nested = format!("{name}{}", pascal(&field.wire));
                match &field.schema.kind {
                    SchemaKind::Object(_) | SchemaKind::StringEnum(_) => {
                        emit_go_type(out, &nested, &field.schema)?
                    }
                    SchemaKind::Array(item)
                        if matches!(
                            item.kind,
                            SchemaKind::Object(_) | SchemaKind::StringEnum(_)
                        ) =>
                    {
                        emit_go_type(out, &format!("{nested}Item"), item)?;
                    }
                    _ => {}
                }
            }
            out.push_str(&format!("type {name} struct {{\n"));
            for field in fields {
                let nested = format!("{name}{}", pascal(&field.wire));
                let mut ty = go_type(&field.schema, &nested)?;
                if !field.required
                    && !ty.starts_with('*')
                    && !ty.starts_with("[]")
                    && !ty.starts_with("map[")
                {
                    ty = format!("*{ty}");
                }
                out.push_str(&format!(
                    "\t{} {} `json:{:?}`\n",
                    pascal(&field.wire),
                    ty,
                    field.wire
                ));
            }
            out.push_str("}\n");
        }
        SchemaKind::StringEnum(values) => {
            out.push_str(&format!("type {name} string\nconst (\n"));
            for value in values {
                out.push_str(&format!("\t{name}{} {name} = {:?}\n", pascal(value), value));
            }
            out.push_str(")\n");
        }
        _ => out.push_str(&format!("type {name} {}\n", go_type(schema, name)?)),
    }
    Ok(())
}

fn go_type(schema: &Schema, hint: &str) -> Result<String, String> {
    let base = match &schema.kind {
        SchemaKind::String => "string".to_owned(),
        SchemaKind::Integer => "int64".to_owned(),
        SchemaKind::Number => "float64".to_owned(),
        SchemaKind::Boolean => "bool".to_owned(),
        SchemaKind::StringEnum(_) | SchemaKind::Object(_) => hint.to_owned(),
        SchemaKind::Array(item) => format!("[]{}", go_type(item, &format!("{hint}Item"))?),
        SchemaKind::Map(value) => {
            format!("map[string]{}", go_type(value, &format!("{hint}Value"))?)
        }
    };
    Ok(if schema.nullable && !base.starts_with('*') {
        format!("*{base}")
    } else {
        base
    })
}

fn emit_dart(operations: &[Operation<'_>]) -> Result<String, String> {
    let mut out =
        String::from("\n// Typed operation facades from handlers-authoritative normalized IR.\n");
    for operation in operations {
        emit_dart_section_types(&mut out, operation)?;
    }
    out.push_str("class TypedRpcClient {\n  TypedRpcClient(this.transport);\n  final OresRpcClient transport;\n");
    for operation in operations {
        out.push_str(&format!(
            "  Future<{}Response> {}({}Input input) async {{\n    final raw = await transport.call({:?}, path: input.pathJson, query: input.queryJson, headers: input.headersJson, body: input.bodyJson, traceId: input.traceId, spanId: input.spanId);\n    return {}Response.fromJson((raw as Map).cast<String, Object?>());\n  }}\n",
            operation.pascal, operation.camel, operation.pascal, operation.contract.operation_key, operation.pascal
        ));
    }
    out.push_str("}\n");
    Ok(out)
}

fn emit_dart_section_types(out: &mut String, operation: &Operation<'_>) -> Result<(), String> {
    let mut fields = Vec::new();
    let mut ctor = Vec::new();
    for (suffix, field, schema) in request_sections(operation) {
        let name = format!("{}{}", operation.pascal, suffix);
        emit_dart_type(out, &name, schema)?;
        fields.push(format!("  final {name} {field};"));
        ctor.push(format!("required this.{field}"));
    }
    fields.push("  final String? traceId;".to_owned());
    fields.push("  final String? spanId;".to_owned());
    ctor.push("this.traceId".to_owned());
    ctor.push("this.spanId".to_owned());
    out.push_str(&format!(
        "class {}Input {{\n  const {}Input({{{}}});\n{}\n",
        operation.pascal,
        operation.pascal,
        ctor.join(", "),
        fields.join("\n")
    ));
    for (_, field, _) in request_sections(operation) {
        out.push_str(&format!(
            "  Map<String, Object?> get {field}Json => {field}.toJson();\n"
        ));
    }
    for field in ["path", "query", "headers"] {
        if !request_sections(operation)
            .iter()
            .any(|(_, candidate, _)| *candidate == field)
        {
            out.push_str(&format!(
                "  Map<String, Object?>? get {field}Json => null;\n"
            ));
        }
    }
    if request_sections(operation)
        .iter()
        .any(|(_, field, _)| *field == "body")
    {
        out.push_str("  Object? get bodyJson => body.toJson();\n");
    } else {
        out.push_str("  Object? get bodyJson => null;\n");
    }
    out.push_str("}\n");
    let response = operation.response.response.as_ref().ok_or_else(|| {
        format!(
            "{}: normalized IR is missing response.body schema",
            operation.contract.operation_key
        )
    })?;
    emit_dart_type(out, &format!("{}Response", operation.pascal), response)?;
    if let Some(error) = operation.response.error.as_ref() {
        emit_dart_type(out, &format!("{}Error", operation.pascal), error)?;
    }
    Ok(())
}

fn emit_dart_type(out: &mut String, name: &str, schema: &Schema) -> Result<(), String> {
    match &schema.kind {
        SchemaKind::Object(fields) => {
            for field in fields {
                let nested = format!("{name}{}", pascal(&field.wire));
                match &field.schema.kind {
                    SchemaKind::Object(_) | SchemaKind::StringEnum(_) => {
                        emit_dart_type(out, &nested, &field.schema)?
                    }
                    SchemaKind::Array(item)
                        if matches!(
                            item.kind,
                            SchemaKind::Object(_) | SchemaKind::StringEnum(_)
                        ) =>
                    {
                        emit_dart_type(out, &format!("{nested}Item"), item)?
                    }
                    _ => {}
                }
            }
            out.push_str(&format!("class {name} {{\n  const {name}({{"));
            for field in fields {
                if field.required {
                    out.push_str("required ");
                }
                out.push_str(&format!("this.{},", dart_ident(&field.wire)));
            }
            out.push_str("});\n");
            for field in fields {
                let hint = format!("{name}{}", pascal(&field.wire));
                let mut ty = dart_type(&field.schema, &hint)?;
                if !field.required && !ty.ends_with('?') {
                    ty.push('?');
                }
                out.push_str(&format!("  final {ty} {};\n", dart_ident(&field.wire)));
            }
            out.push_str(&format!(
                "  factory {name}.fromJson(Map<String, Object?> json) => {name}(\n"
            ));
            for field in fields {
                let hint = format!("{name}{}", pascal(&field.wire));
                let expr = dart_decode(&field.schema, &format!("json[{:?}]", field.wire), &hint)?;
                out.push_str(&format!("    {}: {expr},\n", dart_ident(&field.wire)));
            }
            out.push_str("  );\n  Map<String, Object?> toJson() => {\n");
            for field in fields {
                let hint = format!("{name}{}", pascal(&field.wire));
                let expr = dart_encode(
                    &field.schema,
                    &format!("this.{}", dart_ident(&field.wire)),
                    &hint,
                )?;
                out.push_str(&format!("    {:?}: {expr},\n", field.wire));
            }
            out.push_str("  };\n}\n");
        }
        SchemaKind::StringEnum(values) => {
            out.push_str(&format!("enum {name} {{\n"));
            for value in values {
                out.push_str(&format!("  {},\n", dart_ident(value)));
            }
            out.push_str("}\n");
            out.push_str(&format!(
                "{name} {name}FromWire(String value) {{\n  switch (value) {{\n"
            ));
            for value in values {
                out.push_str(&format!(
                    "    case {:?}: return {name}.{};\n",
                    value,
                    dart_ident(value)
                ));
            }
            out.push_str(
                "    default: throw FormatException('unknown enum value: $value');\n  }\n}\n",
            );
            out.push_str(&format!(
                "String {name}ToWire({name} value) => switch (value) {{\n"
            ));
            for value in values {
                out.push_str(&format!("  {name}.{} => {:?},\n", dart_ident(value), value));
            }
            out.push_str("};\n");
        }
        _ => out.push_str(&format!("typedef {name} = {};\n", dart_type(schema, name)?)),
    }
    Ok(())
}

fn dart_type(schema: &Schema, hint: &str) -> Result<String, String> {
    let base = match &schema.kind {
        SchemaKind::String => "String".to_owned(),
        SchemaKind::Integer => "int".to_owned(),
        SchemaKind::Number => "double".to_owned(),
        SchemaKind::Boolean => "bool".to_owned(),
        SchemaKind::StringEnum(_) | SchemaKind::Object(_) => hint.to_owned(),
        SchemaKind::Array(item) => format!("List<{}>", dart_type(item, &format!("{hint}Item"))?),
        SchemaKind::Map(value) => format!(
            "Map<String, {}>",
            dart_type(value, &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("{base}?")
    } else {
        base
    })
}

fn dart_decode(schema: &Schema, value: &str, hint: &str) -> Result<String, String> {
    let non_null = match &schema.kind {
        SchemaKind::String => format!("{value} as String"),
        SchemaKind::Integer => format!("({value} as num).toInt()"),
        SchemaKind::Number => format!("({value} as num).toDouble()"),
        SchemaKind::Boolean => format!("{value} as bool"),
        SchemaKind::StringEnum(_) => format!("{hint}FromWire({value} as String)"),
        SchemaKind::Object(_) => {
            format!("{hint}.fromJson(({value} as Map).cast<String, Object?>())")
        }
        SchemaKind::Array(item) => format!(
            "({value} as List).map((item) => {}).toList()",
            dart_decode(item, "item", &format!("{hint}Item"))?
        ),
        SchemaKind::Map(item) => format!(
            "({value} as Map).map((key, item) => MapEntry(key as String, {}))",
            dart_decode(item, "item", &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("{value} == null ? null : {non_null}")
    } else {
        non_null
    })
}

fn dart_encode(schema: &Schema, value: &str, hint: &str) -> Result<String, String> {
    let non_null = match &schema.kind {
        SchemaKind::String | SchemaKind::Integer | SchemaKind::Number | SchemaKind::Boolean => {
            value.to_owned()
        }
        SchemaKind::StringEnum(_) => format!("{hint}ToWire({value})"),
        SchemaKind::Object(_) => format!("{value}.toJson()"),
        SchemaKind::Array(item) => format!(
            "{value}.map((item) => {}).toList()",
            dart_encode(item, "item", &format!("{hint}Item"))?
        ),
        SchemaKind::Map(item) => format!(
            "{value}.map((key, item) => MapEntry(key, {}))",
            dart_encode(item, "item", &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("{value} == null ? null : {non_null}")
    } else {
        non_null
    })
}

fn emit_rust(
    map: &RouteMap,
    operations: &[Operation<'_>],
    audience: &str,
    digest: &str,
) -> Result<String, String> {
    let mut out = format!(
        "\n// Typed canonical POST /v1/rpc facade from handlers-authoritative normalized IR.\n\
         pub const TYPED_RPC_CONTRACT_SHA256: &str = {digest:?};\n\
         pub const TYPED_RPC_CLIENT_AUDIENCE: &str = {audience:?};\n\
         pub const TYPED_RPC_SERVICE: &str = {service:?};\n\
         pub const TYPED_RPC_HTTP_PATH: &str = \"/v1/rpc\";\n\n\
         pub struct TypedRpcClient {{\n    transport: ::ores_rpc_calls_http_tcp_pool::HttpRpcClient,\n    sequence: ::std::sync::atomic::AtomicU64,\n}}\n\n\
         impl TypedRpcClient {{\n    pub fn new(transport: ::ores_rpc_calls_http_tcp_pool::HttpRpcClient) -> Self {{\n        Self {{ transport, sequence: ::std::sync::atomic::AtomicU64::new(0) }}\n    }}\n",
        service = map.service,
    );
    for operation in operations {
        emit_rust_section_types(&mut out, operation)?;
        out.push_str(&emit_rust_method(operation)?);
    }
    out.push_str("}\n");
    Ok(out)
}

fn emit_rust_section_types(out: &mut String, operation: &Operation<'_>) -> Result<(), String> {
    let mut fields = Vec::new();
    for (suffix, field, schema) in request_sections(operation) {
        let name = format!("{}{}", operation.pascal, suffix);
        emit_rust_type(out, &name, schema)?;
        fields.push(format!("    pub {field}: {name},"));
    }
    fields.push("    pub trace_id: Option<String>,".to_owned());
    fields.push("    pub span_id: Option<String>,".to_owned());
    out.push_str(&format!(
        "#[derive(Clone, Debug, serde::Serialize)]\npub struct {}Input {{\n{}\n}}\n",
        operation.pascal,
        fields.join("\n")
    ));
    let response = operation.response.response.as_ref().ok_or_else(|| {
        format!(
            "{}: normalized IR is missing response.body schema",
            operation.contract.operation_key
        )
    })?;
    emit_rust_type(out, &format!("{}Response", operation.pascal), response)?;
    if let Some(error) = operation.response.error.as_ref() {
        emit_rust_type(out, &format!("{}Error", operation.pascal), error)?;
    }
    Ok(())
}

fn emit_rust_type(out: &mut String, name: &str, schema: &Schema) -> Result<(), String> {
    match &schema.kind {
        SchemaKind::Object(fields) => {
            for field in fields {
                let nested = format!("{name}{}", pascal(&field.wire));
                match &field.schema.kind {
                    SchemaKind::Object(_) | SchemaKind::StringEnum(_) => {
                        emit_rust_type(out, &nested, &field.schema)?
                    }
                    SchemaKind::Array(item)
                        if matches!(
                            item.kind,
                            SchemaKind::Object(_) | SchemaKind::StringEnum(_)
                        ) =>
                    {
                        emit_rust_type(out, &format!("{nested}Item"), item)?
                    }
                    _ => {}
                }
            }
            out.push_str(&format!("#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]\npub struct {name} {{\n"));
            for field in fields {
                let ident = rust_ident(&field.wire);
                let hint = format!("{name}{}", pascal(&field.wire));
                let mut ty = rust_type(&field.schema, &hint)?;
                if !field.required && !ty.starts_with("Option<") {
                    ty = format!("Option<{ty}>");
                }
                if ident != field.wire {
                    out.push_str(&format!("    #[serde(rename = {:?})]\n", field.wire));
                }
                if !field.required {
                    out.push_str(
                        "    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n",
                    );
                }
                out.push_str(&format!("    pub {ident}: {ty},\n"));
            }
            out.push_str("}\n");
        }
        SchemaKind::StringEnum(values) => {
            out.push_str(&format!("#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]\npub enum {name} {{\n"));
            for value in values {
                out.push_str(&format!(
                    "    #[serde(rename = {:?})]\n    {},\n",
                    value,
                    pascal(value)
                ));
            }
            out.push_str("}\n");
        }
        _ => out.push_str(&format!(
            "pub type {name} = {};\n",
            rust_type(schema, name)?
        )),
    }
    Ok(())
}

fn rust_type(schema: &Schema, hint: &str) -> Result<String, String> {
    let base = match &schema.kind {
        SchemaKind::String => "String".to_owned(),
        SchemaKind::Integer => "i64".to_owned(),
        SchemaKind::Number => "f64".to_owned(),
        SchemaKind::Boolean => "bool".to_owned(),
        SchemaKind::StringEnum(_) | SchemaKind::Object(_) => hint.to_owned(),
        SchemaKind::Array(item) => format!("Vec<{}>", rust_type(item, &format!("{hint}Item"))?),
        SchemaKind::Map(value) => format!(
            "::std::collections::BTreeMap<String, {}>",
            rust_type(value, &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("Option<{base}>")
    } else {
        base
    })
}

fn emit_rust_method(operation: &Operation<'_>) -> Result<String, String> {
    let response = format!("{}Response", operation.pascal);
    let error = if operation.response.error.is_some() {
        format!("{}Error", operation.pascal)
    } else {
        "::serde_json::Value".to_owned()
    };
    let mut sections = String::new();
    for (_, field, _) in request_sections(operation) {
        sections.push_str(&format!(
            "        envelope[{field:?}] = ::serde_json::to_value(&input.{field})?;\n"
        ));
    }
    Ok(format!(
        "    pub async fn {name}(&self, input: {pascal}Input) -> Result<{response}, {pascal}RpcError> {{\n\
             let id = format!(\"rust-{{}}\", self.sequence.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed));\n\
             let mut envelope = ::serde_json::json!({{\"v\":1,\"op\":\"call\",\"id\":id,\"key\":{key:?},\"transport\":\"http\"}});\n\
{sections}\
             if let Some(value) = &input.trace_id {{ envelope[\"traceId\"] = ::serde_json::Value::String(value.clone()); }}\n\
             if let Some(value) = &input.span_id {{ envelope[\"spanId\"] = ::serde_json::Value::String(value.clone()); }}\n\
             let request = ::ores_rpc_calls_http_tcp_pool::PlainHttpRequest::new(TYPED_RPC_SERVICE, {key:?}, ::ores_rpc_calls_http_tcp_pool::HttpMethod::Post, TYPED_RPC_HTTP_PATH).with_json_body(envelope);\n\
             let response = self.transport.send_plain(&request).await.map_err({pascal}RpcError::Transport)?;\n\
             let receipt: TypedRpcReceipt<{response}, {error}> = ::serde_json::from_slice(&response.body).map_err({pascal}RpcError::Decode)?;\n\
             if receipt.id != id || receipt.key != {key:?} {{ return Err({pascal}RpcError::Protocol(\"RPC receipt correlation mismatch\".to_owned())); }}\n\
             if !receipt.ok {{ return Err({pascal}RpcError::Remote(receipt.error)); }}\n\
             receipt.body.ok_or_else(|| {pascal}RpcError::Protocol(\"RPC receipt omitted success body\".to_owned()))\n\
         }}\n",
        name = operation.rust_fn,
        pascal = operation.pascal,
        key = operation.contract.operation_key,
        sections = sections,
        response = response,
        error = error,
    ) + &format!(
        "}}\n#[derive(Debug, serde::Deserialize)]\nstruct TypedRpcReceipt<B, E> {{ id: String, key: String, ok: bool, #[serde(default)] body: Option<B>, #[serde(default)] error: Option<E> }}\n#[derive(Debug)]\npub enum {p}RpcError {{ Transport(::ores_rpc_calls_http_tcp_pool::RpcError), Decode(::serde_json::Error), Protocol(String), Remote(Option<{e}>) }}\nimpl From<::serde_json::Error> for {p}RpcError {{ fn from(value: ::serde_json::Error) -> Self {{ Self::Decode(value) }} }}\nimpl TypedRpcClient {{\n",
        p = operation.pascal,
        e = error,
    ))
}

fn emit_gleam(operations: &[Operation<'_>]) -> Result<String, String> {
    let mut out = String::from("\n// Typed operation facades from handlers-authoritative normalized IR.\nimport gleam/dynamic/decode\nimport gleam/option\nimport gleam/string\n");
    for operation in operations {
        emit_gleam_section_types(&mut out, operation)?;
        let response = format!("{}Response", operation.pascal);
        let sections = request_sections(operation);
        let json_field = |field: &str| {
            if sections.iter().any(|(_, candidate, _)| *candidate == field) {
                format!("input.{field}_json")
            } else {
                "option.None".to_owned()
            }
        };
        let path_json = json_field("path");
        let query_json = json_field("query");
        let headers_json = json_field("headers");
        let body_json = json_field("body");
        out.push_str(&format!(
            "pub fn {}(transport: Transport, base_url: String, id: String, input: {}Input) -> Result({}, String) {{\n  let args = CallArgs({path_json}, {query_json}, {headers_json}, {body_json}, input.trace_id, input.span_id)\n  use raw <- result.try(call(transport, base_url, id, {:?}, args))\n  case decode.run(raw, {}_response_decoder()) {{ Ok(value) -> Ok(value) Error(errors) -> Error(string.inspect(errors)) }}\n}}\n",
            operation.rust_fn, operation.pascal, response, operation.contract.operation_key, snake(&operation.pascal)
        ));
    }
    Ok(out)
}

fn emit_gleam_section_types(out: &mut String, operation: &Operation<'_>) -> Result<(), String> {
    for (suffix, _, schema) in request_sections(operation) {
        emit_gleam_type(out, &format!("{}{}", operation.pascal, suffix), schema)?;
    }
    let response = operation.response.response.as_ref().ok_or_else(|| {
        format!(
            "{}: normalized IR is missing response.body schema",
            operation.contract.operation_key
        )
    })?;
    emit_gleam_type(out, &format!("{}Response", operation.pascal), response)?;
    if let Some(error) = operation.response.error.as_ref() {
        emit_gleam_type(out, &format!("{}Error", operation.pascal), error)?;
    }
    // JSON projections on Input deliberately use json.Json, but callers populate
    // the strongly typed section fields. The conversion functions are generated
    // from those section schemas above.
    let mut typed = Vec::new();
    let mut json = Vec::new();
    for (suffix, field, _) in request_sections(operation) {
        typed.push(format!("    {field}: {}{},", operation.pascal, suffix));
        json.push(format!("    {field}_json: List(#(String, json.Json)),"));
    }
    out.push_str(&format!("pub type {}Input {{\n  {}Input(\n{}\n{}\n    trace_id: option.Option(String),\n    span_id: option.Option(String),\n  )\n}}\n", operation.pascal, operation.pascal, typed.join("\n"), json.join("\n")));
    Ok(())
}

fn emit_gleam_type(out: &mut String, name: &str, schema: &Schema) -> Result<(), String> {
    match &schema.kind {
        SchemaKind::Object(fields) => {
            for field in fields {
                let nested = format!("{name}{}", pascal(&field.wire));
                match &field.schema.kind {
                    SchemaKind::Object(_) | SchemaKind::StringEnum(_) => {
                        emit_gleam_type(out, &nested, &field.schema)?
                    }
                    SchemaKind::Array(item)
                        if matches!(
                            item.kind,
                            SchemaKind::Object(_) | SchemaKind::StringEnum(_)
                        ) =>
                    {
                        emit_gleam_type(out, &format!("{nested}Item"), item)?
                    }
                    _ => {}
                }
            }
            out.push_str(&format!("pub type {name} {{\n  {name}(\n"));
            for field in fields {
                let hint = format!("{name}{}", pascal(&field.wire));
                let mut ty = gleam_type(&field.schema, &hint)?;
                if !field.required && !ty.starts_with("option.Option(") {
                    ty = format!("option.Option({ty})");
                }
                out.push_str(&format!("    {}: {ty},\n", gleam_ident(&field.wire)));
            }
            out.push_str("  )\n}\n");
            out.push_str(&format!(
                "pub fn {}_decoder() -> decode.Decoder({name}) {{\n",
                snake(name)
            ));
            for field in fields {
                let hint = format!("{name}{}", pascal(&field.wire));
                let decoder = gleam_decoder(&field.schema, &hint)?;
                if field.required {
                    out.push_str(&format!(
                        "  use {} <- decode.field({:?}, {decoder})\n",
                        gleam_ident(&field.wire),
                        field.wire
                    ));
                } else {
                    out.push_str(&format!("  use {} <- decode.optional_field({:?}, option.None, decode.optional({decoder}))\n", gleam_ident(&field.wire), field.wire));
                }
            }
            out.push_str(&format!(
                "  decode.success({name}({}))\n}}\n",
                fields
                    .iter()
                    .map(|field| gleam_ident(&field.wire))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        SchemaKind::StringEnum(values) => {
            out.push_str(&format!("pub type {name} {{\n"));
            for value in values {
                out.push_str(&format!("  {}\n", pascal(value)));
            }
            out.push_str("}\n");
            out.push_str(&format!("pub fn {}_decoder() -> decode.Decoder({name}) {{\n  use wire <- decode.then(decode.string)\n  case wire {{\n", snake(name)));
            for value in values {
                out.push_str(&format!(
                    "    {:?} -> decode.success({})\n",
                    value,
                    pascal(value)
                ));
            }
            out.push_str(&format!(
                "    _ -> decode.failure({}, {:?})\n  }}\n}}\n",
                pascal(&values[0]),
                name
            ));
        }
        _ => out.push_str(&format!(
            "pub type {name} = {}\n",
            gleam_type(schema, name)?
        )),
    }
    Ok(())
}

fn gleam_type(schema: &Schema, hint: &str) -> Result<String, String> {
    let base = match &schema.kind {
        SchemaKind::String => "String".to_owned(),
        SchemaKind::Integer => "Int".to_owned(),
        SchemaKind::Number => "Float".to_owned(),
        SchemaKind::Boolean => "Bool".to_owned(),
        SchemaKind::StringEnum(_) | SchemaKind::Object(_) => hint.to_owned(),
        SchemaKind::Array(item) => format!("List({})", gleam_type(item, &format!("{hint}Item"))?),
        SchemaKind::Map(value) => format!(
            "dict.Dict(String, {})",
            gleam_type(value, &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("option.Option({base})")
    } else {
        base
    })
}

fn gleam_decoder(schema: &Schema, hint: &str) -> Result<String, String> {
    let base = match &schema.kind {
        SchemaKind::String => "decode.string".to_owned(),
        SchemaKind::Integer => "decode.int".to_owned(),
        SchemaKind::Number => "decode.float".to_owned(),
        SchemaKind::Boolean => "decode.bool".to_owned(),
        SchemaKind::StringEnum(_) | SchemaKind::Object(_) => format!("{}_decoder()", snake(hint)),
        SchemaKind::Array(item) => format!(
            "decode.list({})",
            gleam_decoder(item, &format!("{hint}Item"))?
        ),
        SchemaKind::Map(value) => format!(
            "decode.dict(decode.string, {})",
            gleam_decoder(value, &format!("{hint}Value"))?
        ),
    };
    Ok(if schema.nullable {
        format!("decode.optional({base})")
    } else {
        base
    })
}

fn request_sections<'a>(
    operation: &'a Operation<'_>,
) -> Vec<(&'static str, &'static str, &'a Schema)> {
    let mut out = Vec::new();
    if let Some(schema) = operation.request.path.as_ref() {
        out.push(("Path", "path", schema));
    }
    if let Some(schema) = operation.request.query.as_ref() {
        out.push(("Query", "query", schema));
    }
    if let Some(schema) = operation.request.headers.as_ref() {
        out.push(("Headers", "headers", schema));
    }
    if let Some(schema) = operation.request.body.as_ref() {
        out.push(("Body", "body", schema));
    }
    out
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn pascal(value: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper {
                out.extend(ch.to_uppercase());
                upper = false;
            } else {
                out.push(ch);
            }
        } else {
            upper = true;
        }
    }
    out
}

fn camel(value: &str) -> String {
    let pascal = pascal(value);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn snake(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn rust_ident(value: &str) -> String {
    let mut out = snake(value);
    if [
        "type", "match", "ref", "self", "crate", "super", "async", "await", "move", "loop", "in",
        "where", "use", "mod", "struct", "enum", "fn", "pub", "impl", "trait",
    ]
    .contains(&out.as_str())
    {
        out.push('_');
    }
    out
}

fn dart_ident(value: &str) -> String {
    let mut out = camel(value);
    if out.is_empty() {
        out = "field".to_owned();
    }
    out
}

fn gleam_ident(value: &str) -> String {
    snake(value)
}
