//! Escaped views of shared validation errors; never another validation engine.
//! Parents own input controls, event handlers and server admission checks.
use ores_form_validation::Code;

/// Contains only a developer-owned field id and localized validation messages.
/// Do not pass user data to localization. Input values are deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldErrors {
    id: String,
    messages: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFieldId;

impl FieldErrors {
    /// `codes` normally comes from `FieldState::visible_errors()` in a Rust UI,
    /// or from the core validator after an Axum form submission.
    pub fn new(
        field_id: &str,
        codes: &[Code],
        localize: impl Fn(Code) -> String,
    ) -> Result<Self, InvalidFieldId> {
        if field_id.is_empty()
            || field_id.len() > 128
            || !field_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        {
            return Err(InvalidFieldId);
        }
        let messages = codes
            .iter()
            .map(|&code| {
                let text = localize(code);
                if text.is_empty() {
                    code.as_str().to_owned()
                } else {
                    text
                }
            })
            .collect();
        Ok(Self {
            id: format!("{field_id}-errors"),
            messages,
        })
    }

    /// Put this on the input's `aria-describedby` attribute.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Put this on the input's `aria-invalid` attribute, not its disabled state.
    pub fn aria_invalid(&self) -> &'static str {
        if self.messages.is_empty() {
            "false"
        } else {
            "true"
        }
    }
}

#[cfg(feature = "mash")]
pub fn maud_errors(errors: &FieldErrors) -> maud::Markup {
    maud::html! {
        div id=(errors.id) role="status" aria-live="polite" {
            @for message in &errors.messages {
                p { (message) }
            }
        }
    }
}

#[cfg(feature = "leptos")]
pub fn leptos_errors(errors: FieldErrors) -> impl leptos::IntoView {
    use leptos::prelude::*;
    leptos::view! {
        <div id=errors.id role="status" aria-live="polite">
            {errors.messages.into_iter().map(|message| leptos::view! { <p>{message}</p> }).collect_view()}
        </div>
    }
}

#[cfg(feature = "dioxus")]
pub fn dioxus_errors(errors: FieldErrors) -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    let FieldErrors { id, messages } = errors;
    rsx! {
        div {
            id: "{id}",
            role: "status",
            "aria-live": "polite",
            for message in messages {
                p { "{message}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ores_form_validation::{FieldState, FieldValidator, Rules};

    fn errors() -> FieldErrors {
        FieldErrors::new("email", &[Code::Required], |_| {
            "<script>unsafe</script>".into()
        })
        .unwrap()
    }

    fn escaped(html: &str) {
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("email-errors"));
        assert!(html.contains("aria-live=\"polite\""));
    }

    #[test]
    fn presentation_uses_shared_lifecycle_and_fallback_messages() {
        let validator = FieldValidator::new(Rules {
            required: true,
            ..Rules::default()
        })
        .unwrap();
        let mut state = FieldState::default();
        state.edit(&validator, None);
        let view = FieldErrors::new("email", state.visible_errors(), |_| String::new()).unwrap();
        assert_eq!(view.aria_invalid(), "false");
        state.blur(&validator, None);
        let view = FieldErrors::new("email", state.visible_errors(), |_| String::new()).unwrap();
        assert_eq!(view.aria_invalid(), "true");
        assert_eq!(view.messages, ["required"]);
        state.edit(&validator, Some("fixed"));
        let view = FieldErrors::new("email", state.visible_errors(), |_| String::new()).unwrap();
        assert_eq!(view.aria_invalid(), "false");
        assert_eq!(view.id(), "email-errors");
        assert!(FieldErrors::new("two ids", &[], |_| String::new()).is_err());
    }

    #[cfg(feature = "mash")]
    #[test]
    fn maud_escapes_messages() {
        escaped(&maud_errors(&errors()).into_string());
    }

    #[cfg(feature = "leptos-ssr")]
    #[test]
    fn leptos_escapes_messages() {
        use leptos::prelude::*;
        escaped(&leptos_errors(errors()).to_html());
    }

    #[cfg(feature = "dioxus-ssr")]
    #[test]
    fn dioxus_escapes_messages() {
        escaped(&dioxus_ssr::render_element(dioxus_errors(errors())));
    }
}
