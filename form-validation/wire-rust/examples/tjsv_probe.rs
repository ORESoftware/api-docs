//! Fixed synthetic conformance entrypoint: stdin JSON, stdout JSON, no CLI flags.
use std::io::{self, Read};
use ores_form_validation::{FieldValidator, Kind, Rules};
use ores_form_validation_wire::ValidationMessage;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input { messages: Vec<MessageCase>, fields: Vec<FieldCase> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageCase { id: String, instance: Value }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldCase { id: String, rules: FixtureRules, value: Option<String> }
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FixtureRules {
    kind: String, required: bool, non_blank: bool,
    min_chars: Option<usize>, max_chars: Option<usize>, min_lines: Option<usize>, max_lines: Option<usize>,
    minimum: Option<f64>, maximum: Option<f64>, date_min: Option<String>, date_max: Option<String>,
}
impl FixtureRules {
    fn build(self) -> FieldValidator {
        let kind = match self.kind.as_str() {
            "" | "text" => Kind::Text, "email" => Kind::Email, "phone_e164" => Kind::PhoneE164,
            "number" => Kind::Number, "integer" => Kind::Integer, "date" => Kind::Date,
            _ => panic!("invalid fixture kind"),
        };
        FieldValidator::new(Rules { kind, required: self.required, non_blank: self.non_blank,
            min_chars: self.min_chars, max_chars: self.max_chars, min_lines: self.min_lines, max_lines: self.max_lines,
            minimum: self.minimum, maximum: self.maximum, date_min: self.date_min, date_max: self.date_max }).expect("valid fixture configuration")
    }
}
fn main() {
    assert_eq!(std::env::args_os().len(), 1, "fixed probe accepts no arguments");
    let mut input = String::new();
    io::stdin().take(1_048_577).read_to_string(&mut input).expect("read synthetic input");
    assert!(input.len() <= 1_048_576, "probe input budget");
    let input: Input = serde_json::from_str(&input).expect("valid probe protocol");
    let messages: Vec<_> = input.messages.into_iter().map(|case| {
        match serde_json::from_value::<ValidationMessage>(case.instance) {
            Ok(message) => json!({"id": case.id, "accepted": true, "value": message}),
            Err(_) => json!({"id": case.id, "accepted": false, "value": null}),
        }
    }).collect();
    let fields: Vec<_> = input.fields.into_iter().map(|case| {
        let codes = case.rules.build().validate(case.value.as_deref());
        let message = ValidationMessage::from_codes("input", &codes).expect("known core errors");
        json!({"id": case.id, "message": message})
    }).collect();
    println!("{}", json!({"messages": messages, "fields": fields}));
}
