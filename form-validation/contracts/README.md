# TJSV-enforced form validation messages — DEN-3045 / DEN-3830

This shared **public message interface** bridges Rust web/native/WASM and
Dart/Flutter without turning Opto-Sync into a validator or adding a schema
interpreter to either UI. Field/domain validation still belongs in the existing
public core; private rules and server admission stay server-side.

`main.tsp` and `authored.schema.json` are independent, human-maintained peer
authorities. Neither is generated from, overwritten by, or given fallback
precedence over the other. TJSV compiles TypeSpec to a disposable Schema B under
ignored `tmp/tjsv-form/witness/` and compares it with authored Schema A. The
emitter's `sealObjectSchemas: true` setting is part of that check: both models
are closed, matching A's `unevaluatedProperties: false`. The generated witness,
Contract IR, runtime observations, boundary envelopes and receipts are downstream
evidence only and never become another editable authority.

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

## Exact-source TJSV gate

The workflow checks out the exact reviewed TJSV commit named by `check.mjs`,
installs its committed lock with lifecycle scripts disabled, and verifies the
validator checkout with the shared `scripts/tjsv-source-integrity.mjs` helper.
The candidate source closure is also bound to the exact candidate Git tree:
every reviewed path must be a regular tracked blob, bounded/no-link reads are
used for evidence bytes, Git blob IDs are recomputed, and the complete reviewed
path inventory must match exactly. The same SHA-256 source snapshot is checked
again after execution. Hidden working-tree/index tricks or source mutation cannot
silently satisfy promotion.

The gate uses the real upstream TJSV APIs:

- `runCheck()` for compiler-backed TypeSpec/authored-JSON-Schema parity and
  differential instance validation;
- `buildContractIr()` for parity-bound, non-authoritative Contract IR;
- `validateInstance()` for the two schema lanes;
- `createRuntimeEvidenceBindingAgainstCurrentInputs()` and
  `verifyRuntimeEvidenceAgainstCurrentInputs()` for the existing current-source
  runtime-conformance oracle; and
- `verifyLanguageBoundaries()` as an additional cross-language promotion veto.

A passing parity report must explicitly retain zero findings, execute differential
probes over all three declarations, and report zero divergences and refusals. The
Contract IR must be passed/admissible with a nonempty declaration inventory and
no excluded or out-of-scope declarations. No generated witness or IR may be
promoted to authority.

## Executed runtimes and boundary promotion

The runtime matrix contains authored Schema A, emitted witness B, real Rust/Serde,
Dart VM and compiled Dart JavaScript codecs. Every message specimen must have the
expected acceptance verdict and preserve accepted values through round trips. The
shared field fixtures are also executed by the actual Rust and Dart primitive
validators and converted into these messages; exact error ordering/content must
match fixture expectations. Probes receive inputs/IDs, not expected verdicts or
expected error arrays.

The existing TJSV runtime-conformance receipt remains required. On top of it, the
same fresh runtime observations are converted to TJSV's public
`language-boundary-evidence/v1` envelopes and submitted to the **actual upstream**
`verifyLanguageBoundaries()` implementation. Promotion requires three required
targets across two distinct languages:

- Rust / native;
- Dart / VM; and
- Dart / JavaScript on Node.

Every required target must report both ingress and egress validation as passed,
bind the exact candidate source SHA, parity `runId` and Contract IR `irId`, and
carry explicit toolchain/generator identities. A boundary receipt may pass only
with 3/3 admitted evidence envelopes, two distinct required languages and zero
unexplained findings.

Each boundary `artifactDigest` is the SHA-256 of the retained parsed runtime
**observation evidence** reviewed by this gate. It binds promotion to that exact
observation; it does **not** claim reproducible builds, compiled-binary identity,
supply-chain attestation or universal language equivalence. Stronger provenance
claims require separate build/attestation evidence.

## Adversarial controls

The existing runtime gate deliberately tests missing runtime/case evidence,
skipped execution, flipped verdicts, stale corpus/IR bindings and an unrecognized
evidence property. A disposable authored-schema max-items mutation must also stop
parity. The Node parser suite separately rejects malformed, duplicate, truncated,
value-leaking and altered-round-trip observations.

The upstream boundary verifier is separately required to reject:

- missing required Rust evidence;
- a stale Dart-VM parity binding;
- attempted promotion of the generated witness to peer authority; and
- disabled ingress on a required target.

These controls call the real TJSV verifiers; no local substitute is allowed to
turn expected data into a fabricated passing receipt.

## Retained evidence

The read-only workflow retains parity, Contract IR, runtime evidence/conformance,
upstream language-boundary evidence and verification, boundary negative controls,
the authored-schema drift evidence, the existing runtime negative controls, and
final `ores.form-validation.tjsv-admission/v2` evidence. The final receipt binds
the candidate source revision, exact TJSV revision, source digests, corpus digest,
parity `runId`, Contract IR `irId`, boundary `verificationId`, runtime observation
digests, toolchains and coverage counts. It explicitly records
`universalEquivalenceProven: false`.

CI cannot pass with an uncommitted Rust lock or formatter drift. The workflow has
read-only repository permissions and never needs credentials. The fixed entrypoint
accepts no ad hoc flags, and stale output reuse fails because receipts are created
exclusively.

This verifies this shared error interface and these executed runtime/corpus cells.
It does not establish universal semantic equivalence, browser DOM/hydration, live
server admission, complete fleet adoption or registry publication. Existing
Chrome primitive execution, Flutter widget and renderer checks remain separate
required evidence. Product-owned TypeSpec/Schema pairs remain in `*-interfaces`;
public domain rules stay in `*-lib-core`, and database rules in `*-orm-core`.
