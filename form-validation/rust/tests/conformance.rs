use garde::Validate;
use ores_form_validation::{garde_rule, FieldState, FieldValidator, Kind, Rules, MAX_INPUT_BYTES};
use serde::Deserialize;

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
            _ => panic!("unknown fixture kind"),
        };
        FieldValidator::new(Rules { kind, required: self.required, non_blank: self.non_blank,
            min_chars: self.min_chars, max_chars: self.max_chars, min_lines: self.min_lines, max_lines: self.max_lines,
            minimum: self.minimum, maximum: self.maximum, date_min: self.date_min, date_max: self.date_max }).unwrap()
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case { id: String, rules: FixtureRules, value: Option<String>, errors: Vec<String> }

#[test]
fn shared_corpus() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("../../fixtures.json")).unwrap();
    assert!(cases.len() >= 80, "empty or truncated conformance corpus");
    let mut ids = std::collections::HashSet::new();
    for case in cases {
        assert!(ids.insert(case.id.clone()), "duplicate case id");
        let actual: Vec<_> = case.rules.build().validate(case.value.as_deref()).into_iter().map(|c| c.as_str()).collect();
        assert_eq!(actual, case.errors, "fixture {}", case.id);
    }
}

#[test]
fn invalid_configuration_never_becomes_a_permissive_validator() {
    let cases = [
        Rules { min_chars: Some(3), max_chars: Some(2), ..Rules::default() },
        Rules { max_lines: Some(0), ..Rules::default() },
        Rules { min_lines: Some(3), max_lines: Some(2), ..Rules::default() },
        Rules { minimum: Some(0.0), ..Rules::default() },
        Rules { kind: Kind::Number, minimum: Some(f64::NAN), ..Rules::default() },
        Rules { kind: Kind::Number, maximum: Some(f64::INFINITY), ..Rules::default() },
        Rules { kind: Kind::Number, minimum: Some(2.0), maximum: Some(1.0), ..Rules::default() },
        Rules { kind: Kind::Integer, minimum: Some(0.5), ..Rules::default() },
        Rules { kind: Kind::Integer, maximum: Some(9_007_199_254_740_992.0), ..Rules::default() },
        Rules { date_min: Some("2024-01-01".into()), ..Rules::default() },
        Rules { kind: Kind::Date, date_min: Some("2023-02-29".into()), ..Rules::default() },
        Rules { kind: Kind::Date, date_min: Some("2025-01-01".into()), date_max: Some("2024-01-01".into()), ..Rules::default() },
        Rules { min_chars: Some(MAX_INPUT_BYTES + 1), ..Rules::default() },
    ];
    for rules in cases { assert!(FieldValidator::new(rules).is_err()); }
}

#[test]
fn input_budget_and_numeric_overflow() {
    let text = FieldValidator::new(Rules::default()).unwrap();
    assert!(text.validate(Some(&"a".repeat(MAX_INPUT_BYTES))).is_empty());
    assert_eq!(text.validate(Some(&"a".repeat(MAX_INPUT_BYTES + 1)))[0].as_str(), "too_large");
    assert_eq!(text.validate(Some(&"😀".repeat(MAX_INPUT_BYTES / 4 + 1)))[0].as_str(), "too_large");
    let number = FieldValidator::new(Rules { kind: Kind::Number, ..Rules::default() }).unwrap();
    assert_eq!(number.validate(Some(&"9".repeat(400)))[0].as_str(), "number");
}

#[test]
fn serde_and_garde_compose_without_treating_decode_as_validation() {
    #[derive(Deserialize, garde::Validate)]
    #[garde(context(FieldValidator))]
    struct Input { #[garde(custom(garde_rule))] value: String }
    let rules = FieldValidator::new(Rules { kind: Kind::Email, required: true, ..Rules::default() }).unwrap();
    let input: Input = serde_json::from_str(r#"{"value":"not-an-email"}"#).unwrap();
    let report = input.validate_with(&rules).unwrap_err().to_string();
    assert!(report.contains("email"));
    assert!(!report.contains("not-an-email"));
    let good: Input = serde_json::from_str(r#"{"value":"person@example.com"}"#).unwrap();
    assert!(good.validate_with(&rules).is_ok());
}

#[test]
fn edit_blur_submit_revalidates_and_clears_old_errors() {
    let validator = FieldValidator::new(Rules { required: true, ..Rules::default() }).unwrap();
    let mut field = FieldState::default();
    field.edit(&validator, Some(""));
    assert!(field.visible_errors().is_empty());
    field.blur(&validator, Some(""));
    assert_eq!(field.visible_errors()[0].as_str(), "required");
    field.edit(&validator, Some("ok"));
    assert!(field.visible_errors().is_empty());
    assert!(!field.submit(&validator, None));
    assert!(field.submit(&validator, Some("ok")));
}
