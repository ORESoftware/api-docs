# Oreslang route/RPC runtime admission — pending compiler evidence

Tracking: [api-docs #326](https://github.com/ORESoftware/api-docs/issues/326).
Status: **not admitted** (2026-10-18). This file is a hardening policy, not a
third contract authority or a compiler-backed integration.

## What the Rust unit gate protects

`rust/src/schema.rs::unadmitted_oreslang_language_surface_fails_closed`
proves the current independently reviewed `json-schema/language-surface.schema.json`
does **not** permit the new `language: "oreslang"` enumerant.
The existing `language: "other"` entry remains generic and is **not** a
statement that Oreslang compiled or passed any runtime tests.

The gate deliberately changes **no** TypeSpec authority, authored JSON Schema,
route inventory, operation key, Protobuf ledger or emitted client. Adding a new
`language: "oreslang"` first to the schema merely to satisfy this test would
be an invalid single-authority change and a false runtime claim.

## Minimum acceptance tests before changing the supported-language map

1. Independently authored TypeSpec and Draft 2020-12 JSON Schema must both
   explicitly support the same Oreslang route/RPC surface; use
   [TJSV](https://github.com/ORESoftware/typespec-json-schema-validator)
   against exact current inputs and negative schema mutation fixtures. Compare
   the canonical RPC semantic digest and preserve every operation key and
   authored wire name; generated code is a derivative witness only.
2. Compile a real Oreslang handler/binding and run unary v1 JSON calls plus
   v2 `call`, `data`, `end`, `error`, `cancel` frames. Exercise
   malformed/unknown frames, deadlines, cancellation, bounded streaming and
   backpressure, binary media mismatch, and cross-language differential traces.
3. Prove compiler/runtime binary identity and source Git tree SHA, actual
   test execution (not workflow declaration), fixture hashes, current TypeSpec
   / JSON-schema receipt, host capability and actor ownership boundaries, and
   consumer-side reproducibility. Missing, skipped, zero-step or failed checks
   are **not admitted**.
4. Add Oreslang as a distinct reviewed language adapter to the existing
   digest-bound RPC generation bundle, **not** a new independent generator
   or operation authority. Then update the authored authority pair and the
   Rust gate/test in the same reviewed PR with verified exact-head evidence.

Until then the only admitted language strings remain the ones declared by
the existing schema: Rust, TypeScript, Dart, Gleam and generic `other`.
The current rust unit test is a denial regression, not feature implementation.
