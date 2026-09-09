# RPC v1 TJSV runtime conformance — DEN-3830

The four-language RPC v1 workflow executes the real
[typespec-json-schema-validator](https://github.com/ORESoftware/typespec-json-schema-validator)
against the current independently authored call and receipt JSON Schemas and
`examples/rpc-v1/conformance.json`. The same committed corpus is consumed by
the existing Rust, TypeScript, Dart and Go suites. Fixture expectations are
synthetic test intent; they are not another product-schema authority.

## What executes

`scripts/test-rpc-v1-tjsv-conformance.mjs` imports TJSV's public
`validateJsonSchemaDocument`, `SchemaResolver`, and `validateInstance` APIs.
It does not implement a substitute JSON Schema validator. Before importing
TJSV, it verifies its tracked source bytes against the revision in
`idl/rpc-v1-projection-admission.policy.json`, using the source-integrity
boundary added by #47. CI checks out an immutable matching commit and runs
`npm ci --ignore-scripts`; missing tooling or pin drift fails the job.

The suite checks all current shared valid/invalid fixtures against TJSV and
the shipped TypeScript runtime. Additional deterministic boundary probes
cover required fields, Unicode-scalar limits, transport values, non-object
frames, receipt state/status bounds, null/type confusion, and request header
names. Each instance is checked at object validation, JSON ingress, UTF-8
byte ingress and NDJSON ingress. Accepted instances are also rechecked at
runtime egress and through a length-prefixed round trip.

The extra in-memory probes exercise TypeScript versus TJSV; only the committed
shared corpus is claimed as four-language coverage. Constructors may supply
v/op defaults; decoders must not. These distinct contracts are not conflated.

Negative controls deliberately loosen and tighten an in-memory schema and
prove disagreement with the runtime. Unsupported schema semantics and
unresolved references are execution failures, not successful rejection of an
invalid payload. Likewise, an unexpected runtime programming exception is
not counted as a contract rejection. No test mutates an authored schema or
commits a fabricated parity/IR/admission receipt.

## CI and local execution

The existing `RPC v1 four-language conformance` workflow now contains a
Linux/macOS TJSV test matrix alongside the original four runtime jobs and
peer-authority audit. The `contract-conformance` aggregate requires all jobs
to report `success`; failure, cancellation, skipping or missing results do
not count as conformance. Configure that check as required in repository
branch protection where appropriate; adding this workflow alone does not
assert that administrative branch-protection settings were changed.

With a clean matching TJSV checkout and its locked dependencies installed:

```sh
TSJSV_VALIDATOR_ROOT=/absolute/path/to/typespec-json-schema-validator \
  node --test scripts/test-rpc-v1-tjsv-conformance.mjs
```

This Node test entry point introduces no new CLI argument parser. CI checks
that conformance leaves the api-docs source tree unchanged. Tool checkout
files live under ignored `tmp/`, not in a committed vendor source copy.

## Boundaries

This gate adds schema-to-runtime acceptance evidence for v1. It does not claim
complete TypeSpec-to-JSON-Schema generated-witness parity or certify every
possible payload. The independent TypeSpec and authored JSON Schema lanes,
existing semantic cross-checks, projection deltas and digest-bound bundle
remain in place. The policy remains `blocked` for production projection
admission until the exact RPC v1 peer-authority receipt/Contract IR and
flattened-Protobuf ingress/egress evidence are available.

RIDL v2 frames are separate. Gleam, live NATS/network interoperability,
application-level authorization and all-language coverage of every new probe
are not certified by this gate. No schema constraints or runtime checks are
relaxed to make the suite pass.
