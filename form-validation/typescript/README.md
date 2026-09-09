# TypeScript/Zod admission profiles — DEN-3045

Reusable typed runtime adapters for the three representative contracts in
`../admission-profiles/`. The independent TypeSpec and authored JSON Schema
remain the contract authorities. Zod schemas are implementation projections,
not a replacement authority and not an arbitrary remote-schema interpreter.

`parseProfile(profile, unknown)` returns an immutable success value or a
value-free `{ success: false, code: 'invalid_submission' }` result. An unknown
profile throws a fixed configuration error. Application handlers should branch
on `success` before side effects. No ZodError, submitted value, unknown key or
localized/provider message is returned on failure. Successful objects are
separate, shallow-frozen records containing only an immutable string.

```ts
import { parseProfile } from './tmp/dist/profiles.js';

const result = parseProfile('TextSubmission', { value: 'Name' });
if (result.success) {
  // result.data.value is a readonly string field, not unknown.
  console.log(result.data.value);
}
```

Profiles retain the existing rules: 1–80 code points for text, E.164 phone
syntax (not ownership), and integer **form text** in 0–150. `-0`, whitespace in
text, combining characters and emoji are preserved; no coercion, defaults,
trimming, case changes or normalization occur. Wrong object shapes, unknown
fields, positional arrays and non-string values fail. Ordinary accessors,
class instances, inherited-property bags and symbol fields are refused before
Zod evaluation. Hostile JavaScript proxies are outside this JSON-value API's
scope, as are raw-byte duplicate-member and malformed-UTF-8 policies.

Zod 4.5.4 and TypeScript 5.9.3 are exact dependencies in the reviewed lockfile.
DOM ambient declarations supply the standard URL type used by Zod; no DOM
runtime calls or browser dependency are added. Dependency type-checking stays enabled.
Zod 4.5 changed string bounds from UTF-16 units to Unicode code points; do not
upgrade or downgrade without the same corpus. Tests include astral emoji,
combining marks and lone surrogates. See the primary release explanation:
https://zod.dev/blog/zod-4-5

```sh
npm ci --ignore-scripts --no-audit --no-fund
npm test
```

Builds/declarations live under ignored `tmp/dist/`. This private package is not
published to npm; source users must build it. The repo's whole-repository Zed
layout preserves these sources; a frozen install or fleet rollout is not
claimed. Product rules remain in public `*-lib-core`, with model contracts in
`*-interfaces`; private authorization/database rules remain server-owned.

The existing exact-head `form-profile-admission.yml` now requires Zod plus
Rust native, Dart VM and Dart JavaScript. It executes the real pinned
ORESoftware/typespec-json-schema-validator compiler/structural/differential
APIs and all 81 shared specimens. The TypeScript probe receives only id,
profile and input, never expected answers or source schemas. Missing lanes,
wrong results, changed values, failed builds and process errors fail closed.
The compiler and probe receive an allowlisted environment, not credentials or
NODE_OPTIONS. Source digests include this package, manifest, lock and probe;
the receipt also records exact versions and the compiled implementation hash.

Finite-corpus conformance is regression evidence, not universal equivalence,
production deployment, all-field form support or automatic handler adoption.
Existing Rust/Dart implementations, message codecs, browser/Flutter tests and
Opto-Sync synchronization responsibilities are unchanged.
