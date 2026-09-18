//! Deterministic positive/negative request-plan corpus derived from the catalog.
//!
//! The corpus is the correctness evidence for the generated documentation: if
//! the docs claim an option is stream-only, the corpus contains a plan that
//! places it on a unary plan and asserts the schema rejects it. Sample values
//! are derived from the declared bounds, never invented, so the corpus is a
//! pure function of the catalog.

use super::emit_schema::PLAN_VERSION;
use super::model::{AppliesTo, Catalog, Option_, Param};
use serde_json::{json, Map, Value};

pub const UNARY_KEY: &str = "demo.users.find_user";
pub const STREAM_KEY: &str = "demo.events.watch_events";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Fixture {
    /// Stable identifier, unique across the corpus.
    pub fixture_id: String,
    /// What this fixture proves about the documented surface.
    pub rationale: String,
    /// Expected verdict from the authored plan schema.
    pub valid: bool,
    pub plan: Value,
}

pub fn corpus(catalog: &Catalog) -> Vec<Fixture> {
    let mut fixtures: Vec<Fixture> = Vec::new();

    fixtures.push(Fixture {
        fixture_id: "baseline.unary.minimal".to_owned(),
        rationale: "A unary plan with only identity fields and the default strategy is valid."
            .to_owned(),
        valid: true,
        plan: base_plan(catalog, AppliesTo::Unary),
    });
    fixtures.push(Fixture {
        fixture_id: "baseline.stream.minimal".to_owned(),
        rationale: "A streaming plan with only identity fields is valid.".to_owned(),
        valid: true,
        plan: base_plan(catalog, AppliesTo::Stream),
    });

    positive_per_option(catalog, &mut fixtures);
    negative_surface_crossing(catalog, &mut fixtures);
    negative_exclusive_groups(catalog, &mut fixtures);
    negative_bounds(catalog, &mut fixtures);
    negative_structural(catalog, &mut fixtures);

    fixtures.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id));
    fixtures
}

fn base_plan(catalog: &Catalog, surface: AppliesTo) -> Value {
    let (kind, key) = match surface {
        AppliesTo::Stream => ("stream", STREAM_KEY),
        _ => ("unary", UNARY_KEY),
    };
    json!({
        "plan_version": PLAN_VERSION,
        "kind": kind,
        "key": key,
        "rpc_path": catalog.rpc_path,
        "serial_strategy": "json"
    })
}

/// One valid plan per option, on each surface the option is documented for.
fn positive_per_option(catalog: &Catalog, fixtures: &mut Vec<Fixture>) {
    for option in &catalog.options {
        let Some(field) = option.plan_field.as_deref() else {
            continue;
        };
        let surfaces: &[AppliesTo] = match option.applies_to {
            AppliesTo::Unary => &[AppliesTo::Unary],
            AppliesTo::Stream => &[AppliesTo::Stream],
            AppliesTo::Both => &[AppliesTo::Unary, AppliesTo::Stream],
        };
        for surface in surfaces {
            let mut plan = base_plan(catalog, *surface);
            apply(&mut plan, catalog, option, field);
            for prerequisite in &option.requires_options {
                if let Some(other) = catalog
                    .options
                    .iter()
                    .find(|o| &o.option_id == prerequisite)
                {
                    if let Some(other_field) = other.plan_field.as_deref() {
                        apply(&mut plan, catalog, other, other_field);
                    }
                }
            }
            fixtures.push(Fixture {
                fixture_id: format!("positive.{}.{}", surface.as_str(), option.option_id),
                rationale: format!(
                    "`{}` is documented for the {} surface and must be accepted there.",
                    option.option_id,
                    surface.as_str()
                ),
                valid: true,
                plan,
            });
        }
    }
}

/// Every surface-exclusive option, placed on the surface it is not documented
/// for. These are the fixtures that make the make_call/stream split falsifiable.
fn negative_surface_crossing(catalog: &Catalog, fixtures: &mut Vec<Fixture>) {
    for option in &catalog.options {
        if option.terminal || option.applies_to == AppliesTo::Both {
            continue;
        }
        let Some(field) = option.plan_field.as_deref() else {
            continue;
        };
        let wrong = match option.applies_to {
            AppliesTo::Unary => AppliesTo::Stream,
            _ => AppliesTo::Unary,
        };
        let mut plan = base_plan(catalog, wrong);
        apply(&mut plan, catalog, option, field);
        fixtures.push(Fixture {
            fixture_id: format!(
                "negative.surface.{}.on_{}",
                option.option_id,
                wrong.as_str()
            ),
            rationale: format!(
                "`{}` is {}-only, so a {} plan carrying it must be rejected.",
                option.option_id,
                option.applies_to.as_str(),
                wrong.as_str()
            ),
            valid: false,
            plan,
        });
    }
}

/// Two members of one exclusive group applied together. In a typed client this
/// is unreachable; the schema is the backstop for untyped producers.
fn negative_exclusive_groups(catalog: &Catalog, fixtures: &mut Vec<Fixture>) {
    for group in &catalog.exclusive_groups {
        let members: Vec<&Option_> = catalog
            .options
            .iter()
            .filter(|o| o.exclusive_group.as_deref() == Some(group.exclusive_group_id.as_str()))
            .collect();
        for (index, left) in members.iter().enumerate() {
            for right in &members[index + 1..] {
                let (Some(left_field), Some(right_field)) =
                    (left.plan_field.as_deref(), right.plan_field.as_deref())
                else {
                    continue;
                };
                if left_field == right_field {
                    // One field cannot hold two values; the enum already covers it.
                    continue;
                }
                let surface = if left.applies_to == AppliesTo::Stream
                    || right.applies_to == AppliesTo::Stream
                {
                    AppliesTo::Stream
                } else {
                    AppliesTo::Unary
                };
                let mut plan = base_plan(catalog, surface);
                apply(&mut plan, catalog, left, left_field);
                apply(&mut plan, catalog, right, right_field);
                fixtures.push(Fixture {
                    fixture_id: format!(
                        "negative.exclusive.{}.{}_with_{}",
                        group.exclusive_group_id, left.option_id, right.option_id
                    ),
                    rationale: format!(
                        "`{}` and `{}` are contradictory members of `{}`.",
                        left.option_id, right.option_id, group.exclusive_group_id
                    ),
                    valid: false,
                    plan,
                });
            }
        }
    }
}

/// Numeric options pushed one step outside their documented bounds.
fn negative_bounds(catalog: &Catalog, fixtures: &mut Vec<Fixture>) {
    for option in &catalog.options {
        let Some(field) = option.plan_field.as_deref() else {
            continue;
        };
        if option.params.len() != 1 {
            continue;
        }
        let param = &option.params[0];
        if !matches!(param.ty.as_str(), "u8" | "u32" | "i64") {
            continue;
        }
        let surface = match option.applies_to {
            AppliesTo::Stream => AppliesTo::Stream,
            _ => AppliesTo::Unary,
        };
        for (edge, value) in [
            ("below_minimum", param.minimum.map(|m| m as i64 - 1)),
            ("above_maximum", param.maximum.map(|m| m as i64 + 1)),
        ] {
            let Some(value) = value else { continue };
            let mut plan = base_plan(catalog, surface);
            plan[field] = json!(value);
            for prerequisite in &option.requires_options {
                if let Some(other) = catalog
                    .options
                    .iter()
                    .find(|o| &o.option_id == prerequisite)
                {
                    if let Some(other_field) = other.plan_field.as_deref() {
                        apply(&mut plan, catalog, other, other_field);
                    }
                }
            }
            fixtures.push(Fixture {
                fixture_id: format!("negative.bounds.{}.{edge}", option.option_id),
                rationale: format!(
                    "`{}` documents {} as {edge:?}-bounded; {value} is outside the documented range.",
                    option.option_id, param.name
                ),
                valid: false,
                plan,
            });
        }
    }
}

fn negative_structural(catalog: &Catalog, fixtures: &mut Vec<Fixture>) {
    let mut unknown = base_plan(catalog, AppliesTo::Unary);
    unknown["not_a_documented_option"] = json!(true);
    fixtures.push(Fixture {
        fixture_id: "negative.structural.unknown_field".to_owned(),
        rationale: "A plan field absent from the documentation must be rejected, so the docs \
                    cannot silently omit part of the surface."
            .to_owned(),
        valid: false,
        plan: unknown,
    });

    let mut missing = base_plan(catalog, AppliesTo::Unary);
    missing
        .as_object_mut()
        .expect("base plan is an object")
        .remove("serial_strategy");
    fixtures.push(Fixture {
        fixture_id: "negative.structural.missing_serial_strategy".to_owned(),
        rationale:
            "Every plan states its serialization strategy explicitly, including the default."
                .to_owned(),
        valid: false,
        plan: missing,
    });

    let mut bad_kind = base_plan(catalog, AppliesTo::Unary);
    bad_kind["kind"] = json!("duplex");
    fixtures.push(Fixture {
        fixture_id: "negative.structural.unknown_kind".to_owned(),
        rationale: "There are exactly two client surfaces.".to_owned(),
        valid: false,
        plan: bad_kind,
    });

    let mut backoff_without_budget = base_plan(catalog, AppliesTo::Unary);
    backoff_without_budget["retry_backoff"] = json!({ "base_millis": 100, "factor": 2.0 });
    fixtures.push(Fixture {
        fixture_id: "negative.structural.backoff_without_retry_budget".to_owned(),
        rationale: "A backoff schedule without a retry budget is inert and is refused.".to_owned(),
        valid: false,
        plan: backoff_without_budget,
    });
}

/// Write an option's documented plan value using only declared information.
fn apply(plan: &mut Value, catalog: &Catalog, option: &Option_, field: &str) {
    let value = if let Some(constant) = &option.plan_value {
        constant.clone()
    } else if option.params.iter().any(|p| p.ty == "callback") {
        json!(1)
    } else if option.params.len() > 1 {
        let mut object = Map::new();
        for param in &option.params {
            object.insert(param.name.clone(), sample(param, catalog));
        }
        Value::Object(object)
    } else if let Some(param) = option.params.first() {
        sample(param, catalog)
    } else {
        json!(true)
    };
    plan[field] = value;
}

/// Sample values come from declared bounds, so they move when the bounds move.
fn sample(param: &Param, catalog: &Catalog) -> Value {
    match param.ty.as_str() {
        "enum" => param
            .enum_id
            .as_deref()
            .and_then(|id| catalog.enum_by_id(id))
            .and_then(|e| e.variants.first())
            .map(|v| v.wire_value.clone())
            .unwrap_or(Value::Null),
        "bool" => json!(true),
        "u8" | "u32" | "i64" => json!(param.minimum.map_or(1, |m| m as i64)),
        "f64" => json!(param.minimum.unwrap_or(1.0)),
        "url" => json!("http://127.0.0.1:8080"),
        "json_object" => json!({}),
        "secret_string" | "string" => json!("x"),
        _ => json!("x"),
    }
}
