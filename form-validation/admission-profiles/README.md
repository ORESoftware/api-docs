# TJSV form-admission profiles (DEN-3045)

These representative, product-neutral JSON-value boundaries test the **actual**
`ores-form-validation` Rust implementation and `ores_form_validation` on Dart VM
and compiled JavaScript. They complement the portable field corpus and the
validation-message contract; they do not replace either.

`main.tsp` and `authored.schema.json` are independent, manually maintained peer
authorities. Neither is generated from or allowed to overwrite the other.
`ORESoftware/typespec-json-schema-validator` at the reviewed full SHA in
`check.mjs` executes its real TypeSpec compiler, structural comparison and
bidirectional instance validation. Generated witnesses are temporary comparison
evidence only. Both schemas must also agree with all 78 labeled specimens.

## Covered boundaries

| Profile | Admission contract |
| --- | --- |
| TextSubmission | Exactly one required string `value`, 1–80 Unicode scalar values. Whitespace and line breaks are preserved; no implicit nonblank rule. |
| PhoneSubmission | Exactly one required string `value`, E.164 syntax (`+` and 2–15 ASCII digits, no leading zero). Not number-plan validity or ownership. |
| IntegerSubmission | Exactly one required string `value`, strict integer **form text** with numeric value 0–150. `-0` is preserved; decimal notation, exponent, leading plus/zero and whitespace are rejected. Not JSON Schema's numeric `integer` representation. |

All profiles reject null/missing fields, wrong types, non-object envelopes and
unknown fields. Cases cover supplementary-plane emoji, combining characters,
line terminators, embedded NUL, lower/upper bounds and coercion attempts. The
Rust adapter uses strict Serde decoding **then** the production FieldValidator;
Dart performs the same object checks and calls its production FieldValidator.
The adapters do not reproduce the field validators or run a third schema engine.

This is JSON-value admission, not a raw-wire parser conformance claim. Duplicate
JSON keys, malformed UTF-8 and unpaired-surrogate wire representations are out
of this profile. Transport decoding/budgets, email/date/logical-line policies,
Flutter widgets and browser/native product behavior retain their separate gates.
Existing core tests continue to cover Dart's malformed Unicode rejection.

## Evidence gate

After checking out the pinned validator at `tmp/tjsv`, installing its locked
Node dependencies and preparing the existing Rust/Dart toolchains:

```sh
node --test form-validation/admission-profiles/evidence.test.mjs
(cd form-validation/dart && dart pub get --offline)
node form-validation/admission-profiles/check.mjs
```

The fixed check takes no command-line options; it adds no competing flag parser.
It executes Rust and both Dart targets freshly from the candidate source, checks
exact case coverage and input preservation, and compares every result with TJSV.
Missing/duplicate/unknown rows, absent runtimes, wrong versions, malformed
booleans, normalization, validator exceptions and failed processes stop the gate.
Runtime output is captured directly, never loaded from an earlier receipt.

Negative controls deliberately reduce a disposable schema copy's text maximum
from 80 to 79 and fabricate an accepting runtime verdict for an invalid case.
Both must be detected. Authored files are not changed to make the gate pass.

Receipts retain the candidate commit, TJSV full SHA, all tracked form-source
SHA-256 digests, corpus and runtime-output digests, toolchain versions, parity
run IDs, negative-control evidence and per-case **verdicts, not submitted values**.
The exact-head read-only workflow checks committed formatting and locks and
uploads receipts for review. No CI credential or write permission is required.

A finite corpus is regression evidence, not a universal equivalence proof.
Product `*-interfaces` must still own their independent authorities; public
`*-lib-core` composes these primitive rules, and actual handlers must validate
before effects. This PR does not migrate the fleet or make Opto-Sync a validator.
