//! Representative JSON-value boundaries calling the production validator.
//! Product commands remain owned by each product's interfaces and lib-core.
#![cfg(not(target_arch = "wasm32"))]

use ores_form_validation::{FieldValidator, Kind, Rules};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    value: String,
}

fn validator(profile: &str) -> FieldValidator {
    let rules = match profile {
        "TextSubmission" => Rules {
            required: true,
            min_chars: Some(1),
            max_chars: Some(80),
            ..Rules::default()
        },
        "PhoneSubmission" => Rules {
            kind: Kind::PhoneE164,
            required: true,
            ..Rules::default()
        },
        "IntegerSubmission" => Rules {
            kind: Kind::Integer,
            required: true,
            minimum: Some(0.0),
            maximum: Some(150.0),
            ..Rules::default()
        },
        _ => panic!("unsupported admission profile"),
    };
    FieldValidator::new(rules).expect("reviewed profile configuration")
}

#[test]
fn profile_admission() {
    let corpus: Value = serde_json::from_str(include_str!("../../admission-profiles/corpus.json"))
        .expect("valid committed corpus JSON");
    assert_eq!(corpus["schema"], "ores.form-admission.corpus/v1");
    let cases = corpus["cases"].as_array().expect("corpus cases");
    assert!(!cases.is_empty());
    let results: Vec<Value> = cases
        .iter()
        .map(|row| {
            let profile = row["profile"].as_str().expect("profile name");
            let validation = validator(profile);
            // Schema errors and semantic rejections are explicit. Panics and tool
            // failures never become a successful negative fixture.
            let decoded = serde_json::from_value::<Submission>(row["input"].clone());
            let admitted = decoded
                .ok()
                .filter(|_| row["input"].is_object())
                .filter(|dto| validation.validate(Some(&dto.value)).is_empty());
            let preserved = admitted
                .as_ref()
                .map(|dto| json!({ "value": dto.value }) == row["input"]);
            let accepted = admitted.is_some();
            assert_eq!(
                accepted,
                row["expected"].as_bool().expect("boolean expectation"),
                "profile mismatch for {}",
                row["id"]
            );
            json!({
                "id": row["id"], "profile": profile,
                "accepted": accepted, "preserved": preserved
            })
        })
        .collect();
    println!(
        "\nORES_FORM_ADMISSION={}",
        json!({ "schema": "ores.form-admission.runtime/v1", "results": results })
    );
}
