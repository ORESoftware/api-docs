use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::common::{collect_files, read_json, read_text, relative_path, write_json, CheckResult};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Field {
    pub name: String,
    pub required: bool,
    pub kind: String,
    pub const_value: Option<Value>,
    pub enum_values: Option<Vec<String>>,
    pub min_length: Option<i64>,
    pub max_length: Option<i64>,
    pub pattern: Option<String>,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub proto_number: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub name: String,
    pub source: String,
    pub fields: BTreeMap<String, Field>,
    pub enum_values: Option<Vec<String>>,
    pub union_members: Option<Vec<String>>,
}

fn strip_comment(line: &str) -> &str {
    line.split_once("//").map_or(line, |(before, _)| before).trim()
}

fn parse_decorator(line: &str) -> Option<(String, Option<String>)> {
    let value = line.strip_prefix('@')?.trim();
    if let Some(open) = value.find('(') {
        let close = value.rfind(')')?;
        if close < open {
            return None;
        }
        Some((
            value[..open].trim().to_owned(),
            Some(value[open + 1..close].trim().to_owned()),
        ))
    } else {
        Some((value.to_owned(), None))
    }
}

#[derive(Default)]
struct Decorators {
    min_length: Option<i64>,
    max_length: Option<i64>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    pattern: Option<String>,
}

fn decorators(values: &[(String, Option<String>)]) -> Decorators {
    let mut out = Decorators::default();
    for (name, argument) in values {
        match name.as_str() {
            "minLength" => out.min_length = argument.as_deref().and_then(|value| value.parse().ok()),
            "maxLength" => out.max_length = argument.as_deref().and_then(|value| value.parse().ok()),
            "minValue" => out.minimum = argument.as_deref().and_then(|value| value.parse().ok()),
            "maxValue" => out.maximum = argument.as_deref().and_then(|value| value.parse().ok()),
            "pattern" => {
                out.pattern = argument
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<String>(value).ok())
            }
            _ => {}
        }
    }
    out
}

fn parse_typespec_field(name: &str, required: bool, type_source: &str, deco: Decorators) -> Field {
    let source = type_source.trim().trim_end_matches(',').trim();
    let (kind, const_value, enum_values) = if source.starts_with('"') && source.contains('|') {
        let values = source
            .split('|')
            .filter_map(|part| serde_json::from_str::<String>(part.trim()).ok())
            .collect::<Vec<_>>();
        ("enum", None, Some(values))
    } else if source.starts_with('"') && source.ends_with('"') {
        (
            "const",
            serde_json::from_str::<Value>(source).ok(),
            None,
        )
    } else if let Ok(number) = source.parse::<i64>() {
        ("const", Some(json!(number)), None)
    } else {
        let kind = match source {
            "string" => "string",
            "int32" | "int64" | "uint32" | "integer" => "integer",
            "boolean" => "boolean",
            "unknown" => "any",
            value if value.starts_with("Record<") => "object",
            value if value.ends_with("[]") => "array",
            _ => "ref",
        };
        (kind, None, None)
    };
    Field {
        name: name.to_owned(),
        required,
        kind: kind.to_owned(),
        const_value,
        enum_values,
        min_length: deco.min_length,
        max_length: deco.max_length,
        pattern: deco.pattern,
        minimum: deco.minimum,
        maximum: deco.maximum,
        proto_number: None,
    }
}

fn declaration(line: &str) -> Option<(&'static str, String)> {
    for (kind, prefix) in [("model", "model "), ("enum", "enum "), ("union", "union ")] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let name = rest.split_whitespace().next()?.trim_end_matches('{');
            if line.contains('{') && !name.is_empty() {
                return Some((kind, name.to_owned()));
            }
        }
    }
    None
}

pub fn parse_typespec(text: &str, source: &str) -> BTreeMap<String, Shape> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut shapes = BTreeMap::new();
    let mut namespace = String::new();
    let mut index = 0;
    let mut pending: Vec<(String, Option<String>)> = Vec::new();
    while index < lines.len() {
        let line = strip_comment(lines[index]);
        index += 1;
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line
            .strip_prefix("namespace ")
            .and_then(|rest| rest.strip_suffix(';'))
        {
            namespace = value.trim().to_owned();
            continue;
        }
        if let Some(decorator) = parse_decorator(line) {
            pending.push(decorator);
            continue;
        }
        let Some((kind, local_name)) = declaration(line) else {
            pending.clear();
            continue;
        };
        let qualified = if namespace.is_empty() {
            local_name
        } else {
            format!("{namespace}.{local_name}")
        };
        let mut body = Vec::new();
        let mut depth = line.matches('{').count() as i64 - line.matches('}').count() as i64;
        while depth > 0 && index < lines.len() {
            let next = lines[index];
            index += 1;
            depth += next.matches('{').count() as i64 - next.matches('}').count() as i64;
            if depth > 0 {
                body.push(next);
            }
        }
        let mut shape = Shape {
            name: qualified.clone(),
            source: source.to_owned(),
            ..Shape::default()
        };
        match kind {
            "enum" => {
                let values = body
                    .iter()
                    .map(|line| strip_comment(line).trim_end_matches(',').trim())
                    .filter(|line| !line.is_empty())
                    .map(|line| line.split_once(':').map_or(line, |(name, _)| name).trim().to_owned())
                    .collect::<Vec<_>>();
                shape.enum_values = Some(values);
            }
            "union" => {
                let values = body
                    .iter()
                    .map(|line| strip_comment(line).trim_end_matches(',').trim())
                    .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim().to_owned()))
                    .collect::<Vec<_>>();
                shape.union_members = Some(values);
            }
            _ => {
                let mut inner_pending = Vec::new();
                for raw in body {
                    let line = strip_comment(raw);
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(decorator) = parse_decorator(line) {
                        inner_pending.push(decorator);
                        continue;
                    }
                    let Some(property) = line.strip_suffix(';') else {
                        inner_pending.clear();
                        continue;
                    };
                    let Some((raw_name, raw_type)) = property.split_once(':') else {
                        inner_pending.clear();
                        continue;
                    };
                    let raw_name = raw_name.trim();
                    let (raw_name, required) = raw_name
                        .strip_suffix('?')
                        .map_or((raw_name, true), |name| (name, false));
                    let name = raw_name.trim_matches('`');
                    let deco = decorators(&inner_pending);
                    inner_pending.clear();
                    shape.fields.insert(
                        name.to_owned(),
                        parse_typespec_field(name, required, raw_type, deco),
                    );
                }
            }
        }
        shapes.insert(qualified, shape);
        pending.clear();
    }
    shapes
}

fn json_schema_field(name: &str, spec: &Value, required: bool) -> Field {
    if spec == &Value::Bool(true) {
        return Field {
            name: name.to_owned(),
            required,
            kind: "any".to_owned(),
            ..Field::default()
        };
    }
    let Some(object) = spec.as_object() else {
        return Field {
            name: name.to_owned(),
            required,
            kind: "unknown".to_owned(),
            ..Field::default()
        };
    };
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if object.contains_key("const") {
                "const".to_owned()
            } else if object.contains_key("enum") {
                "enum".to_owned()
            } else {
                "unknown".to_owned()
            }
        });
    let enum_values = object.get("enum").and_then(Value::as_array).map(|items| {
        items
            .iter()
            .map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))
            .collect::<Vec<_>>()
    });
    Field {
        name: name.to_owned(),
        required,
        kind,
        const_value: object.get("const").cloned(),
        enum_values,
        min_length: object.get("minLength").and_then(Value::as_i64),
        max_length: object.get("maxLength").and_then(Value::as_i64),
        pattern: object.get("pattern").and_then(Value::as_str).map(str::to_owned),
        minimum: object.get("minimum").and_then(Value::as_i64),
        maximum: object.get("maximum").and_then(Value::as_i64),
        proto_number: None,
    }
}

pub fn parse_json_schema(document: &Value, name: &str, source: &str) -> Shape {
    let required = document
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let mut shape = Shape {
        name: name.to_owned(),
        source: source.to_owned(),
        ..Shape::default()
    };
    if let Some(properties) = document.get("properties").and_then(Value::as_object) {
        for (field_name, spec) in properties {
            shape.fields.insert(
                field_name.clone(),
                json_schema_field(field_name, spec, required.contains(field_name.as_str())),
            );
        }
    }
    if let Some(blocks) = document.get("allOf").and_then(Value::as_array) {
        for block in blocks {
            let then = block.get("then").unwrap_or(&Value::Null);
            let then_required = then
                .get("required")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).collect::<BTreeSet<_>>())
                .unwrap_or_default();
            if let Some(properties) = then.get("properties").and_then(Value::as_object) {
                for (field_name, spec) in properties {
                    shape.fields.entry(field_name.clone()).or_insert_with(|| {
                        json_schema_field(field_name, spec, then_required.contains(field_name.as_str()))
                    });
                }
            }
        }
    }
    shape
}

fn valid_proto_type(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_alphanumeric() || character == '_' || character == '.')
}

fn json_name_from_options(options: &str) -> Option<String> {
    let marker = "json_name";
    let start = options.find(marker)? + marker.len();
    let rest = options[start..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quoted = rest.strip_prefix('"')?;
    let end = quoted.find('"')?;
    Some(quoted[..end].to_owned())
}

fn parse_proto_field(line: &str) -> Option<(String, String, i64, bool)> {
    let line = line.strip_suffix(';')?.trim();
    let (left, right) = line.split_once('=')?;
    let mut tokens = left.split_whitespace().collect::<Vec<_>>();
    let modifier = if matches!(tokens.first().copied(), Some("optional") | Some("repeated")) {
        Some(tokens.remove(0))
    } else {
        None
    };
    if tokens.len() != 2 || !valid_proto_type(tokens[0]) {
        return None;
    }
    let proto_type = tokens[0].to_owned();
    let raw_name = tokens[1].to_owned();
    let mut right = right.trim();
    let options = if let Some(open) = right.find('[') {
        let close = right.rfind(']')?;
        let options = right[open + 1..close].to_owned();
        right = right[..open].trim();
        options
    } else {
        String::new()
    };
    let number = right.parse::<i64>().ok()?;
    let name = json_name_from_options(&options).unwrap_or(raw_name);
    Some((name, proto_type, number, modifier == Some("optional")))
}

pub fn parse_proto(text: &str, source: &str) -> BTreeMap<String, Shape> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut shapes = BTreeMap::new();
    let mut package = String::new();
    let mut index = 0;
    while index < lines.len() {
        let line = strip_comment(lines[index]);
        index += 1;
        if let Some(value) = line
            .strip_prefix("package ")
            .and_then(|rest| rest.strip_suffix(';'))
        {
            package = value.trim().to_owned();
            continue;
        }
        let (kind, local_name) = if let Some(rest) = line.strip_prefix("message ") {
            ("message", rest.split_whitespace().next().unwrap_or_default().trim_end_matches('{'))
        } else if let Some(rest) = line.strip_prefix("enum ") {
            ("enum", rest.split_whitespace().next().unwrap_or_default().trim_end_matches('{'))
        } else {
            continue;
        };
        if !line.contains('{') || local_name.is_empty() {
            continue;
        }
        let qualified = if package.is_empty() {
            local_name.to_owned()
        } else {
            format!("{package}.{local_name}")
        };
        let mut body = Vec::new();
        let mut depth = line.matches('{').count() as i64 - line.matches('}').count() as i64;
        while depth > 0 && index < lines.len() {
            let next = lines[index];
            index += 1;
            depth += next.matches('{').count() as i64 - next.matches('}').count() as i64;
            if depth > 0 {
                body.push(next);
            }
        }
        let mut shape = Shape {
            name: qualified.clone(),
            source: source.to_owned(),
            ..Shape::default()
        };
        if kind == "enum" {
            let mut values = Vec::new();
            for raw in body {
                let line = strip_comment(raw);
                let Some((name, number)) = line.strip_suffix(';').and_then(|line| line.split_once('=')) else {
                    continue;
                };
                if number.trim().parse::<i64>().is_ok() {
                    values.push(name.trim().to_owned());
                }
            }
            shape.enum_values = Some(values);
        } else {
            for raw in body {
                let line = strip_comment(raw);
                let Some((name, proto_type, number, optional)) = parse_proto_field(line) else {
                    continue;
                };
                let kind = match proto_type.as_str() {
                    "string" => "string",
                    "bool" => "boolean",
                    "uint32" | "int32" => "integer",
                    "bytes" => "any",
                    _ => "ref",
                };
                shape.fields.insert(
                    name.clone(),
                    Field {
                        name,
                        required: !optional && proto_type != "bytes",
                        kind: kind.to_owned(),
                        proto_number: Some(number),
                        ..Field::default()
                    },
                );
            }
        }
        shapes.insert(qualified, shape);
    }
    shapes
}

pub fn load_all_typespec(root: &Path) -> CheckResult<BTreeMap<String, Shape>> {
    let directory = root.join("idl/typespec");
    let mut out = BTreeMap::new();
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("tsp"))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.file_name().and_then(|value| value.to_str()) == Some("main.tsp") {
            continue;
        }
        let source = relative_path(root, &path);
        out.extend(parse_typespec(&read_text(&path)?, &source));
    }
    Ok(out)
}

pub fn load_all_proto(root: &Path) -> CheckResult<BTreeMap<String, Shape>> {
    let directory = root.join("idl/protobuf");
    let mut out = BTreeMap::new();
    for path in collect_files(&directory, "proto")? {
        let source = relative_path(root, &path);
        out.extend(parse_proto(&read_text(&path)?, &source));
    }
    Ok(out)
}

pub fn load_json_shapes(root: &Path) -> CheckResult<BTreeMap<String, Shape>> {
    let mut out = BTreeMap::new();
    for stem in ["rpc-call", "rpc-receipt", "rpc-frame", "telemetry-attributes"] {
        let path = root.join(format!("json-schema/{stem}.schema.json"));
        let source = relative_path(root, &path);
        out.insert(stem.to_owned(), parse_json_schema(&read_json(&path)?, stem, &source));
    }
    Ok(out)
}

fn compare_names(left: &Shape, right: &Shape) -> Vec<String> {
    let left_names = left.fields.keys().cloned().collect::<BTreeSet<_>>();
    let right_names = right.fields.keys().cloned().collect::<BTreeSet<_>>();
    let mut diffs = Vec::new();
    let missing = left_names.difference(&right_names).cloned().collect::<Vec<_>>();
    let extra = right_names.difference(&left_names).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        diffs.push(format!(
            "{} missing fields present in {}: {missing:?}",
            right.name, left.name
        ));
    }
    if !extra.is_empty() {
        diffs.push(format!("{} has extra fields vs {}: {extra:?}", right.name, left.name));
    }
    diffs
}

fn compare_constraints(left: &Shape, right: &Shape, skip_required: bool) -> Vec<String> {
    let mut diffs = Vec::new();
    for name in left.fields.keys().filter(|name| right.fields.contains_key(*name)) {
        let a = &left.fields[name];
        let b = &right.fields[name];
        for (label, left_value, right_value) in [
            ("const", a.const_value.as_ref().map(Value::to_string), b.const_value.as_ref().map(Value::to_string)),
            ("min_length", a.min_length.map(|value| value.to_string()), b.min_length.map(|value| value.to_string())),
            ("max_length", a.max_length.map(|value| value.to_string()), b.max_length.map(|value| value.to_string())),
            ("pattern", a.pattern.clone(), b.pattern.clone()),
            ("minimum", a.minimum.map(|value| value.to_string()), b.minimum.map(|value| value.to_string())),
            ("maximum", a.maximum.map(|value| value.to_string()), b.maximum.map(|value| value.to_string())),
        ] {
            if left_value.is_some() && right_value.is_some() && left_value != right_value {
                diffs.push(format!(
                    "{}.{}.{label}={left_value:?} vs {}.{}.{label}={right_value:?}",
                    left.name, name, right.name, name
                ));
            }
        }
        if let (Some(left_enum), Some(right_enum)) = (&a.enum_values, &b.enum_values) {
            let mut left_enum = left_enum.clone();
            let mut right_enum = right_enum.clone();
            left_enum.sort();
            right_enum.sort();
            if left_enum != right_enum {
                diffs.push(format!(
                    "{}.{}.enum {:?} vs {}.{}.enum {:?}",
                    left.name, name, a.enum_values, right.name, name, b.enum_values
                ));
            }
        }
        if !skip_required && a.required != b.required {
            diffs.push(format!(
                "{}.{} required={} vs {}.{} required={}",
                left.name, name, a.required, right.name, name, b.required
            ));
        }
    }
    diffs
}

fn proto_enum_json_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|item| !item.ends_with("_UNSPECIFIED"))
        .map(|item| {
            let lowered = item.to_lowercase();
            lowered
                .strip_prefix("transport_")
                .unwrap_or(&lowered)
                .to_owned()
        })
        .collect()
}

pub fn check_protobuf_lock(proto: &BTreeMap<String, Shape>, lock: &Value) -> Vec<String> {
    let mut diffs = Vec::new();
    let Some(messages) = lock.get("messages").and_then(Value::as_object) else {
        return vec!["protobuf.lock.json: messages must be an object".to_owned()];
    };
    for (qualified, expected) in messages {
        let Some(shape) = proto.get(qualified) else {
            diffs.push(format!("protobuf.lock missing message in sources: {qualified}"));
            continue;
        };
        let got = shape
            .fields
            .values()
            .filter_map(|field| field.proto_number.map(|number| (field.name.clone(), json!(number))))
            .collect::<serde_json::Map<_, _>>();
        let want = expected.get("fields").cloned().unwrap_or_else(|| json!({}));
        if Value::Object(got.clone()) != want {
            diffs.push(format!("{qualified} field numbers {:?} != lock {want}", got));
        }
        let reserved = expected
            .get("reserved")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect::<BTreeSet<_>>();
        let actual_numbers = shape
            .fields
            .values()
            .filter_map(|field| field.proto_number)
            .collect::<BTreeSet<_>>();
        let overlap = reserved.intersection(&actual_numbers).copied().collect::<Vec<_>>();
        if !overlap.is_empty() {
            diffs.push(format!("{qualified} reuses reserved field numbers {overlap:?}"));
        }
    }
    diffs
}

fn shape<'a>(map: &'a BTreeMap<String, Shape>, name: &str, vetoes: &mut Vec<String>) -> Option<&'a Shape> {
    let value = map.get(name);
    if value.is_none() {
        vetoes.push(format!("missing shape {name}"));
    }
    value
}

pub fn cross_check(root: &Path) -> CheckResult<Value> {
    let schemas = load_json_shapes(root)?;
    let typespec = load_all_typespec(root)?;
    let proto = load_all_proto(root)?;
    let lock = read_json(&root.join("idl/protobuf.lock.json"))?;
    let expected = read_json(&root.join("idl/expected-deltas.json"))?;
    let expected_ids = expected
        .get("deltas")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|delta| delta.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut vetoes = Vec::new();
    let mut notes = Vec::new();

    for (schema_name, type_name, proto_name) in [
        ("rpc-call", "Ores.Rpc.V1.RpcCall", Some("ores.rpc.v1.RpcCall")),
        (
            "rpc-receipt",
            "Ores.Rpc.V1.RpcReceipt",
            Some("ores.rpc.v1.RpcReceipt"),
        ),
        (
            "telemetry-attributes",
            "Ores.Rpc.Telemetry.TelemetryAttributes",
            None,
        ),
    ] {
        let Some(schema) = shape(&schemas, schema_name, &mut vetoes) else {
            continue;
        };
        let Some(model) = shape(&typespec, type_name, &mut vetoes) else {
            continue;
        };
        vetoes.extend(compare_names(schema, model));
        vetoes.extend(compare_constraints(schema, model, false));
        if let Some(proto_name) = proto_name {
            if let Some(message) = shape(&proto, proto_name, &mut vetoes) {
                vetoes.extend(compare_names(schema, message));
                notes.push(format!("expected-delta proto-json-bytes applies to {}", message.name));
            }
        }
    }

    if let (Some(frame_schema), Some(frame_union)) = (
        shape(&schemas, "rpc-frame", &mut vetoes),
        shape(&typespec, "Ores.Rpc.V2.RpcFrame", &mut vetoes),
    ) {
        let want = BTreeSet::from(["call", "data", "end", "error", "cancel"]);
        let got = frame_union
            .union_members
            .as_ref()
            .map(|values| values.iter().map(String::as_str).collect::<BTreeSet<_>>())
            .unwrap_or_default();
        if got.is_empty() {
            vetoes.push("TypeSpec RpcFrame must be a union of call/data/end/error/cancel".to_owned());
        } else if got != want {
            vetoes.push(format!("TypeSpec RpcFrame arms {got:?} != {want:?}"));
        }
        let mut type_fields = BTreeSet::new();
        for arm in [
            "RpcCallFrame",
            "RpcDataFrame",
            "RpcEndFrame",
            "RpcErrorFrame",
            "RpcCancelFrame",
        ] {
            let qualified = format!("Ores.Rpc.V2.{arm}");
            if let Some(model) = shape(&typespec, &qualified, &mut vetoes) {
                type_fields.extend(model.fields.keys().cloned());
            }
        }
        let schema_fields = frame_schema.fields.keys().cloned().collect::<BTreeSet<_>>();
        let missing = schema_fields.difference(&type_fields).cloned().collect::<Vec<_>>();
        if !missing.is_empty() {
            vetoes.push(format!("JSON Schema rpc-frame fields missing from TypeSpec arms: {missing:?}"));
        }
        notes.push("expected-delta v2-union-vs-if-then applies to rpc-frame".to_owned());
        if let Some(proto_frame) = shape(&proto, "ores.rpc.v2.RpcFrame", &mut vetoes) {
            let proto_fields = proto_frame.fields.keys().cloned().collect::<BTreeSet<_>>();
            let missing = schema_fields.difference(&proto_fields).cloned().collect::<Vec<_>>();
            if !missing.is_empty() {
                vetoes.push(format!("JSON Schema rpc-frame fields missing from proto: {missing:?}"));
            }
            notes.push("expected-delta v2-proto-flattened applies to ores.rpc.v2.RpcFrame".to_owned());
        }
    }

    if let (Some(transport), Some(call_schema)) = (
        typespec.get("Ores.Rpc.V1.Transport"),
        schemas.get("rpc-call"),
    ) {
        let schema_transport = call_schema
            .fields
            .get("transport")
            .and_then(|field| field.enum_values.clone())
            .unwrap_or_default();
        let mut left = transport.enum_values.clone().unwrap_or_default();
        let mut right = schema_transport.clone();
        left.sort();
        right.sort();
        if left != right {
            vetoes.push(format!("TypeSpec Transport {left:?} != JSON Schema {right:?}"));
        }
        if let Some(proto_transport) = proto.get("ores.rpc.v1.Transport") {
            if let Some(values) = &proto_transport.enum_values {
                let mut mapped = proto_enum_json_values(values);
                let mut expected = schema_transport;
                mapped.sort();
                expected.sort();
                if mapped != expected {
                    vetoes.push(format!("protobuf Transport {mapped:?} != JSON Schema {expected:?}"));
                }
                notes.push("expected-delta proto-transport-unspecified applies to ores.rpc.v1.Transport".to_owned());
            }
        }
    }

    vetoes.extend(check_protobuf_lock(&proto, &lock));
    let required_delta_ids = BTreeSet::from([
        "proto-transport-unspecified",
        "proto-json-bytes",
        "v2-union-vs-if-then",
        "v2-unevaluated-properties",
        "additional-properties-closed",
        "v2-meta-map-vs-object",
        "v2-proto-flattened",
    ]);
    let missing = required_delta_ids
        .difference(&expected_ids)
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        vetoes.push(format!("expected-deltas.json missing ids {missing:?}"));
    }

    Ok(json!({
        "ok": vetoes.is_empty(),
        "vetoes": vetoes,
        "notes": notes,
        "shapes": {
            "json-schema": schemas.keys().cloned().collect::<Vec<_>>(),
            "typespec": typespec.keys().cloned().collect::<Vec<_>>(),
            "protobuf": proto.keys().cloned().collect::<Vec<_>>(),
        }
    }))
}

fn same_constraint(vetoes: &mut Vec<String>, left: &Option<Value>, right: &Option<Value>, label: &str) {
    if left != right {
        vetoes.push(format!("{label}: JSON Schema={left:?} TypeSpec={right:?}"));
    }
}

fn strict_shape(vetoes: &mut Vec<String>, schema: &Shape, typespec: &Shape, enums: &BTreeMap<String, Vec<String>>) {
    let schema_names = schema.fields.keys().cloned().collect::<BTreeSet<_>>();
    let type_names = typespec.fields.keys().cloned().collect::<BTreeSet<_>>();
    if schema_names != type_names {
        vetoes.push(format!(
            "{}: fields {type_names:?} != JSON Schema {schema_names:?}",
            typespec.name
        ));
        return;
    }
    for name in schema_names {
        let schema_field = &schema.fields[&name];
        let type_field = &typespec.fields[&name];
        let (type_kind, type_enum) = if type_field.kind == "ref"
            && matches!(type_field.name.as_str(), "transport" | "rpc.transport")
        {
            enums
                .get("Ores.Rpc.V1.Transport")
                .map_or(("ref", None), |values| ("enum", Some(values.clone())))
        } else {
            (type_field.kind.as_str(), type_field.enum_values.clone())
        };
        let schema_kind = if schema_field.enum_values.is_some() && schema_field.kind == "string" {
            "enum"
        } else {
            schema_field.kind.as_str()
        };
        if schema_kind != type_kind {
            vetoes.push(format!(
                "{}.{}.kind: JSON Schema={schema_kind:?} TypeSpec={type_kind:?}",
                typespec.name, name
            ));
        }
        if schema_field.required != type_field.required {
            vetoes.push(format!(
                "{}.{}.required: JSON Schema={} TypeSpec={}",
                typespec.name, name, schema_field.required, type_field.required
            ));
        }
        same_constraint(
            vetoes,
            &schema_field.const_value,
            &type_field.const_value,
            &format!("{}.{}.const", typespec.name, name),
        );
        for (label, left, right) in [
            (
                "min_length",
                schema_field.min_length.map(|value| json!(value)),
                type_field.min_length.map(|value| json!(value)),
            ),
            (
                "max_length",
                schema_field.max_length.map(|value| json!(value)),
                type_field.max_length.map(|value| json!(value)),
            ),
            (
                "pattern",
                schema_field.pattern.clone().map(Value::String),
                type_field.pattern.clone().map(Value::String),
            ),
            (
                "minimum",
                schema_field.minimum.map(|value| json!(value)),
                type_field.minimum.map(|value| json!(value)),
            ),
            (
                "maximum",
                schema_field.maximum.map(|value| json!(value)),
                type_field.maximum.map(|value| json!(value)),
            ),
        ] {
            same_constraint(vetoes, &left, &right, &format!("{}.{}.{label}", typespec.name, name));
        }
        let mut schema_enum = schema_field.enum_values.clone().unwrap_or_default();
        let mut resolved_enum = type_enum.unwrap_or_default();
        schema_enum.sort();
        resolved_enum.sort();
        if schema_enum != resolved_enum {
            vetoes.push(format!(
                "{}.{}.enum: JSON Schema={schema_enum:?} TypeSpec={resolved_enum:?}",
                typespec.name, name
            ));
        }
    }
}

const EXPECTED_DELTA_IDS: &[&str] = &[
    "proto-transport-unspecified",
    "proto-json-bytes",
    "v2-union-vs-if-then",
    "v2-unevaluated-properties",
    "additional-properties-closed",
    "v2-meta-map-vs-object",
    "v2-proto-flattened",
];

fn audit_delta_allowlist(root: &Path, vetoes: &mut Vec<String>) -> CheckResult<()> {
    let document = read_json(&root.join("idl/expected-deltas.json"))?;
    let Some(deltas) = document.get("deltas").and_then(Value::as_array) else {
        vetoes.push("expected-deltas.json: deltas must be an array".to_owned());
        return Ok(());
    };
    let mut ids = Vec::new();
    for (index, delta) in deltas.iter().enumerate() {
        let Some(object) = delta.as_object() else {
            vetoes.push(format!("expected-deltas.json[{index}]: expected object"));
            continue;
        };
        let Some(id) = object.get("id").and_then(Value::as_str).filter(|value| !value.is_empty()) else {
            vetoes.push(format!("expected-deltas.json[{index}]: id required"));
            continue;
        };
        ids.push(id.to_owned());
        if object
            .get("reason")
            .and_then(Value::as_str)
            .is_none_or(|reason| reason.trim().len() < 24)
        {
            vetoes.push(format!(
                "expected-deltas.json[{index}] {id}: reviewable reason of at least 24 characters required"
            ));
        }
    }
    let duplicates = ids
        .iter()
        .filter(|id| ids.iter().filter(|other| *other == *id).count() > 1)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !duplicates.is_empty() {
        vetoes.push(format!("expected-deltas.json: duplicate ids {duplicates:?}"));
    }
    let actual = ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = EXPECTED_DELTA_IDS.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        vetoes.push(format!(
            "expected-deltas.json ids {actual:?} != exact allow-list {expected:?}"
        ));
    }
    Ok(())
}

fn raw_proto_assignments(text: &str, package: &str, kind: &str) -> BTreeMap<String, BTreeMap<String, i64>> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut result = BTreeMap::new();
    let mut index = 0;
    while index < lines.len() {
        let line = strip_comment(lines[index]);
        index += 1;
        let prefix = format!("{kind} ");
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        if !line.contains('{') {
            continue;
        }
        let local = rest.split_whitespace().next().unwrap_or_default().trim_end_matches('{');
        if local.is_empty() {
            continue;
        }
        let qualified = if package.is_empty() {
            local.to_owned()
        } else {
            format!("{package}.{local}")
        };
        let mut assignments = BTreeMap::new();
        let mut depth = line.matches('{').count() as i64 - line.matches('}').count() as i64;
        while depth > 0 && index < lines.len() {
            let raw = lines[index];
            index += 1;
            depth += raw.matches('{').count() as i64 - raw.matches('}').count() as i64;
            if depth <= 0 {
                continue;
            }
            let body = strip_comment(raw).trim_end_matches(';').trim();
            let Some((left, right)) = body.split_once('=') else {
                continue;
            };
            let number_text = right
                .trim()
                .split_once('[')
                .map_or(right.trim(), |(number, _)| number.trim());
            let Ok(number) = number_text.parse::<i64>() else {
                continue;
            };
            let name = if kind == "enum" {
                left.trim().to_owned()
            } else {
                let raw_name = left.split_whitespace().last().unwrap_or_default();
                let options = right
                    .split_once('[')
                    .and_then(|(_, options)| options.rsplit_once(']').map(|(options, _)| options))
                    .unwrap_or_default();
                json_name_from_options(options).unwrap_or_else(|| raw_name.to_owned())
            };
            if !name.is_empty() {
                assignments.insert(name, number);
            }
        }
        result.insert(qualified, assignments);
    }
    result
}

fn package_from_proto(text: &str) -> String {
    text.lines()
        .map(strip_comment)
        .find_map(|line| {
            line.strip_prefix("package ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(|value| value.trim().to_owned())
        })
        .unwrap_or_default()
}

fn audit_proto_source_coverage(
    root: &Path,
    proto: &BTreeMap<String, Shape>,
    vetoes: &mut Vec<String>,
) -> CheckResult<()> {
    let mut source_messages = BTreeMap::new();
    let mut source_enums = BTreeMap::new();
    for path in collect_files(&root.join("idl/protobuf"), "proto")? {
        let text = read_text(&path)?;
        let package = package_from_proto(&text);
        source_messages.extend(raw_proto_assignments(&text, &package, "message"));
        source_enums.extend(raw_proto_assignments(&text, &package, "enum"));
    }
    for (name, assignments) in &source_messages {
        let Some(parsed) = proto.get(name) else {
            vetoes.push(format!("{name}: protobuf source message was not parsed"));
            continue;
        };
        let parsed_assignments = parsed
            .fields
            .values()
            .filter_map(|field| field.proto_number.map(|number| (field.name.clone(), number)))
            .collect::<BTreeMap<_, _>>();
        if *assignments != parsed_assignments {
            vetoes.push(format!(
                "{name}: protobuf parser coverage {parsed_assignments:?} != source {assignments:?}"
            ));
        }
    }
    let lock = read_json(&root.join("idl/protobuf.lock.json"))?;
    let locked_enums = lock.get("enums").cloned().unwrap_or(Value::Null);
    let source_enum_value = serde_json::to_value(source_enums).map_err(|error| error.to_string())?;
    if locked_enums != source_enum_value {
        vetoes.push(format!(
            "protobuf ledger enums {locked_enums} != source enums {source_enum_value}"
        ));
    }
    Ok(())
}

fn audit_proto_ledger(
    root: &Path,
    proto: &BTreeMap<String, Shape>,
    vetoes: &mut Vec<String>,
) -> CheckResult<()> {
    let lock = read_json(&root.join("idl/protobuf.lock.json"))?;
    let Some(messages) = lock.get("messages").and_then(Value::as_object) else {
        vetoes.push("protobuf.lock.json: messages must be an object".to_owned());
        return Ok(());
    };
    let source = proto
        .iter()
        .filter(|(_, shape)| !shape.fields.is_empty())
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    let locked = messages.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if source != locked {
        vetoes.push(format!(
            "protobuf ledger messages {locked:?} != source messages {source:?}"
        ));
    }
    vetoes.extend(check_protobuf_lock(proto, &lock));
    for (name, shape) in proto {
        let numbers = shape.fields.values().filter_map(|field| field.proto_number).collect::<Vec<_>>();
        let duplicate = numbers
            .iter()
            .filter(|number| numbers.iter().filter(|other| *other == *number).count() > 1)
            .copied()
            .collect::<BTreeSet<_>>();
        if !duplicate.is_empty() {
            vetoes.push(format!("{name}: duplicate field numbers {duplicate:?}"));
        }
        if numbers.iter().any(|number| *number < 1) {
            vetoes.push(format!("{name}: field numbers must be positive"));
        }
    }
    Ok(())
}

fn audit_typespec_references(root: &Path, vetoes: &mut Vec<String>) -> CheckResult<()> {
    let v1 = read_text(&root.join("idl/typespec/v1.tsp"))?;
    let telemetry = read_text(&root.join("idl/typespec/telemetry.tsp"))?;
    let transport_count = v1
        .lines()
        .map(strip_comment)
        .filter(|line| *line == "transport?: Transport;")
        .count();
    if transport_count != 2 {
        vetoes.push("TypeSpec v1 must contain exactly two optional Transport fields".to_owned());
    }
    let telemetry_count = telemetry
        .lines()
        .map(strip_comment)
        .filter(|line| *line == "`rpc.transport`: Ores.Rpc.V1.Transport;")
        .count();
    if telemetry_count != 1 {
        vetoes.push("TypeSpec telemetry must bind rpc.transport to Ores.Rpc.V1.Transport".to_owned());
    }
    Ok(())
}

pub fn audit(root: &Path) -> CheckResult<Value> {
    let base = cross_check(root)?;
    let mut vetoes = base["vetoes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let schemas = load_json_shapes(root)?;
    let typespec = load_all_typespec(root)?;
    let proto = load_all_proto(root)?;
    let enums = typespec
        .iter()
        .filter_map(|(name, shape)| shape.enum_values.clone().map(|values| (name.clone(), values)))
        .collect::<BTreeMap<_, _>>();
    for (schema_name, type_name) in [
        ("rpc-call", "Ores.Rpc.V1.RpcCall"),
        ("rpc-receipt", "Ores.Rpc.V1.RpcReceipt"),
        ("telemetry-attributes", "Ores.Rpc.Telemetry.TelemetryAttributes"),
    ] {
        match (schemas.get(schema_name), typespec.get(type_name)) {
            (Some(schema), Some(type_shape)) => strict_shape(&mut vetoes, schema, type_shape, &enums),
            _ => vetoes.push(format!("missing strict pair {schema_name} / {type_name}")),
        }
    }
    audit_delta_allowlist(root, &mut vetoes)?;
    audit_proto_ledger(root, &proto, &mut vetoes)?;
    audit_proto_source_coverage(root, &proto, &mut vetoes)?;
    audit_typespec_references(root, &mut vetoes)?;

    let expected = BTreeSet::from([
        "Ores.ApiDocs.DocsDiscoveryManifest",
        "Ores.ApiDocs.DocsProjectionRoutes",
        "Ores.Rpc.Telemetry.TelemetryAttributes",
        "Ores.Rpc.V1.RpcCall",
        "Ores.Rpc.V1.RpcReceipt",
        "Ores.Rpc.V1.Transport",
        "Ores.Rpc.V2.RpcCallFrame",
        "Ores.Rpc.V2.RpcCancelFrame",
        "Ores.Rpc.V2.RpcDataFrame",
        "Ores.Rpc.V2.RpcEndFrame",
        "Ores.Rpc.V2.RpcErrorFrame",
        "Ores.Rpc.V2.RpcFrame",
    ]);
    let actual = typespec.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual != expected {
        vetoes.push(format!(
            "TypeSpec declaration set {actual:?} != reviewed set {expected:?}"
        ));
    }
    vetoes.sort();
    vetoes.dedup();
    Ok(json!({
        "ok": vetoes.is_empty(),
        "vetoes": vetoes,
        "baseNotes": base["notes"],
    }))
}

pub fn run_cross_check(root: &Path, write_report: Option<&Path>) -> CheckResult<()> {
    let report = cross_check(root)?;
    if let Some(path) = write_report {
        write_json(path, &report)?;
    }
    println!("{}", serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?);
    if report["ok"] == true {
        Ok(())
    } else {
        Err("rpc idl peer-authority veto".to_owned())
    }
}

pub fn run_audit(root: &Path) -> CheckResult<()> {
    let report = audit(root)?;
    println!("{}", serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?);
    if report["ok"] == true {
        Ok(())
    } else {
        Err("strict rpc idl admission veto".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use super::super::common::{copy_tree, repo_root, TempDir};

    fn copied_idl_root() -> (TempDir, PathBuf) {
        let source = repo_root();
        let temp = TempDir::new("rpc-idl").unwrap();
        let root = temp.path().join("repo");
        copy_tree(&source.join("idl"), &root.join("idl")).unwrap();
        copy_tree(&source.join("json-schema"), &root.join("json-schema")).unwrap();
        (temp, root)
    }

    #[test]
    fn current_tree_cross_check_and_audit_are_green() {
        let root = repo_root();
        let cross = cross_check(&root).unwrap();
        assert_eq!(cross["vetoes"], json!([]));
        assert_eq!(cross["ok"], true);
        let strict = audit(&root).unwrap();
        assert_eq!(strict["vetoes"], json!([]));
        assert_eq!(strict["ok"], true);
    }

    #[test]
    fn typespec_rpc_call_constraints_are_parsed() {
        let root = repo_root();
        let shapes = load_all_typespec(&root).unwrap();
        let call = &shapes["Ores.Rpc.V1.RpcCall"];
        assert_eq!(
            call.fields.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "v", "op", "id", "key", "transport", "path", "query", "headers", "body",
                "traceId", "spanId"
            ])
        );
        assert_eq!(call.fields["v"].const_value, Some(json!(1)));
        assert_eq!(call.fields["op"].const_value, Some(json!("call")));
        assert_eq!(call.fields["id"].max_length, Some(128));
        assert_eq!(call.fields["key"].pattern.as_deref(), Some("^[A-Za-z][A-Za-z0-9_]*$"));
    }

    #[test]
    fn dropping_typespec_field_vetoes() {
        let (_temp, root) = copied_idl_root();
        let path = root.join("idl/typespec/v1.tsp");
        let text = read_text(&path).unwrap().replace(
            "  @minLength(1)\n  @maxLength(32)\n  spanId?: string;\n",
            "",
        );
        fs::write(path, text).unwrap();
        let report = cross_check(&root).unwrap();
        assert_eq!(report["ok"], false);
        assert!(report["vetoes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.as_str().unwrap().contains("spanId")));
    }

    #[test]
    fn duplicate_proto_number_and_parser_gap_veto() {
        let (_temp, root) = copied_idl_root();
        let path = root.join("idl/protobuf/ores/rpc/v1/rpc.proto");
        let text = read_text(&path)
            .unwrap()
            .replace("optional string span_id = 10", "optional string span_id = 9");
        fs::write(&path, text).unwrap();
        let report = audit(&root).unwrap();
        assert_eq!(report["ok"], false);
        assert!(report["vetoes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.as_str().unwrap().contains("duplicate field numbers")));
    }
}
