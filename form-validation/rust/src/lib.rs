//! Shared form primitives, not an authorization or synchronization engine.
//! Product rules belong in `*-lib-core`; see the sibling README for semantics.
use chrono::NaiveDate;
use garde::Validate;

pub const MAX_INPUT_BYTES: usize = 65_536;
pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    Email,
    PhoneE164,
    Number,
    Integer,
    Date,
}

/// Developer-owned configuration. This is not a remotely executable schema.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rules {
    pub kind: Kind,
    pub required: bool,
    pub non_blank: bool,
    pub min_chars: Option<usize>,
    pub max_chars: Option<usize>,
    pub min_lines: Option<usize>,
    pub max_lines: Option<usize>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub date_min: Option<String>,
    pub date_max: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    TooLarge,
    Required,
    Blank,
    MinChars,
    MaxChars,
    MinLines,
    MaxLines,
    Email,
    PhoneE164,
    Number,
    Integer,
    UnsafeInteger,
    Minimum,
    Maximum,
    Date,
    DateMin,
    DateMax,
}

impl Code {
    /// Stable localization keys. No submitted value is retained in an error.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::Required => "required",
            Self::Blank => "blank",
            Self::MinChars => "min_chars",
            Self::MaxChars => "max_chars",
            Self::MinLines => "min_lines",
            Self::MaxLines => "max_lines",
            Self::Email => "email",
            Self::PhoneE164 => "phone_e164",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::UnsafeInteger => "unsafe_integer",
            Self::Minimum => "minimum",
            Self::Maximum => "maximum",
            Self::Date => "date",
            Self::DateMin => "date_min",
            Self::DateMax => "date_max",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidRules;
impl std::fmt::Display for InvalidRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid form validation rules")
    }
}
impl std::error::Error for InvalidRules {}

/// Validated immutable configuration, safe to share between server and UI.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldValidator {
    rules: Rules,
    date_min: Option<NaiveDate>,
    date_max: Option<NaiveDate>,
}

impl FieldValidator {
    pub fn new(rules: Rules) -> Result<Self, InvalidRules> {
        let ordered = |a: Option<usize>, b: Option<usize>| match (a, b) {
            (Some(a), Some(b)) => a <= b,
            _ => true,
        };
        if !ordered(rules.min_chars, rules.max_chars)
            || !ordered(rules.min_lines, rules.max_lines)
            || [rules.min_chars, rules.max_chars]
                .into_iter()
                .flatten()
                .any(|n| n > MAX_INPUT_BYTES)
            || [rules.min_lines, rules.max_lines]
                .into_iter()
                .flatten()
                .any(|n| n == 0 || n > MAX_INPUT_BYTES + 1)
        {
            return Err(InvalidRules);
        }
        let numeric = matches!(rules.kind, Kind::Number | Kind::Integer);
        if (!numeric && (rules.minimum.is_some() || rules.maximum.is_some()))
            || [rules.minimum, rules.maximum]
                .into_iter()
                .flatten()
                .any(|n| !n.is_finite())
            || matches!((rules.minimum, rules.maximum), (Some(a), Some(b)) if a > b)
        {
            return Err(InvalidRules);
        }
        if rules.kind == Kind::Integer
            && [rules.minimum, rules.maximum]
                .into_iter()
                .flatten()
                .any(|n| n.fract() != 0.0 || n.abs() > MAX_SAFE_INTEGER)
        {
            return Err(InvalidRules);
        }
        if rules.kind != Kind::Date && (rules.date_min.is_some() || rules.date_max.is_some()) {
            return Err(InvalidRules);
        }
        let date_min = rules
            .date_min
            .as_deref()
            .map(parse_date)
            .transpose_option()?;
        let date_max = rules
            .date_max
            .as_deref()
            .map(parse_date)
            .transpose_option()?;
        if matches!((date_min, date_max), (Some(a), Some(b)) if a > b) {
            return Err(InvalidRules);
        }
        Ok(Self {
            rules,
            date_min,
            date_max,
        })
    }

    /// Null and the empty string are absent. Whitespace is never silently trimmed.
    /// All returned codes have deterministic order, independent of UI framework.
    pub fn validate(&self, value: Option<&str>) -> Vec<Code> {
        let Some(value) = value.filter(|s| !s.is_empty()) else {
            return if self.rules.required {
                vec![Code::Required]
            } else {
                vec![]
            };
        };
        if value.len() > MAX_INPUT_BYTES {
            return vec![Code::TooLarge];
        }
        let mut errors = Vec::new();
        if self.rules.non_blank && value.chars().all(is_white_space) {
            errors.push(Code::Blank);
        }
        let chars = value.chars().count();
        if self.rules.min_chars.is_some_and(|n| chars < n) {
            errors.push(Code::MinChars);
        }
        if self.rules.max_chars.is_some_and(|n| chars > n) {
            errors.push(Code::MaxChars);
        }
        let lines = logical_lines(value);
        if self.rules.min_lines.is_some_and(|n| lines < n) {
            errors.push(Code::MinLines);
        }
        if self.rules.max_lines.is_some_and(|n| lines > n) {
            errors.push(Code::MaxLines);
        }
        match self.rules.kind {
            Kind::Text => {}
            Kind::Email => {
                if !email(value) {
                    errors.push(Code::Email);
                }
            }
            Kind::PhoneE164 => {
                if !phone_e164(value) {
                    errors.push(Code::PhoneE164);
                }
            }
            Kind::Number | Kind::Integer => {
                let integer = self.rules.kind == Kind::Integer;
                let number = decimal(value, integer);
                match number {
                    None => errors.push(if integer { Code::Integer } else { Code::Number }),
                    Some(n) if integer && n.abs() > MAX_SAFE_INTEGER => {
                        errors.push(Code::UnsafeInteger)
                    }
                    Some(n) => {
                        if self.rules.minimum.is_some_and(|min| n < min) {
                            errors.push(Code::Minimum);
                        }
                        if self.rules.maximum.is_some_and(|max| n > max) {
                            errors.push(Code::Maximum);
                        }
                    }
                }
            }
            Kind::Date => match parse_date(value) {
                None => errors.push(Code::Date),
                Some(date) => {
                    if self.date_min.is_some_and(|min| date < min) {
                        errors.push(Code::DateMin);
                    }
                    if self.date_max.is_some_and(|max| date > max) {
                        errors.push(Code::DateMax);
                    }
                }
            },
        }
        errors
    }
}

// Convert optional bounds without making absent and malformed equivalent.
trait TransposeOption<T> {
    fn transpose_option(self) -> Result<Option<T>, InvalidRules>;
}
impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Result<Option<T>, InvalidRules> {
        match self {
            None => Ok(None),
            Some(Some(value)) => Ok(Some(value)),
            Some(None) => Err(InvalidRules),
        }
    }
}

fn is_white_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// CRLF is one break; bare CR/LF, NEL, LS and PS are also breaks.
/// A trailing break introduces an empty final line. Visual wrapping is irrelevant.
pub fn logical_lines(value: &str) -> usize {
    let mut count = 1;
    let mut previous_cr = false;
    for c in value.chars() {
        if matches!(c, '\r' | '\n' | '\u{0085}' | '\u{2028}' | '\u{2029}')
            && !(c == '\n' && previous_cr)
        {
            count += 1;
        }
        previous_cr = c == '\r';
    }
    count
}

#[derive(garde::Validate)]
struct HtmlEmail<'a> {
    #[garde(email)]
    value: &'a str,
}
fn email(value: &str) -> bool {
    if !value.is_ascii() || value.len() > 254 {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && domain.contains('.')
        && HtmlEmail { value }.validate().is_ok()
}
fn phone_e164(value: &str) -> bool {
    let b = value.as_bytes();
    (3..=16).contains(&b.len())
        && b[0] == b'+'
        && matches!(b[1], b'1'..=b'9')
        && b[2..].iter().all(u8::is_ascii_digit)
}
fn decimal(value: &str, integer: bool) -> Option<f64> {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(a, b)| (a, Some(b)));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
        || (integer && fraction.is_some())
        || fraction.is_some_and(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    value.parse::<f64>().ok().filter(|n| n.is_finite())
}
fn parse_date(value: &str) -> Option<NaiveDate> {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || b.iter()
            .enumerate()
            .any(|(i, c)| i != 4 && i != 7 && !c.is_ascii_digit())
    {
        return None;
    }
    let year = value[..4].parse::<i32>().ok()?;
    if year == 0 {
        return None;
    }
    NaiveDate::from_ymd_opt(year, value[5..7].parse().ok()?, value[8..].parse().ok()?)
}

/// Use with `#[garde(context(FieldValidator))]` and `#[garde(custom(garde_rule))]`.
/// Deliberately does not serialize Garde's internal report or user input.
pub fn garde_rule(value: &str, validator: &FieldValidator) -> garde::Result {
    match validator.validate(Some(value)).first() {
        None => Ok(()),
        Some(code) => Err(garde::Error::new(code.as_str())),
    }
}

/// Framework-neutral edit/blur/submit behavior for Rust native, Leptos and Dioxus.
/// Values stay in the application; this state retains only validation codes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldState {
    touched: bool,
    submitted: bool,
    errors: Vec<Code>,
}
impl FieldState {
    pub fn edit(&mut self, validator: &FieldValidator, value: Option<&str>) {
        self.errors = validator.validate(value);
    }
    pub fn blur(&mut self, validator: &FieldValidator, value: Option<&str>) {
        self.touched = true;
        self.edit(validator, value);
    }
    pub fn submit(&mut self, validator: &FieldValidator, value: Option<&str>) -> bool {
        self.submitted = true;
        self.edit(validator, value);
        self.errors.is_empty()
    }
    pub fn visible_errors(&self) -> &[Code] {
        if self.touched || self.submitted {
            &self.errors
        } else {
            &[]
        }
    }
}
