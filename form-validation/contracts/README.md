# TJSV-enforced form validation messages — DEN-3045

This shared **public message interface** bridges Rust web/native/WASM and
Dart/Flutter without turning Opto-Sync into a validator or adding a schema
interpreter to either UI. Field/domain validation still belongs in the existing
public core; private rules and server admission stay server-side.

`main.tsp` and `authored.schema.json` are independent authored authorities.
Neither is generated from the other. TJSV compiles TypeSpec to separate Schema B
under ignored `tmp/tjsv-form/witness/` and compares it against Schema A. The
emitter's `sealObjectSchemas: true` setting is part of the check: both models
are closed, matching A's `unevaluatedProperties: false`. The Contract IR and
receipts are downstream evidence only, never another editable authority.

## Public wire contract

```json
{"schema_version":"ores.form-validation/v1","issues":[{"field":"email","code":"email"}]}
```

Messages have exactly `schema_version` and `issues`. Each issue has exactly
`field` and `code`. Fields are developer-owned ASCII identifiers, 1–128
characters, beginning with a letter/digit and then letters/digits/underscore/
dot/colon/hyphen. They are not user text or automatically generated HTML IDs.
The complete error-code vocabulary includes the existing 17 Rust core codes
plus Dart's `invalid_unicode`. At most 128 issues are accepted; empty is allowed
but is not authorization or proof of server acceptance. Ordering is preserved.
Unknown versions/codes/properties, null required fields and malformed IDs fail.

There is no submitted-value, localized-message, database/provider-error or
credential field. This protects the interface, not arbitrary application misuse:
callers must supply developer-owned field IDs, localize codes at presentation,
escape HTML as before and never log raw decode failures or forms.

## Rust use

`form-validation/wire-rust` is the optional `ores-form-validation-wire` companion
crate. The original lightweight primitive crate is unchanged. The companion
imports it, not vice versa, and depends on Serde but no UI, database or sync.

```rust
use ores_form_validation::{FieldValidator, Kind, Rules};
use ores_form_validation_wire::ValidationMessage;

let validator = FieldValidator::new(Rules {
    kind: Kind::Email, required: true, ..Rules::default()
})?;
let message = ValidationMessage::from_codes("email", &validator.validate(Some(input)))?;
// Serialize with your normal Serde transport. Deserialize ValidationMessage
// to enforce the same version/shape/code/identifier bounds on incoming values.
```

Constructors and Serde deserialization both enforce the rules. Fields are private;
getters expose immutable data. This is suitable for MASH/Axum response DTOs,
Leptos/Dioxus server-function DTOs and native clients. Application-specific
transport wiring still must adopt it; this package does not change deployed APIs.

## Dart/Flutter use

```dart
import 'package:ores_form_validation/validation_message.dart';

final message = ValidationMessage.fromCodes('email', emailValidator.validate(input));
final jsonValue = message.toJson();
final decoded = ValidationMessage.fromJson(jsonValue);
```

The list is copied and unmodifiable. The existing Flutter validator callback
and widgets are unchanged. Transport JSON syntax/duplicate-member handling,
request/body budgets, authentication and field-revision binding are separate
admission concerns; this gate compares already-decoded JSON **values**, not
arbitrary raw JSON byte streams or network security.

## Executed gate

The workflow checks out TJSV at the immutable revision in `check.mjs`, installs
its own committed lock with lifecycle scripts disabled, and uses the real APIs:
`runCheck`, `buildContractIr`, `validateInstance`,
`createRuntimeEvidenceBindingAgainstCurrentInputs`, and
`verifyRuntimeEvidenceAgainstCurrentInputs`. Missing or mismatched sources stop
admission. These are not placeholder commands or a homegrown TJSV substitute.

The matrix contains independently authored Schema A, emitted witness B, real
Rust/Serde, Dart VM and compiled Dart JavaScript codecs. Every message specimen
must have the expected acceptance verdict and preserve accepted values through
round trips. The 85 existing field fixtures are also executed by the actual
Rust and Dart primitive validators and converted into these messages; exact
error ordering/content must match the fixture expectations in all three runtimes.
The probes receive inputs/IDs, not expected verdicts or expected error arrays.

Negative controls call the real TJSV gate with a missing runtime/case, a skipped
runtime, a flipped verdict, stale corpus/IR bindings and an unrecognized evidence
property. A deliberate authored-schema max-items drift must stop parity. The
small Node test suite separately rejects malformed, duplicate, truncated,
value-leaking and altered-round-trip observations.

Reports bind source revision, individual source-file digests, TJSV revision,
corpus digest, parity receipt and verified Contract IR. Source changes during
the run are rejected. CI cannot pass with an uncommitted lock or formatter drift.
The workflow has read-only repository permissions and never uploads credentials.

Run the fixed entrypoint from an exact checkout after preparing `tmp/tjsv`, Rust
and Dart as in `.github/workflows/tjsv-form-contract.yml`. It accepts no ad hoc
flags. Reports use exclusive creation; reusing stale output intentionally fails.

This verifies this shared error interface and these executed runtime/corpus
cells. It does not establish universal semantic equivalence, browser DOM/hydration,
live server admission, complete fleet adoption or registry publication. Existing
Chrome primitive execution, Flutter widget and renderer checks remain separate
required evidence. Product-owned TypeSpec/Schema pairs remain in `*-interfaces`;
public domain rules stay in `*-lib-core`, and database rules in `*-orm-core`.
