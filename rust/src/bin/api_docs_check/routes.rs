use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use ores_api_docs::{path_template_vars, RouteMap};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Number, Value};

use super::common::{read_text, write_text, CheckResult, TempDir};

#[derive(Clone, Debug, PartialEq)]
pub enum OrderedValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

impl OrderedValue {
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_array(&self) -> Option<&[Self]> {
        if let Self::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Self)]> {
        if let Self::Object(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn get(&self, name: &str) -> Option<&Self> {
        self.as_object()?
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value))
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Number(value) => Value::Number(value.clone()),
            Self::String(value) => Value::String(value.clone()),
            Self::Array(items) => Value::Array(items.iter().map(Self::to_json).collect()),
            Self::Object(entries) => {
                let mut object = serde_json::Map::new();
                for (key, value) in entries {
                    object.insert(key.clone(), value.to_json());
                }
                Value::Object(object)
            }
        }
    }
}

struct OrderedValueVisitor;

impl<'de> Visitor<'de> for OrderedValueVisitor {
    type Value = OrderedValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(OrderedValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(OrderedValue::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(OrderedValue::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_f64(value)
            .map(OrderedValue::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(OrderedValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(OrderedValue::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(OrderedValue::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(OrderedValue::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(OrderedValue::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some((key, value)) = map.next_entry()? {
            values.push((key, value));
        }
        Ok(OrderedValue::Object(values))
    }
}

impl<'de> Deserialize<'de> for OrderedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OrderedValueVisitor)
    }
}

#[derive(Clone, Debug)]
pub struct OrderedRouteDoc {
    pub service: String,
    pub schema_version: String,
    pub map: Vec<(String, OrderedValue)>,
}

pub fn parse_ordered_route_doc(text: &str) -> CheckResult<OrderedRouteDoc> {
    let root: OrderedValue = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let service = root
        .get("service")
        .and_then(OrderedValue::as_str)
        .ok_or_else(|| "route map service must be a string".to_owned())?
        .to_owned();
    let schema_version = root
        .get("schema_version")
        .and_then(OrderedValue::as_str)
        .ok_or_else(|| "route map schema_version must be a string".to_owned())?
        .to_owned();
    let map = root
        .get("map")
        .and_then(OrderedValue::as_object)
        .ok_or_else(|| "route map map must be an object".to_owned())?
        .to_vec();
    Ok(OrderedRouteDoc {
        service,
        schema_version,
        map,
    })
}

fn py_json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            value if value <= '\u{1f}' => out.push_str(&format!("\\u{:04x}", value as u32)),
            value if value.is_ascii() => out.push(value),
            value if (value as u32) <= 0xffff => {
                out.push_str(&format!("\\u{:04x}", value as u32));
            }
            value => {
                let scalar = value as u32 - 0x1_0000;
                let high = 0xd800 + ((scalar >> 10) & 0x3ff);
                let low = 0xdc00 + (scalar & 0x3ff);
                out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
            }
        }
    }
    out.push('"');
    out
}

fn pascal(key: &str) -> String {
    if key.chars().next().is_some_and(char::is_uppercase) && !key.contains('_') {
        return key.to_owned();
    }
    key.replace('-', "_")
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

fn rust_field_name(name: &str) -> (String, String) {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
        "enum", "extern", "false", "fn", "for", "if", "impl", "in", "include", "let",
        "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self",
        "static", "struct", "super", "trait", "true", "type", "union", "unsafe", "use",
        "where", "while", "yield", "box",
    ];
    let mut rust_name = String::new();
    for character in name.chars() {
        if character.is_uppercase() {
            if !rust_name.is_empty() {
                rust_name.push('_');
            }
            for lowered in character.to_lowercase() {
                rust_name.push(lowered);
            }
        } else if character == '-' {
            rust_name.push('_');
        } else {
            rust_name.push(character);
        }
    }
    while rust_name.starts_with('_') {
        rust_name.remove(0);
    }
    if KEYWORDS.contains(&rust_name.as_str()) {
        rust_name.push('_');
    }
    let rename = if rust_name == name {
        String::new()
    } else {
        format!("    #[serde(rename = {})]\n", py_json_string(name))
    };
    (rust_name, rename)
}

fn schema_type(schema: &OrderedValue) -> Option<&OrderedValue> {
    schema.get("type")
}

fn ts_object_type(schema: &OrderedValue) -> String {
    let Some(properties) = schema.get("properties").and_then(OrderedValue::as_object) else {
        return "Record<string, unknown>".to_owned();
    };
    if properties.is_empty() {
        return "Record<string, unknown>".to_owned();
    }
    let required: BTreeSet<&str> = schema
        .get("required")
        .and_then(OrderedValue::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(OrderedValue::as_str)
        .collect();
    let fields = properties
        .iter()
        .map(|(name, subschema)| {
            let optional = if required.contains(name.as_str()) { "" } else { "?" };
            format!(
                "{}{optional}: {}",
                py_json_string(name),
                ts_type(Some(subschema), "unknown")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("{{ {fields} }}")
}

fn ts_type(schema: Option<&OrderedValue>, fallback: &str) -> String {
    let Some(schema) = schema else {
        return fallback.to_owned();
    };
    if let Some(values) = schema.get("enum").and_then(OrderedValue::as_array) {
        if values.iter().all(|value| value.as_str().is_some()) {
            return values
                .iter()
                .map(|value| py_json_string(value.as_str().expect("checked")))
                .collect::<Vec<_>>()
                .join(" | ");
        }
    }
    if let Some(types) = schema_type(schema).and_then(OrderedValue::as_array) {
        let mut parts = Vec::new();
        let mut nullable = false;
        for item in types {
            if item.as_str() == Some("null") {
                nullable = true;
                continue;
            }
            let synthetic = OrderedValue::Object(vec![("type".to_owned(), item.clone())]);
            parts.push(ts_type(Some(&synthetic), fallback));
        }
        let inner = if parts.is_empty() {
            fallback.to_owned()
        } else {
            parts.join(" | ")
        };
        return if nullable { format!("{inner} | null") } else { inner };
    }
    match schema_type(schema).and_then(OrderedValue::as_str) {
        Some("string") => "string".to_owned(),
        Some("integer" | "number") => "number".to_owned(),
        Some("boolean") => "boolean".to_owned(),
        Some("array") => format!("Array<{}>", ts_type(schema.get("items"), "unknown")),
        Some("object") => ts_object_type(schema),
        None if schema.get("properties").is_some() => ts_object_type(schema),
        _ => fallback.to_owned(),
    }
}

fn rust_type(schema: Option<&OrderedValue>, fallback: &str) -> String {
    let Some(schema) = schema else {
        return fallback.to_owned();
    };
    if let Some(types) = schema_type(schema).and_then(OrderedValue::as_array) {
        let mut non_null = types.iter().filter(|value| value.as_str() != Some("null"));
        let inner = non_null
            .next()
            .map(|kind| {
                let synthetic = OrderedValue::Object(vec![("type".to_owned(), kind.clone())]);
                rust_type(Some(&synthetic), fallback)
            })
            .unwrap_or_else(|| fallback.to_owned());
        return if types.iter().any(|value| value.as_str() == Some("null")) {
            format!("Option<{inner}>")
        } else {
            inner
        };
    }
    match schema_type(schema).and_then(OrderedValue::as_str) {
        Some("string") => "String".to_owned(),
        Some("integer") => "i64".to_owned(),
        Some("number") => "f64".to_owned(),
        Some("boolean") => "bool".to_owned(),
        Some("array") => format!(
            "Vec<{}>",
            rust_type(schema.get("items"), "serde_json::Value")
        ),
        _ => fallback.to_owned(),
    }
}

fn rust_struct(name: &str, schema: &OrderedValue) -> String {
    let properties = schema
        .get("properties")
        .and_then(OrderedValue::as_object)
        .unwrap_or(&[]);
    let required: BTreeSet<&str> = schema
        .get("required")
        .and_then(OrderedValue::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(OrderedValue::as_str)
        .collect();
    let mut fields = Vec::new();
    for (field_name, subschema) in properties {
        let mut field_type = rust_type(Some(subschema), "serde_json::Value");
        if !required.contains(field_name.as_str()) && !field_type.starts_with("Option<") {
            field_type = format!("Option<{field_type}>");
        }
        let (rust_name, rename) = rust_field_name(field_name);
        fields.push(format!("{rename}    pub {rust_name}: {field_type},"));
    }
    let body = if fields.is_empty() {
        "    // no fields".to_owned()
    } else {
        fields.join("\n")
    };
    format!(
        "#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]\npub struct {name} {{\n{body}\n}}\n"
    )
}

fn raw_field<'a>(raw: &'a OrderedValue, field: &str) -> Option<&'a OrderedValue> {
    raw.get(field)
}

pub fn gen_typescript(service: &str, ordered: &[(String, OrderedValue)], map: &RouteMap) -> CheckResult<String> {
    let mut companion = Vec::new();
    let mut lines = vec![
        "/** Generated from a route-map JSON. Do not edit by hand. */".to_owned(),
        String::new(),
        "export type HttpMethod = \"GET\" | \"POST\" | \"PUT\" | \"PATCH\" | \"DELETE\" | \"HEAD\" | \"OPTIONS\";".to_owned(),
        "export type RpcTransport = \"http\" | \"tcp\" | \"websocket\" | \"nats\";".to_owned(),
        String::new(),
        format!("export const SERVICE = {} as const;", py_json_string(service)),
        String::new(),
        "export const Routes = {".to_owned(),
    ];
    for (key, raw) in ordered {
        let entry = map.lookup(key).ok_or_else(|| format!("missing normalized route {key}"))?;
        let path_schema = raw_field(raw, "path_params");
        let query_schema = raw_field(raw, "query_schema");
        let header_schema = raw_field(raw, "header_schema");
        let request_schema = raw_field(raw, "request_schema");
        let response_schema = raw_field(raw, "response_schema");
        let path_type = if path_schema.is_some() {
            ts_type(path_schema, "{ [k: string]: string }")
        } else {
            "Record<string, never>".to_owned()
        };
        let query_type = if query_schema.is_some() {
            ts_type(query_schema, "Record<string, never>")
        } else {
            "Record<string, never>".to_owned()
        };
        let header_type = if header_schema.is_some() {
            ts_type(header_schema, "Record<string, never>")
        } else {
            "Record<string, never>".to_owned()
        };
        let request_type = if request_schema.is_some() {
            ts_type(request_schema, "unknown")
        } else {
            "void".to_owned()
        };
        let response_type = if response_schema.is_some() {
            ts_type(response_schema, "unknown")
        } else {
            "unknown".to_owned()
        };
        companion.push((
            key.clone(),
            path_type.clone(),
            query_type,
            header_type,
            request_type,
            response_type,
        ));
        let variables = path_template_vars(&entry.path).map_err(|error| error.to_string())?;
        let build = if variables.is_empty() {
            "undefined as ((p: Record<string, never>) => string) | undefined".to_owned()
        } else {
            format!(
                "(p: {path_type}) => {}.replace(/\\{{([^}}]+)\\}}/g, (_, n) => encodeURIComponent(String((p as Record<string, string>)[n])))",
                py_json_string(&entry.path)
            )
        };
        let methods = entry
            .methods
            .iter()
            .map(|method| py_json_string(method))
            .collect::<Vec<_>>()
            .join(", ");
        let transports = entry
            .transports
            .iter()
            .map(|transport| py_json_string(transport))
            .collect::<Vec<_>>()
            .join(", ");
        lines.extend([
            format!("  {}: {{", py_json_string(key)),
            format!("    key: {},", py_json_string(key)),
            format!("    path: {} as const,", py_json_string(&entry.path)),
            format!("    methods: [{methods}] as const,"),
            format!("    transports: [{transports}] as const,"),
            format!("    buildPath: {build},"),
            "  },".to_owned(),
        ]);
    }
    lines.extend([
        "} as const;".to_owned(),
        String::new(),
        "export type RouteName = keyof typeof Routes;".to_owned(),
        String::new(),
        "export interface RouteTypes {".to_owned(),
    ]);
    for (key, path_type, query_type, header_type, request_type, response_type) in companion {
        lines.push(format!(
            "  {}: {{ path: {path_type}; query: {query_type}; headers: {header_type}; body: {request_type}; response: {response_type} }};",
            py_json_string(&key)
        ));
    }
    lines.extend([
        "}".to_owned(),
        String::new(),
        "/** Adding a map key without a handler is a TypeScript error. */".to_owned(),
        "export type RouteHandlers<Ctx> = {".to_owned(),
        "  [K in RouteName]: (ctx: Ctx, args: {".to_owned(),
        "    path: RouteTypes[K][\"path\"];".to_owned(),
        "    query: RouteTypes[K][\"query\"];".to_owned(),
        "    headers: RouteTypes[K][\"headers\"];".to_owned(),
        "    body: RouteTypes[K][\"body\"];".to_owned(),
        "  }) => Promise<RouteTypes[K][\"response\"]> | RouteTypes[K][\"response\"];".to_owned(),
        "};".to_owned(),
        String::new(),
        "export function lookup<K extends RouteName>(key: K): (typeof Routes)[K] {".to_owned(),
        "  return Routes[key];".to_owned(),
        "}".to_owned(),
        String::new(),
    ]);
    Ok(format!("{}\n", lines.join("\n")))
}

pub fn gen_dart(service: &str, ordered: &[(String, OrderedValue)], map: &RouteMap) -> CheckResult<String> {
    let mut lines = vec![
        "/// Generated from a route-map JSON. Do not edit by hand.".to_owned(),
        "library;".to_owned(),
        String::new(),
        format!("const String kService = {};", py_json_string(service)),
        String::new(),
        "class RouteMeta {".to_owned(),
        "  const RouteMeta({required this.key, required this.path, required this.methods, this.transports = const ['http']});".to_owned(),
        "  final String key;".to_owned(),
        "  final String path;".to_owned(),
        "  final List<String> methods;".to_owned(),
        "  final List<String> transports;".to_owned(),
        "  String expand(Map<String, String> params) {".to_owned(),
        "    var out = path;".to_owned(),
        "    params.forEach((k, v) {".to_owned(),
        "      out = out.replaceAll('{$k}', Uri.encodeComponent(v));".to_owned(),
        "    });".to_owned(),
        "    return out;".to_owned(),
        "  }".to_owned(),
        "}".to_owned(),
        String::new(),
        "abstract final class Routes {".to_owned(),
    ];
    for (key, _) in ordered {
        let entry = map.lookup(key).ok_or_else(|| format!("missing normalized route {key}"))?;
        let identifier = if key.chars().next().is_some_and(char::is_uppercase) {
            format!("rpc{key}")
        } else {
            key.clone()
        };
        let methods = entry
            .methods
            .iter()
            .map(|method| py_json_string(method))
            .collect::<Vec<_>>()
            .join(", ");
        let transports = entry
            .transports
            .iter()
            .map(|transport| py_json_string(transport))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "  static const {identifier} = RouteMeta(key: {}, path: {}, methods: [{methods}], transports: [{transports}]);",
            py_json_string(key),
            py_json_string(&entry.path)
        ));
    }
    lines.push(String::new());
    lines.push("  static const Map<String, RouteMeta> byKey = {".to_owned());
    for (key, _) in ordered {
        let identifier = if key.chars().next().is_some_and(char::is_uppercase) {
            format!("rpc{key}")
        } else {
            key.clone()
        };
        lines.push(format!("    {}: {identifier},", py_json_string(key)));
    }
    lines.extend(["  };".to_owned(), "}".to_owned(), String::new()]);
    Ok(format!("{}\n", lines.join("\n")))
}

pub fn gen_rust(service: &str, ordered: &[(String, OrderedValue)], map: &RouteMap) -> CheckResult<String> {
    let mut variants = Vec::new();
    let mut as_str = Vec::new();
    let mut from_str = Vec::new();
    let mut path_match = Vec::new();
    let mut methods_match = Vec::new();
    let mut transports_match = Vec::new();
    let mut structs = Vec::new();
    let mut all = Vec::new();
    for (key, raw) in ordered {
        let variant = pascal(key);
        let entry = map.lookup(key).ok_or_else(|| format!("missing normalized route {key}"))?;
        variants.push(format!("    {variant},"));
        all.push(format!("Self::{variant}"));
        as_str.push(format!("            Self::{variant} => {},", py_json_string(key)));
        from_str.push(format!(
            "            {} => Some(Self::{variant}),",
            py_json_string(key)
        ));
        path_match.push(format!(
            "            Self::{variant} => {},",
            py_json_string(&entry.path)
        ));
        let methods = entry
            .methods
            .iter()
            .map(|method| py_json_string(method))
            .collect::<Vec<_>>()
            .join(", ");
        methods_match.push(format!("            Self::{variant} => &[{methods}],"));
        let transports = entry
            .transports
            .iter()
            .map(|transport| py_json_string(transport))
            .collect::<Vec<_>>()
            .join(", ");
        transports_match.push(format!("            Self::{variant} => &[{transports}],"));
        for (field, suffix) in [
            ("path_params", "Path"),
            ("query_schema", "Query"),
            ("header_schema", "Headers"),
            ("request_schema", "Request"),
            ("response_schema", "Response"),
        ] {
            if let Some(schema) = raw_field(raw, field) {
                if schema
                    .get("properties")
                    .and_then(OrderedValue::as_object)
                    .is_some_and(|properties| !properties.is_empty())
                {
                    structs.push(rust_struct(&format!("{variant}{suffix}"), schema));
                }
            }
        }
    }
    let struct_block = structs.join("\n");
    Ok(format!(
        "//! Generated from a route-map JSON. Do not edit by hand.\n//! Exhaustive `RouteKey` match is the backend compile check.\n#![allow(dead_code)]\n\npub const SERVICE: &str = {};\n\n#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]\npub enum RouteKey {{\n{}\n}}\n\nimpl RouteKey {{\n    pub const ALL: &'static [Self] = &[{}];\n\n    #[must_use]\n    pub fn as_str(self) -> &'static str {{\n        match self {{\n{}\n        }}\n    }}\n\n    #[must_use]\n    pub fn parse(key: &str) -> Option<Self> {{\n        match key {{\n{}\n            _ => None,\n        }}\n    }}\n\n    #[must_use]\n    pub fn path(self) -> &'static str {{\n        match self {{\n{}\n        }}\n    }}\n\n    #[must_use]\n    pub fn methods(self) -> &'static [&'static str] {{\n        match self {{\n{}\n        }}\n    }}\n\n    #[must_use]\n    pub fn transports(self) -> &'static [&'static str] {{\n        match self {{\n{}\n        }}\n    }}\n}}\n\n{}\n",
        py_json_string(service),
        variants.join("\n"),
        all.join(", "),
        as_str.join("\n"),
        from_str.join("\n"),
        path_match.join("\n"),
        methods_match.join("\n"),
        transports_match.join("\n"),
        struct_block
    ))
}

pub fn gen_gleam(service: &str, ordered: &[(String, OrderedValue)], map: &RouteMap) -> CheckResult<String> {
    let mut variants = Vec::new();
    let mut to_string = Vec::new();
    let mut parse = Vec::new();
    let mut path_match = Vec::new();
    let mut methods_match = Vec::new();
    let mut transports_match = Vec::new();
    let mut all = Vec::new();
    for (key, _) in ordered {
        let variant = pascal(key);
        let entry = map.lookup(key).ok_or_else(|| format!("missing normalized route {key}"))?;
        variants.push(format!("  {variant}"));
        all.push(variant.clone());
        to_string.push(format!("    {variant} -> {}", py_json_string(key)));
        parse.push(format!("    {} -> Ok({variant})", py_json_string(key)));
        path_match.push(format!("    {variant} -> {}", py_json_string(&entry.path)));
        methods_match.push(format!(
            "    {variant} -> [{}]",
            entry
                .methods
                .iter()
                .map(|method| py_json_string(method))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        transports_match.push(format!(
            "    {variant} -> [{}]",
            entry
                .transports
                .iter()
                .map(|transport| py_json_string(transport))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(format!(
        "//// Generated from a route-map JSON. Do not edit by hand.\n//// Exhaustive `RouteKey` case is the backend compile check.\n\npub const service: String = {}\n\npub type RouteKey {{\n{}\n}}\n\npub fn all() -> List(RouteKey) {{\n  [{}]\n}}\n\npub fn to_string(key: RouteKey) -> String {{\n  case key {{\n{}\n  }}\n}}\n\npub fn parse(key: String) -> Result(RouteKey, Nil) {{\n  case key {{\n{}\n    _ -> Error(Nil)\n  }}\n}}\n\npub fn path(key: RouteKey) -> String {{\n  case key {{\n{}\n  }}\n}}\n\npub fn methods(key: RouteKey) -> List(String) {{\n  case key {{\n{}\n  }}\n}}\n\npub fn transports(key: RouteKey) -> List(String) {{\n  case key {{\n{}\n  }}\n}}\n",
        py_json_string(service),
        variants.join("\n"),
        all.join(", "),
        to_string.join("\n"),
        parse.join("\n"),
        path_match.join("\n"),
        methods_match.join("\n"),
        transports_match.join("\n")
    ))
}

fn stem_for(path: &Path) -> CheckResult<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid route-map file name: {}", path.display()))?;
    Ok(name
        .strip_suffix(".route-map.json")
        .unwrap_or(name)
        .replace('-', "_"))
}

pub fn render_outputs(map_path: &Path) -> CheckResult<BTreeMap<PathBuf, String>> {
    let text = read_text(map_path)?;
    let ordered = parse_ordered_route_doc(&text)?;
    if ordered.schema_version.starts_with("2.") {
        return Err(format!(
            "{} is RIDL v2; use the RIDL generator instead",
            map_path.display()
        ));
    }
    let map = RouteMap::from_json_str(&text).map_err(|error| format!("{}: {error}", map_path.display()))?;
    let stem = stem_for(map_path)?;
    let normalize = |text: String| format!("{}\n", text.trim_end_matches('\n'));
    Ok(BTreeMap::from([
        (
            PathBuf::from(format!("typescript/{stem}.ts")),
            normalize(gen_typescript(&ordered.service, &ordered.map, &map)?),
        ),
        (
            PathBuf::from(format!("dart/lib/{stem}.dart")),
            normalize(gen_dart(&ordered.service, &ordered.map, &map)?),
        ),
        (
            PathBuf::from(format!("rust/src/{stem}.rs")),
            normalize(gen_rust(&ordered.service, &ordered.map, &map)?),
        ),
        (
            PathBuf::from(format!("gleam/src/{stem}.gleam")),
            normalize(gen_gleam(&ordered.service, &ordered.map, &map)?),
        ),
    ]))
}

pub fn default_maps(root: &Path) -> CheckResult<Vec<PathBuf>> {
    let examples = root.join("examples");
    let mut maps = Vec::new();
    for entry in fs::read_dir(&examples).map_err(|error| format!("{}: {error}", examples.display()))? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".route-map.json"))
        {
            continue;
        }
        let text = read_text(&path)?;
        let doc: Value = serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        if !doc
            .get("schema_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .starts_with("2.")
        {
            maps.push(path);
        }
    }
    maps.sort();
    Ok(maps)
}

fn make_writable(path: &Path) -> CheckResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(permissions.mode() | 0o200);
            fs::set_permissions(path, permissions)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn make_readonly(path: &Path) -> CheckResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() & !0o222);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

pub fn write_outputs(map_path: &Path, out_dir: &Path) -> CheckResult<Vec<PathBuf>> {
    let outputs = render_outputs(map_path)?;
    let mut paths = Vec::new();
    for (relative, content) in outputs {
        let path = out_dir.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        make_writable(&path)?;
        write_text(&path, &content)?;
        make_readonly(&path)?;
        paths.push(relative);
    }
    Ok(paths)
}

pub fn run_generate_routes(root: &Path, maps: &[PathBuf], out: &Path, check: bool) -> CheckResult<()> {
    let maps = if maps.is_empty() {
        default_maps(root)?
    } else {
        maps.iter()
            .map(|path| if path.is_absolute() { path.clone() } else { root.join(path) })
            .collect()
    };
    let mut v1 = Vec::new();
    for path in maps {
        let text = read_text(&path)?;
        let doc = parse_ordered_route_doc(&text)?;
        if doc.schema_version.starts_with("2.") {
            eprintln!("note: skipping RIDL v2 map {} (use ridl generate)", path.display());
        } else {
            v1.push(path);
        }
    }
    if v1.is_empty() {
        return Err("no v1 route maps".to_owned());
    }
    if check {
        let temp = TempDir::new("api-docs-routes")?;
        let mut produced = Vec::new();
        for map_path in &v1 {
            produced.extend(write_outputs(map_path, temp.path())?);
        }
        produced.sort();
        produced.dedup();
        let mut drift = Vec::new();
        for relative in produced {
            let expected = temp.path().join(&relative);
            let existing = out.join(&relative);
            let expected_text = read_text(&expected)?;
            if read_text(&existing).ok().as_deref() != Some(expected_text.as_str()) {
                drift.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
        if drift.is_empty() {
            println!("generated routes ok");
            return Ok(());
        }
        let mut message = String::from("generated routes are stale:\n");
        for item in drift {
            message.push_str(&format!("  - {item}\n"));
        }
        message.push_str(
            "run: cargo run --manifest-path rust/Cargo.toml --bin api-docs-check -- generate-routes\n",
        );
        return Err(message.trim_end().to_owned());
    }
    for map_path in &v1 {
        for relative in write_outputs(map_path, out)? {
            println!("wrote: {}", out.join(relative).display());
        }
    }
    Ok(())
}

pub fn mechanism_manifest(map: &RouteMap) -> Value {
    let mut object = serde_json::Map::new();
    for (key, entry) in &map.map {
        let mut item = serde_json::Map::new();
        item.insert("key".to_owned(), Value::String(key.clone()));
        item.insert("path".to_owned(), Value::String(entry.path.clone()));
        item.insert(
            "methods".to_owned(),
            Value::Array(entry.methods.iter().cloned().map(Value::String).collect()),
        );
        item.insert(
            "transports".to_owned(),
            Value::Array(entry.transports.iter().cloned().map(Value::String).collect()),
        );
        if let Some(value) = &entry.tcp_framing {
            item.insert("tcpFraming".to_owned(), Value::String(value.clone()));
        }
        item.insert(
            "delivery".to_owned(),
            Value::String(entry.delivery.clone().unwrap_or_else(|| "direct".to_owned())),
        );
        if let Some(value) = &entry.alias_of {
            item.insert("aliasOf".to_owned(), Value::String(value.clone()));
        }
        if let Some(value) = &entry.opto_sync {
            item.insert(
                "optoSync".to_owned(),
                serde_json::to_value(value).expect("opto-sync metadata is serializable"),
            );
        }
        object.insert(key.clone(), Value::Object(item));
    }
    Value::Object(object)
}

pub fn compact_json(value: &Value) -> CheckResult<String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

pub fn pretty_json(value: &Value) -> CheckResult<String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

pub fn go_ident(key: &str) -> CheckResult<String> {
    let value = pascal(key);
    if value.chars().next().is_some_and(char::is_alphabetic) {
        Ok(value)
    } else {
        Err(format!("cannot form Go identifier from {key:?}"))
    }
}

pub fn insert_before(text: &str, marker: &str, declaration: &str, language: &str) -> CheckResult<String> {
    let Some(index) = text.find(marker) else {
        return Err(format!("{language} renderer no longer contains {marker:?}"));
    };
    let mut output = String::with_capacity(text.len() + declaration.len());
    output.push_str(&text[..index]);
    output.push_str(declaration);
    output.push_str(&text[index..]);
    Ok(output)
}

pub fn source_ordered_map(path: &Path) -> CheckResult<(OrderedRouteDoc, RouteMap)> {
    let text = read_text(path)?;
    let ordered = parse_ordered_route_doc(&text)?;
    let map = RouteMap::from_json_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((ordered, map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::common::repo_root;

    #[test]
    fn ordered_json_preserves_source_route_and_property_order() {
        let root = repo_root();
        let text = read_text(&root.join("examples/rpc-transports.route-map.json")).unwrap();
        let doc = parse_ordered_route_doc(&text).unwrap();
        assert_eq!(doc.map[0].0, "healthz");
        let headers = doc
            .map
            .iter()
            .find(|(key, _)| key == "get_item")
            .unwrap()
            .1
            .get("header_schema")
            .unwrap()
            .get("properties")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(headers[0].0, "x-request-id");
    }

    #[test]
    fn rust_renderer_matches_committed_rpc_transport_surface() {
        let root = repo_root();
        let path = root.join("examples/rpc-transports.route-map.json");
        let rendered = render_outputs(&path).unwrap();
        let expected = read_text(&root.join("generated/rust/src/rpc_transports.rs")).unwrap();
        assert_eq!(rendered[&PathBuf::from("rust/src/rpc_transports.rs")], expected);
    }

    #[test]
    fn committed_generated_routes_are_green() {
        let root = repo_root();
        run_generate_routes(&root, &[], &root.join("generated"), true).unwrap();
    }
}
