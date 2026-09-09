# TJSV form-admission profiles (DEN-3045 / DEN-3830)

These representative, product-neutral JSON-value boundaries test the **actual**
`ores-form-validation` Rust implementation, `ores_form_validation` on Dart VM
and Dart-compiled JavaScript, and the TypeScript/Zod implementation. They
complement the portable field corpus and the validation-message contract; they
do not replace either.

`main.tsp` and `authored.schema.json` are independent, manually maintained peer
authorities. Neither is generated from or allowed to overwrite the other.
`ORESoftware/typespec-json-schema-validator` (TJSV) at the reviewed full SHA in
`check.mjs` executes its real TypeSpec compiler, structural comparison,
bidirectional instance validation, Contract IR builder and
`verifyLanguageBoundaries()` promotion gate. The TypeSpec-generated JSON Schema
witness, Contract IR, runtime observations, boundary evidence envelopes and
verification receipt are downstream evidence only. They are never a third
authority and cannot win a disagreement by fallback.

## Covered boundaries

| Profile | Admission contract |
| --- | --- |
| TextSubmission | Exactly one required string `value`, 1–80 Unicode scalar values. Whitespace and line breaks are preserved; no implicit nonblank rule. |
| PhoneSubmission | Exactly one required string `value`, E.164 syntax (`+` and 2–15 ASCII digits, no leading zero). Not number-plan validity or ownership. |
| IntegerSubmission | Exactly one required string `value`, strict integer **form text** with numeric value 0–150. `-0` is preserved; decimal notation, exponent, leading plus/zero and whitespace are rejected. Not JSON Schema's numeric `integer` representation. |

All profiles reject null/missing fields, wrong types, non-object envelopes
(including positional arrays) and unknown fields. Cases cover supplementary-plane
emoji, combining characters, line terminators, embedded NUL, lower/upper bounds
and coercion attempts. The Rust adapter uses strict Serde decoding **then** the
production FieldValidator; Dart performs the same object checks and calls its
production FieldValidator; TypeScript executes the reviewed compiled Zod profile.
The adapters do not reproduce the field validators or run a third schema engine.

This is JSON-value admission, not a raw-wire parser conformance claim. Duplicate
JSON keys, malformed UTF-8 and unpaired-surrogate wire representations are out
of this profile. Transport decoding/budgets, email/date/logical-line policies,
Flutter widgets and browser/native product behavior retain their separate gates.
Existing core tests continue to cover Dart's malformed Unicode rejection.

## Evidence and promotion gate

After checking out the exact reviewed TJSV commit at `tmp/tjsv`, installing its
locked Node dependencies and preparing the reviewed Rust, Dart and TypeScript
stacks, CI runs:

```sh
node --test \
  form-validation/admission-profiles/evidence.test.mjs \
  form-validation/admission-profiles/four-runtimes.test.mjs \
  form-validation/admission-profiles/language-boundary.test.mjs
(cd form-validation/dart && dart pub get --offline)
node form-validation/admission-profiles/check.mjs
```

The fixed check takes no command-line options and adds no competing flag parser.
Promotion is deliberately staged and fail-closed:

1. TJSV `runCheck()` compares the independent TypeSpec and authored Draft
   2020-12 JSON Schema authorities, executes the positive/negative instance
   corpus and requires explicit zero findings, executed probes, zero divergence
   and zero refusals.
2. TJSV `buildContractIr()` derives a non-authoritative Contract IR bound to that
   exact parity receipt. The declaration inventory must be nonempty and complete;
   excluded or out-of-scope declarations cannot cross this runtime boundary.
3. Fresh candidate code executes four runtime targets: Rust/native, Dart/VM,
   Dart-compiled JavaScript on Node, and TypeScript/Zod on Node. Exact case
   coverage, expected verdicts and input preservation must agree with TJSV.
4. The retained observations are converted into the public TJSV
   `language-boundary-evidence/v1` envelope and submitted to the **actual upstream**
   `verifyLanguageBoundaries()` implementation. All four targets are required,
   both ingress and egress must be `passed`, and the target set must cover three
   distinct languages (`rust`, `dart`, `typescript`).
5. Only a `language-boundary-verification/v1` receipt with `status: passed`, zero
   unexplained findings, four admitted evidence envelopes and three distinct
   required languages may promote the candidate.

Runtime output is captured directly from fresh processes, never loaded from an
earlier receipt. Pinned validator sources are byte-verified using the repository
integrity helper rather than trusted from HEAD/status alone. Candidate bytes are
also compared with their committed blobs; bounded no-link reads and strict UTF-8
decoding reject altered or malformed evidence. Installed dependencies remain the
reviewed lock-file boundary.

Each boundary evidence `artifactDigest` is the SHA-256 of the retained runtime
**observation evidence** for that target. It binds the verifier to the exact
observation it reviewed; it does **not** claim a reproducible build, compiled
binary identity, supply-chain attestation, or universal language equivalence.
Those stronger claims require their own dedicated build/provenance evidence.

## Adversarial controls

The gate must also prove its rejection paths. It deliberately:

- reduces a disposable authored-schema copy's text maximum from 80 to 79;
- fabricates an accepting Dart runtime verdict for an invalid specimen;
- removes required Rust boundary evidence;
- changes a Dart boundary envelope to a stale parity receipt;
- attempts to promote the generated witness to a peer authority; and
- disables required egress validation for a runtime target.

Every mutation must stop evaluation through the relevant real TJSV or runtime
comparison path. Authored source files are never changed to make a gate pass.

## Retained receipts

The exact-head read-only workflow retains the parity report, Contract IR,
deliberate-drift parity report, language-boundary evidence map,
language-boundary verification receipt, boundary negative-control results and the
final `ores.form-admission.receipt/v2` receipt. The final receipt binds the
candidate commit, reviewed TJSV full SHA, tracked form-source SHA-256 digests,
corpus and runtime-output digests, toolchain identities, parity `runId`, Contract
IR `irId`, TJSV `verificationId`, boundary counts, negative-control evidence and
per-case **verdicts, not submitted values**. CI also requires committed formatting
and lock files. No write permission or credential is needed by the workflow.

A finite corpus is regression evidence, not a universal equivalence proof.
Product `*-interfaces` must still own their independent TypeSpec and JSON Schema
contract authorities; public `*-lib-core` composes these primitive rules, and
actual handlers must validate before effects. This gate does not migrate the
fleet, turn generated evidence into an authority, or make Opto-Sync a validator.
