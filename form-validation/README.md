# Portable form validation (DEN-3045)

Shared form rules for MASH, Leptos, Dioxus, native Rust desktop, and Flutter.
This is a product-neutral primitive and adapter package, **not** a product
schema authority, remote-schema interpreter, authorization system, or
Opto-Sync validation engine.

| Directory | Package and purpose |
| --- | --- |
| `rust/` | `ores-form-validation`: native/WASM rules, Garde adapter, value-free edit/blur/submit state |
| `dart/` | `ores_form_validation`: Dart VM/JavaScript rules, same conformance corpus, Flutter-compatible callback |
| `web/` | `ores-form-views`: Maud, Leptos and Dioxus escaped error views, optional per-framework features |
| `flutter/` | `ores_form_fields`: real `OresTextFormField`, scalar-aware counter, shared validation callback |

## Ownership and existing libraries

Product `*-interfaces` retain independent authored TypeSpec and JSON Schema
contracts. Product `*-lib-core` composes these helpers with its public rules;
`*-clients` and UIs import that core instead of copying validators. Private
checks stay in server-only code and `*-orm-core`. `fixtures.json` is a synthetic
test corpus, not a third product-contract authority. Product contract changes
still require the peer-authority/tjsv admission gates.

Serde deserializes data; decoding is not validation. Rust uses Garde for the
HTML email grammar and exposes a Garde custom-rule adapter. Dates use Chrono
without clock/timezone/network features. Dart uses the same explicit email
profile and platform date parser with strict round-trip checks; it has no
runtime package dependencies. Rust and Dart are separate implementations
checked against the same fixtures, not a Rust FFI dependency inside Flutter.
UI frameworks do not define their own validators or regular expressions.

## Version 1 semantics

| Rule | Exact behavior |
| --- | --- |
| Required | `None`/`null` and `""` are absent. An optional absent field skips other rules. Whitespace is not trimmed. |
| Non-blank | Explicit Unicode White_Space check with a pinned codepoint set; no implicit normalization. |
| Characters | Unicode scalar values, not UTF-8 bytes, UTF-16 units or grapheme clusters. Combining marks count separately. Bounds inclusive. |
| Lines | CRLF is one break; bare CR/LF, U+0085, U+2028, U+2029 also break. A trailing break adds a line. Visual wrapping does not. Max 2 means at most two; min 2 + max 2 means exactly two. |
| Email | Garde/WHATWG ASCII mailbox profile, local part <=64, total <=254, dotted domain, no leading/trailing/consecutive local dots. EAI, quoted local parts and local-only domains intentionally unsupported. No deliverability/ownership claim. |
| Phone | E.164 **syntax only**: `+`, first digit 1–9, total 2–15 ASCII digits. National formatting needs an explicit region-aware libphonenumber adapter before validation. Number-plan validity and verified ownership are separate operations. |
| Number | Strict decimal text `-?(0|[1-9][0-9]*)(\.[0-9]+)?`; finite IEEE-754 double, inclusive min/max. No spaces, grouping, exponent, leading plus, NaN or Infinity. **Not exact decimal/money validation.** |
| Integer | Strict integer text; inclusive bounds and magnitude <=2^53−1 for exact VM/JavaScript parity. Larger identifiers remain strings or use a separate BigInt contract. |
| Date | Exact `YYYY-MM-DD`, real Gregorian date, year 0001–9999; inclusive optional date bounds. No timezone, implicit clock or datetime coercion. |
| Input budget | 65,536 UTF-8 bytes per nonempty field. Dart rejects unpaired surrogates instead of replacing them; Rust strings cannot contain them. |
| Configuration | Contradictory/out-of-budget lengths, zero lines, nonfinite or inverted numeric bounds, fractional integer bounds, invalid dates and wrong-kind bounds fail construction. |

Errors contain ordered localization codes only: size/required, blank, character
bounds, line bounds, format, range. No submitted email, phone, field value,
provider response or database message is retained in a validation error. The
application owns field paths and localization; never log raw forms.

## Rust: one core for browser, server and native

```rust
use ores_form_validation::{FieldValidator, InvalidRules, Kind, Rules};

fn main() -> Result<(), InvalidRules> {
    let email = FieldValidator::new(Rules {
        kind: Kind::Email,
        required: true,
        max_chars: Some(254),
        max_lines: Some(1),
        ..Rules::default()
    })?;
    assert!(email.validate(Some("person@example.com")).is_empty());
    Ok(())
}
```

`garde_rule` composes with `#[garde(context(FieldValidator))]` and
`#[garde(custom(garde_rule))]`. The Rust test derives Serde and Garde together,
proves deserialization alone accepts invalid text, then checks that validation
rejects it without retaining the input in its report. Product cross-field and
database checks are not flattened into primitive field rules.

`FieldState::edit` recalculates errors but hides them until touched/submitted;
`blur` reveals them; `submit` always revalidates; correction clears stale errors.
State retains codes, not field values. Native controls use this same crate
without a DOM, webview, Flutter engine, database or network dependency.

## MASH, Leptos and Dioxus

The optional `web/` crate provides `FieldErrors`, `maud_errors`,
`leptos_errors`, and `dioxus_errors`. Build only the needed feature: `mash`,
`leptos`, or `dioxus`; SSR tests additionally enable `leptos-ssr` and
`dioxus-ssr`. Browser builds do not enable SSR features.

Construct `FieldErrors` from the core validator's codes, or from
`FieldState::visible_errors()`, and a code-to-message localizer. Developer-owned
field IDs are validated. Bind `FieldErrors::id()` to the input's
`aria-describedby` and `aria_invalid()` to its `aria-invalid` attribute. The
view contains escaped text in an `aria-live="polite"` status region. An empty
translation falls back to the stable code. Actual renderer tests reject raw
script markup and require the complete escaped message.

**MASH:** deserialize the Axum form/JSON body into the product DTO, then run the
core validator before effects. Render the result through `maud_errors`. Return
422 for invalid submissions; configure HTMX to swap only the designated 422
form fragment. Keep no-JavaScript submission functional. CSRF, authorization,
body-size limits and database constraints remain separate server gates.

**Leptos/Dioxus:** store `FieldState` in a signal, call `edit` on input and `blur`
on blur, then pass visible codes into the corresponding error view. Revalidate
in server functions before effects. Input controls and product-specific event
wiring remain the application's responsibility. These are tested error views,
not complete form builders or a claim of browser hydration/E2E coverage.

HTML attributes are hints, not the rule authority. In particular, do not attach
UTF-16-based HTML length enforcement that contradicts the scalar-count rule.
Preserve IME composition; do not silently truncate, trim or normalize input.

## Flutter/Dart

```dart
import 'package:ores_form_fields/ores_form_fields.dart';
import 'package:ores_form_validation/ores_form_validation.dart' as forms;

// Place inside a Form. The owning State creates/disposes emailController.
final email = forms.FieldValidator(const forms.Rules(
  kind: forms.Kind.email, required: true, maxChars: 254, maxLines: 1,
));
final field = OresTextFormField(
  controller: emailController,
  validator: email,
  label: 'Email',
  localize: (code) => messages[code] ?? code,
);
```

The wrapper delegates to `email.validator()`, which is exactly
`String? Function(String?)` and can also be assigned directly to an existing
`TextFormField` or `FormField<String>`. Null means valid; an empty translation
cannot hide invalidity. Validate the whole `FormState` on submit, then validate
again on the server.

The wrapper supplies a scalar-aware counter rather than Flutter's grapheme
`maxLength` limiter. `visualMaxLines` controls layout; `Rules(maxLines: 2)` is
the semantic two-logical-line rule. The tests exercise actual `Form` and
`TextFormField` widgets: invalid/corrected email, emoji/combining-character
counts without truncation, and visual rows versus logical lines.

## Opto-Sync is not the validator

Drafts may be incomplete and invalid. Sync them only under an explicit,
versioned draft contract and consent/retention policy, not as authorized domain
commands. Validate locally for feedback; revalidate at server admission before
effects and after merging synchronized/conflicting fields. A locally valid
result is not an authorization grant. Rule-version mismatch needs migration
or rejection, not trust in an old validation receipt. Never sync credentials,
OTPs, payment secrets or transient validation/provider errors.

Remote uniqueness/availability/ownership checks are asynchronous server rules.
Cancel/debounce them and bind responses to field revisions so stale responses
cannot replace newer results. They do not replace deterministic local checks,
permissions, transactional constraints or database uniqueness.

## Verification and adoption

```sh
cargo test --manifest-path form-validation/rust/Cargo.toml --locked
cargo test --manifest-path form-validation/web/Cargo.toml --locked --all-features
(cd form-validation/dart && dart pub get --offline && dart analyze --fatal-infos && dart run test/conformance.dart)
(cd form-validation/flutter && flutter pub get && flutter analyze --no-pub --fatal-infos && flutter test --no-pub)
```

Read-only CI checks 85 shared positive/negative fixtures on Rust native, Dart VM
and compiled JavaScript; additional tests cover invalid configuration, input
budgets, Serde/Garde composition, privacy and UI lifecycle. It also runs the
three Flutter widget tests and real Maud/Leptos/Dioxus SSR tests. Rust core and
client-feature web adapters are compiled for WASM, not browser-executed.
Clippy, Dart/Flutter analysis, reviewed committed locks and clean formatting
are merge gates. Dependency/formatting candidates may be uploaded for review,
but missing committed locks or any source drift fails the final gate.

Consume an immutable reviewed commit through Zed's existing **whole-repository**
target, then reference these Cargo/pub subdirectories. Keep sibling paths:
`web/` imports `../rust` and `flutter/` imports `../dart`; extracting one subtree
alone breaks those paths. The manifest includes the new directory and checks
its expected package files without adding reverse Opto-Sync/ores-otel edges.

Native registry publication, a genuine frozen Zed installation, product DTO
and input-event wiring, browser/hydration E2E, and each native/mobile target's
application tests remain rollout gates. Passing this package's tests is not a
claim that every organization's forms or server handlers have been migrated.

Primary references: https://serde.rs/ ; https://docs.rs/garde/0.23.0/garde/ ;
https://html.spec.whatwg.org/multipage/input.html#email-state-(type=email) ;
https://docs.flutter.dev/cookbook/forms/validation ;
https://api.dart.dev/dart-core/DateTime/DateTime.utc.html
