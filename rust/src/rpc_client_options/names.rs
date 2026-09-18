//! Deterministic per-language identifier derivation.
//!
//! `option_id` in the catalog is snake_case and is the only authored spelling.
//! Every language surface is a pure function of it, so a client cannot quietly
//! rename a method away from the contract.

/// Languages that carry a generated RPC client surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    Rust,
    TypeScript,
    Dart,
    Go,
    Gleam,
}

impl Language {
    /// Canonical order used by every generated artifact.
    pub const ALL: [Self; 5] = [
        Self::Rust,
        Self::TypeScript,
        Self::Dart,
        Self::Go,
        Self::Gleam,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Dart => "dart",
            Self::Go => "go",
            Self::Gleam => "gleam",
        }
    }

    /// Derive the method spelling this language uses for a snake_case id.
    pub fn method_name(self, option_id: &str) -> String {
        match self {
            // Rust and Gleam keep snake_case verbatim.
            Self::Rust | Self::Gleam => option_id.to_owned(),
            Self::TypeScript | Self::Dart => lower_camel_case(option_id),
            Self::Go => upper_camel_case(option_id),
        }
    }

    /// Derive the spelling this language uses for an enum variant.
    pub fn variant_name(self, variant_id: &str) -> String {
        match self {
            Self::Rust => upper_camel_case(variant_id),
            Self::Gleam => upper_camel_case(variant_id),
            Self::TypeScript | Self::Dart => lower_camel_case(variant_id),
            Self::Go => upper_camel_case(variant_id),
        }
    }
}

fn lower_camel_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut upper_next = false;
    for ch in value.chars() {
        if ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next && !out.is_empty() {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
            upper_next = false;
        }
    }
    out
}

fn upper_camel_case(value: &str) -> String {
    let camel = lower_camel_case(value);
    let mut chars = camel.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_names_are_derived_per_language() {
        assert_eq!(Language::Rust.method_name("make_call"), "make_call");
        assert_eq!(Language::Gleam.method_name("make_call"), "make_call");
        assert_eq!(Language::TypeScript.method_name("make_call"), "makeCall");
        assert_eq!(Language::Dart.method_name("with_timeout"), "withTimeout");
        assert_eq!(Language::Go.method_name("with_timeout"), "WithTimeout");
        assert_eq!(Language::Go.method_name("stream"), "Stream");
    }

    #[test]
    fn multi_word_ids_round_trip_without_losing_segments() {
        assert_eq!(
            Language::TypeScript.method_name("skip_cloudflare_cache"),
            "skipCloudflareCache"
        );
        assert_eq!(
            Language::Go.method_name("stale_while_revalidate"),
            "StaleWhileRevalidate"
        );
        assert_eq!(Language::TypeScript.method_name("use_ipv4"), "useIpv4");
    }

    #[test]
    fn variant_names_follow_language_convention() {
        assert_eq!(Language::Rust.variant_name("message_pack"), "MessagePack");
        assert_eq!(
            Language::TypeScript.variant_name("message_pack"),
            "messagePack"
        );
        assert_eq!(Language::Go.variant_name("drop_oldest"), "DropOldest");
    }

    #[test]
    fn derivation_is_total_and_injective_over_the_catalog() {
        // Two distinct option ids must never collide in any language, or a
        // generated client would silently lose a method.
        let ids = [
            "use_json",
            "use_message_pack",
            "with_timeout",
            "with_trace_id",
            "throttle",
            "throttle_each",
        ];
        for language in Language::ALL {
            let mut names: Vec<String> =
                ids.iter().map(|id| language.method_name(id)).collect();
            names.sort();
            let before = names.len();
            names.dedup();
            assert_eq!(before, names.len(), "collision in {}", language.as_str());
        }
    }
}
