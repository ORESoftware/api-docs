# Portable form validation (DEN-3045)

A product-neutral Rust native/WASM crate and Dart VM/JavaScript package for MASH,
Leptos, Dioxus, native Rust desktop, and Flutter fields. This is a **validation
primitive and UI-adapter package**, not a product schema authority, a generic
remote-schema interpreter, an authorization system, or an Opto-Sync component.

## Ownership and existing libraries

Product `*-interfaces` retain independent authored TypeSpec and JSON Schema
contracts. Product `*-lib-core` composes these helpers with its public rules;
`*-clients` and UIs import that core instead of copying validators. Private
checks stay in server-only code and `*-orm-core`. This directory's JSON is a
synthetic test corpus, not a third product-contract authority. Product contract
changes still require the existing peer-authority/tjsv admission gates.

Serde deserializes data; decoding is not validation. Rust uses Garde for the
HTML email grammar and exposes a Garde custom-rule adapter. The date primitive
uses Chrono without clock/timezone/network features. Dart uses the identical
explicit email profile and platform date parser with strict round-trip checks;
it has no runtime package dependencies. UI frameworks do not define independent
regexes. Shared fixtures are checked on Rust native, Dart VM and compiled Dart
JavaScript; Rust WASM is compile-checked separately.

## Version 1 semantics

| Rule | Exact behavior |
| --- | --- |
| Required | `None`/`null` and `""` are absent. An optional absent field skips other rules. Whitespace is not trimmed. |
| Non-blank | Optional explicit Unicode White_Space check with a pinned codepoint set; no implicit normalization. |
| Characters | Unicode scalar values, not UTF-8 bytes, UTF-16 units or grapheme clusters. Combining marks count separately. Bounds inclusive. |
| Lines | Logical lines. CRLF is one break; bare CR/LF, U+0085, U+2028, U+2029 also break. Trailing break adds a line. Wrapping does not. Max 2 means at most two; min 2 + max 2 means exactly two. |
| Email | Garde/WHATWG ASCII mailbox profile, local part <=64, total <=254, dotted domain, no leading/trailing/consecutive local dots. EAI, quoted local parts and local-only domains intentionally unsupported. No deliverability/ownership claim. |
| Phone | E.164 **syntax only**: `+`, first digit 1–9, total 2–15 ASCII digits. National formatting requires an explicit region-aware libphonenumber adapter before validation. Number-plan validity and verified ownership are separate server operations. |
| Number | Strict decimal text `-?(0|[1-9][0-9]*)(\.[0-9]+)?`; finite IEEE-754 double, inclusive min/max. No spaces, grouping, exponent, leading plus, NaN or Infinity. This is not exact decimal/money validation. |
| Integer | Strict integer text; inclusive bounds and magnitude <=2^53−1 for exact VM/JavaScript parity. Larger identifiers should remain strings, or use a separate BigInt contract. |
| Date | Exact `YYYY-MM-DD`, real Gregorian date, year 0001–9999; inclusive optional date bounds. No timezone, implicit current clock or datetime coercion. |
| Input budget | 65,536 UTF-8 bytes per nonempty field; checked before field rules. Dart rejects unpaired surrogates instead of replacing them. |
| Configuration | Contradictory/out-of-budget lengths, zero lines, nonfinite or inverted numeric bounds, fractional integer bounds, invalid dates and wrong-kind bounds fail construction. |

Errors contain ordered localization codes only: size/required, blank, character
bounds, line bounds, format, range. No email, phone number, field value, provider
response or database message is copied into an error. The application owns field
paths and localization; never render unescaped errors/values or log raw forms.

## Rust: web and native share the exact same crate

```rust
use ores_form_validation::{FieldValidator, Kind, Rules};
let email = FieldValidator::new(Rules {
    kind: Kind::Email, required: true, max_chars: Some(254), max_lines: Some(1),
    ..Rules::default()
})?;
let errors = email.validate(Some("person@example.com"));
# Ok::<(), ores_form_validation::InvalidRules>(())
```

`garde_rule` composes with `#[garde(context(FieldValidator))]` and
`#[garde(custom(garde_rule))]`; the Rust test derives Serde and Garde together,
proves deserialization alone accepts invalid text, and verifies the Garde
report is value-free. Product-specific cross-field and database rules are not
flattened into these primitive field checks.

`FieldState` is the shared edit/blur/submit adapter. `edit` updates validity but
hides errors until touched/submitted; `blur` reveals them; `submit` always
revalidates; correcting input clears stale local errors. It never retains input.

- **MASH:** deserialize the Axum form/JSON body into the product input DTO,
  run its core validator before domain effects, and render errors with Maud's
  escaped text nodes. Return 422 for invalid submissions. HTMX must explicitly
  swap the designated 422 form fragment (not arbitrary error responses); keep
  ordinary no-JavaScript form submission functional. CSRF and authorization are
  separate gates. HTML attributes are hints, not the rule authority.
- **Leptos:** store `FieldState` in a signal; call `edit` on input and `blur` on
  blur; render `visible_errors()` via escaped view text with `aria-invalid` and
  `aria-describedby`. The server function calls the same public validator again.
- **Dioxus:** store the same adapter in `use_signal`; the event handler passes
  the current string to `edit`/`blur`. Render codes through localized RSX text.
  Revalidate inside the server function, not only the client event handler.
- **Native Rust:** GPUI/Slint/egui/other native controls use the same adapter and
  validator. No DOM, network, database or webview dependency is required.

These are integration boundaries, not a claim that every product UI has been
modified. Concrete framework widgets, app-specific DTO wiring and browser/widget
E2E acceptance are separate rollout gates.

## Flutter/Dart

```dart
final email = FieldValidator(const Rules(
  kind: Kind.email, required: true, maxChars: 254, maxLines: 1,
));
TextFormField(
  validator: email.validator(localize: (code) => messages[code] ?? code),
  autovalidateMode: AutovalidateMode.onUserInteraction,
);
```

The callback is exactly `String? Function(String?)`: null only for valid input.
An empty translation falls back to the stable code instead of hiding invalidity.
It works with `FormField<String>` and Flutter desktop/mobile/web. Validate the
whole `FormState` on submit, then validate again on the server. `maxLines: 2`
on a Flutter text control is a layout setting, **not** the semantic two-line
rule. Flutter's built-in character counter uses graphemes; supply a scalar-aware
counter rather than using its `maxLength` as a contradictory enforcement rule.
Preserve IME composition while editing; do not auto-truncate/normalize input.

## Opto-Sync is not the validator

Drafts may be incomplete and invalid. Store/sync them only under an explicit,
versioned draft contract and consent/retention policy, not as authorized domain
commands. Validate at the UI boundary for feedback; validate on server admission
before side effects and after merging synchronized/conflicting fields. A locally
valid result is not an authorization grant. Schema/rule version mismatch needs
migration or rejection, not silently trusting an old validation receipt. Never
sync credentials, OTPs, payment secrets or transient validation/provider errors.

Remote uniqueness/availability/ownership checks are asynchronous server rules.
Cancel/debounce them and bind their response to the field revision so an old
response cannot overwrite a newer value. They do not replace the deterministic
local rules, permission checks, transactional constraints or database uniqueness.

## Verification and adoption

Run `cargo test --manifest-path form-validation/rust/Cargo.toml --locked`.
From `form-validation/dart`, run `dart pub get --offline`, `dart analyze
--fatal-infos`, then `dart run test/conformance.dart`. The read-only CI also
compiles/runs that same corpus as JavaScript and compiles Rust for WASM. During
bootstrap CI uploads the generated Cargo.lock/formatting candidates, but its
final gate fails until those exact candidates are reviewed and committed.

Consume this repository at an immutable reviewed commit via the existing Zed
package boundary, then point Cargo/pub at the materialized `form-validation/rust`
and `form-validation/dart` subdirectories. Native registry publication, frozen
Zed install acceptance, concrete Leptos/Dioxus/MASH/Flutter widgets, and adoption
in each product's core/UI remain explicit gates; do not claim they are already
released. The package deliberately adds no Opto-Sync or ores-otel dependency.

Primary references: https://serde.rs/ ; https://docs.rs/garde/0.23.0/garde/ ;
https://html.spec.whatwg.org/multipage/input.html#email-state-(type=email) ;
https://docs.flutter.dev/cookbook/forms/validation ;
https://api.dart.dev/dart-core/DateTime/DateTime.utc.html
