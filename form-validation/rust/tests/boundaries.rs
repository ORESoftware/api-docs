use ores_form_validation::{Code, FieldState, FieldValidator, Kind, Rules, MAX_INPUT_BYTES};

fn validator(kind: Kind) -> FieldValidator {
    FieldValidator::new(Rules { kind, required: true, ..Rules::default() }).unwrap()
}

#[test]
fn email_mailbox_and_label_limits() {
    let email = validator(Kind::Email);
    assert!(email.validate(Some(&format!("{}@example.com", "a".repeat(64)))).is_empty());
    assert_eq!(email.validate(Some(&format!("{}@example.com", "a".repeat(65)))), [Code::Email]);
    let domain = format!("{}.{}.{}", "b".repeat(63), "c".repeat(63), "d".repeat(61));
    let largest = format!("{}@{domain}", "a".repeat(64));
    assert_eq!(largest.len(), 254);
    assert!(email.validate(Some(&largest)).is_empty());
    assert_eq!(email.validate(Some(&(largest + "d"))), [Code::Email]);
    for domain in [format!("{}.com", "x".repeat(64)), "a_b.com".into(), "example..com".into(), "example.com.".into()] {
        assert_eq!(email.validate(Some(&format!("a@{domain}"))), [Code::Email]);
    }
}

#[test]
fn separators_and_scalar_budgets_are_exact() {
    let two = FieldValidator::new(Rules { min_lines: Some(2), max_lines: Some(2), required: true, ..Rules::default() }).unwrap();
    for text in ["a\r\nb", "a\nb", "a\rb", "a\u{85}b", "a\u{2028}b", "a\u{2029}b"] {
        assert!(two.validate(Some(text)).is_empty());
    }
    assert_eq!(two.validate(Some("a\r\r\nb")), [Code::MaxLines]);
    let four = FieldValidator::new(Rules { max_chars: Some(4), ..Rules::default() }).unwrap();
    assert!(four.validate(Some("😀😀😀😀")).is_empty());
    assert_eq!(four.validate(Some("😀😀😀😀😀")), [Code::MaxChars]);
    let text = validator(Kind::Text);
    assert!(text.validate(Some(&"😀".repeat(MAX_INPUT_BYTES / 4))).is_empty());
    assert_eq!(text.validate(Some(&"😀".repeat(MAX_INPUT_BYTES / 4 + 1))), [Code::TooLarge]);
}

#[test]
fn calendar_century_and_integer_boundaries() {
    let date = validator(Kind::Date);
    for year in [1600, 2000, 2400] {
        assert!(date.validate(Some(&format!("{year}-02-29"))).is_empty());
    }
    for year in [1700, 1800, 1900, 2100] {
        assert_eq!(date.validate(Some(&format!("{year}-02-29"))), [Code::Date]);
    }
    let integer = validator(Kind::Integer);
    for value in ["-9007199254740991", "9007199254740991", "0", "-0"] {
        assert!(integer.validate(Some(value)).is_empty());
    }
    for value in ["9007199254740992", "9007199254740993", "-9007199254740993"] {
        assert_eq!(integer.validate(Some(value)), [Code::UnsafeInteger]);
    }
}

#[test]
fn submission_revalidates_values_changed_without_an_edit_event() {
    let email = validator(Kind::Email);
    let mut field = FieldState::default();
    field.edit(&email, Some("person@example.com"));
    // A stale UI-valid state must not authorize autofill/sync/programmatic changes.
    assert!(!field.submit(&email, Some("not-an-email")));
    assert_eq!(field.visible_errors(), [Code::Email]);
    assert!(!field.submit(&email, None));
    assert_eq!(field.visible_errors(), [Code::Required]);
    assert!(field.submit(&email, Some("person@example.com")));
    assert!(field.visible_errors().is_empty());
    assert!(!format!("{field:?}").contains("person@example.com"));
}
