//! Derived JSON Schema projection of the catalog.
//!
//! This schema is *evidence*, never a second authored authority.
//! `json-schema/rpc-request-plan.schema.json` stays hand-authored, and the two
//! are reconciled behaviourally: every fixture in the generated corpus must
//! receive the same verdict from both. Structural equality is deliberately not
//! required, because an authored schema may legitimately express a contiguous
//! integer enum as a range.

use super::model::{AppliesTo, Catalog, Option_};
use serde_json::{json, Map, Value};

pub const PLAN_VERSION: &str = "1.0.0";

/// Plan fields the catalog does not own: they identify the call rather than
/// configure it, and are authored directly in the peer schema.
pub const IDENTITY_FIELDS: [&str; 5] = ["plan_version", "kind", "key", "rpc_path", "transport"];

pub fn derive(catalog: &Catalog) -> Value {
    let mut properties = Map::new();

    properties.insert("plan_version".to_owned(), json!({ "const": PLAN_VERSION }));
    properties.insert("kind".to_owned(), json!({ "enum": ["unary", "stream"] }));
    properties.insert(
        "key".to_owned(),
        json!({ "type": "string", "minLength": 1 }),
    );
    properties.insert(
        "rpc_path".to_owned(),
        json!({ "type": "string", "pattern": "^/.*", "default": catalog.rpc_path }),
    );
    properties.insert(
        "transport".to_owned(),
        json!({ "enum": ["http", "tcp", "websocket", "nats"] }),
    );

    for field in plan_fields(catalog) {
        properties.insert(field.name.clone(), field.schema);
    }

    let mut all_of: Vec<Value> = Vec::new();
    all_of.push(surface_exclusion(catalog, AppliesTo::Unary));
    all_of.push(surface_exclusion(catalog, AppliesTo::Stream));
    for constraint in exclusive_group_constraints(catalog) {
        all_of.push(constraint);
    }
    for constraint in prerequisite_constraints(catalog) {
        all_of.push(constraint);
    }

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/oresoftware/api-docs/raw/main/generated/rpc-client-options/plan.derived.schema.json",
        "title": "ores-api-docs RPC request plan (derived from the option catalog)",
        "description": "Generated evidence artifact. The authored peer at json-schema/rpc-request-plan.schema.json is the authority; these two are reconciled by verdict agreement over the generated fixture corpus.",
        "x-catalog-version": catalog.catalog_version,
        "type": "object",
        "additionalProperties": false,
        "required": ["plan_version", "kind", "key", "rpc_path", "serial_strategy"],
        "properties": Value::Object(properties),
        "allOf": all_of
    })
}

pub struct PlanField {
    pub name: String,
    pub schema: Value,
    pub applies_to: AppliesTo,
}

/// Every plan field the catalog can write, in canonical (sorted) order.
pub fn plan_fields(catalog: &Catalog) -> Vec<PlanField> {
    let mut fields: Vec<PlanField> = Vec::new();

    for option in &catalog.options {
        let Some(name) = option.plan_field.as_deref() else {
            continue;
        };
        if let Some(existing) = fields.iter_mut().find(|f| f.name == name) {
            merge_into(existing, option, catalog);
            continue;
        }
        fields.push(PlanField {
            name: name.to_owned(),
            schema: field_schema(option, catalog),
            applies_to: option.applies_to,
        });
    }

    // Fold in the "nothing was selected" value for exclusive groups that have one.
    for group in &catalog.exclusive_groups {
        let Some(implicit) = group.implicit_plan_value_ref() else {
            continue;
        };
        let member_field = catalog
            .options
            .iter()
            .find(|o| o.exclusive_group.as_deref() == Some(group.exclusive_group_id.as_str()))
            .and_then(|o| o.plan_field.clone());
        let Some(member_field) = member_field else {
            continue;
        };
        if let Some(field) = fields.iter_mut().find(|f| f.name == member_field) {
            push_enum_value(&mut field.schema, implicit.clone());
        }
    }

    fields.sort_by(|a, b| a.name.cmp(&b.name));
    fields
}

fn merge_into(field: &mut PlanField, option: &Option_, catalog: &Catalog) {
    // Two options writing one field widen both the surface and the value set.
    if field.applies_to != option.applies_to {
        field.applies_to = AppliesTo::Both;
    }
    if let Some(value) = &option.plan_value {
        push_enum_value(&mut field.schema, value.clone());
    } else {
        // A parameterised writer subsumes constant writers.
        let widened = field_schema(option, catalog);
        if widened.get("enum").is_none() {
            let existing = field.schema.get("enum").cloned();
            field.schema = widened;
            if let Some(Value::Array(values)) = existing {
                for value in values {
                    push_enum_value(&mut field.schema, value);
                }
            }
        }
    }
}

fn push_enum_value(schema: &mut Value, value: Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let entry = object
        .entry("enum".to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(values) = entry.as_array_mut() {
        if !values.contains(&value) {
            values.push(value);
            values.sort_by_key(|v| v.to_string());
        }
    }
    object.remove("type");
}

fn field_schema(option: &Option_, catalog: &Catalog) -> Value {
    // A constant-writing option (no params) contributes a single value.
    if option.params.is_empty() {
        return match &option.plan_value {
            Some(Value::Bool(_)) => json!({ "type": "boolean" }),
            Some(other) => json!({ "enum": [other] }),
            None => json!(true),
        };
    }

    // Callback options serialize as a registration count.
    if option.params.iter().any(|p| p.ty == "callback") {
        return json!({ "type": "integer", "minimum": 0 });
    }

    if option.params.len() > 1 {
        let mut properties = Map::new();
        let mut required: Vec<String> = Vec::new();
        for param in &option.params {
            properties.insert(param.name.clone(), scalar_schema(param, catalog));
            required.push(param.name.clone());
        }
        required.sort();
        return json!({
            "type": "object",
            "additionalProperties": false,
            "required": required,
            "properties": Value::Object(properties)
        });
    }

    scalar_schema(&option.params[0], catalog)
}

fn scalar_schema(param: &super::model::Param, catalog: &Catalog) -> Value {
    match param.ty.as_str() {
        "enum" => {
            let variants = param
                .enum_id
                .as_deref()
                .and_then(|id| catalog.enum_by_id(id))
                .map(|e| {
                    e.variants
                        .iter()
                        .map(|v| v.wire_value.clone())
                        .collect::<Vec<Value>>()
                })
                .unwrap_or_default();
            json!({ "enum": variants })
        }
        "bool" => json!({ "type": "boolean" }),
        "u8" | "u32" | "i64" => {
            let mut object = Map::new();
            object.insert("type".to_owned(), json!("integer"));
            if let Some(minimum) = param.minimum {
                object.insert("minimum".to_owned(), json!(minimum as i64));
            }
            if let Some(maximum) = param.maximum {
                object.insert("maximum".to_owned(), json!(maximum as i64));
            }
            Value::Object(object)
        }
        "f64" => {
            let mut object = Map::new();
            object.insert("type".to_owned(), json!("number"));
            if let Some(minimum) = param.minimum {
                object.insert("minimum".to_owned(), json!(minimum));
            }
            if let Some(maximum) = param.maximum {
                object.insert("maximum".to_owned(), json!(maximum));
            }
            Value::Object(object)
        }
        "string" | "secret_string" => {
            let mut object = Map::new();
            object.insert("type".to_owned(), json!("string"));
            object.insert("minLength".to_owned(), json!(1));
            if let Some(max_length) = param.max_length {
                object.insert("maxLength".to_owned(), json!(max_length));
            }
            Value::Object(object)
        }
        "url" => json!({ "type": "string", "format": "uri" }),
        "json_object" => json!({ "type": "object" }),
        _ => json!(true),
    }
}

/// A plan of one surface must not carry the other surface's exclusive options.
fn surface_exclusion(catalog: &Catalog, surface: AppliesTo) -> Value {
    let excluded = match surface {
        AppliesTo::Unary => AppliesTo::Stream,
        AppliesTo::Stream => AppliesTo::Unary,
        AppliesTo::Both => AppliesTo::Both,
    };
    let mut names: Vec<String> = catalog
        .options
        .iter()
        .filter(|o| o.applies_to == excluded && !o.terminal)
        .filter_map(|o| o.plan_field.clone())
        .collect();
    names.sort();
    names.dedup();

    let any_of: Vec<Value> = names
        .into_iter()
        .map(|name| json!({ "required": [name] }))
        .collect();

    json!({
        "title": format!("{} plans reject {} options", surface.as_str(), excluded.as_str()),
        "if": { "properties": { "kind": { "const": surface.as_str() } }, "required": ["kind"] },
        "then": { "not": { "anyOf": any_of } }
    })
}

/// Exclusive groups whose members write *distinct* plan fields cannot be
/// expressed as a single enum, so they need an explicit pairwise exclusion.
fn exclusive_group_constraints(catalog: &Catalog) -> Vec<Value> {
    let mut constraints = Vec::new();
    for group in &catalog.exclusive_groups {
        let mut fields: Vec<String> = catalog
            .options
            .iter()
            .filter(|o| o.exclusive_group.as_deref() == Some(group.exclusive_group_id.as_str()))
            .filter_map(|o| o.plan_field.clone())
            .collect();
        fields.sort();
        fields.dedup();
        if fields.len() < 2 {
            // Members share one plan field; the enum already makes them exclusive.
            continue;
        }
        let mut pairs: Vec<Value> = Vec::new();
        for (index, left) in fields.iter().enumerate() {
            for right in &fields[index + 1..] {
                pairs.push(json!({ "not": { "required": [left, right] } }));
            }
        }
        constraints.push(json!({
            "title": format!("{} members are mutually exclusive", group.exclusive_group_id),
            "allOf": pairs
        }));
    }
    constraints
}

fn prerequisite_constraints(catalog: &Catalog) -> Vec<Value> {
    let mut constraints = Vec::new();
    for option in &catalog.options {
        if option.requires_options.is_empty() {
            continue;
        }
        let Some(field) = option.plan_field.as_deref() else {
            continue;
        };
        let mut required: Vec<String> = option
            .requires_options
            .iter()
            .filter_map(|id| {
                catalog
                    .options
                    .iter()
                    .find(|o| &o.option_id == id)
                    .and_then(|o| o.plan_field.clone())
            })
            .collect();
        required.sort();
        constraints.push(json!({
            "title": format!("{} requires {}", option.option_id, option.requires_options.join(", ")),
            "if": { "required": [field] },
            "then": { "required": required }
        }));
    }
    constraints
}
