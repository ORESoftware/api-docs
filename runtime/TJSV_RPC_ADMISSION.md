# TJSV admission of RPC v1 wire fixtures

The `TJSV RPC v1 admission` workflow executes the real
[ORESoftware/typespec-json-schema-validator](https://github.com/ORESoftware/typespec-json-schema-validator)
implementation against both independently authored RPC envelope schemas and
`examples/rpc-v1/conformance.json`. The same encoded fixtures pass through the
actual TypeScript `decodeCall` / `decodeReceipt` implementation. Expected valid
fixtures must be accepted by both, and expected invalid fixtures rejected by both;
agreement on the wrong result fails. Successful decoding must preserve the entire
JSON value, including absent-versus-null fields.

The validator is a full-SHA-pinned checkout with its original `package-lock.json`;
`npm ci --ignore-scripts` installs locked dependencies. The entrypoint verifies
the pin and rejects modified tracked validator files. It does not install an
unversioned npm package or duplicate TJSV's validator implementation.

This gate is an additional admission oracle, not a replacement for the independent
TypeSpec authority, strict RPC audits, receipt-state checks, digest-bound bundle,
SQL/Protobuf projections, or the Rust/Dart/Go/TypeScript runtime-conformance jobs.
It does **not** claim compiler-backed TypeSpec/JSON Schema equivalence or execution
of the Rust, Dart, or Go decoders. Those remain separately evidenced gates. This
finite corpus is not a proof of universal schema/runtime equivalence. TCP prefix
metadata is checked, but this runner does not open TCP/WebSocket/NATS connections,
validate operation-specific bodies, or replace transport/duplicate-member tests.

## Reproduction

From a clean repository checkout, install the exact validator revision recorded
in `scripts/tjsv-rpc-admission.mjs` at `tmp/tjsv`, then run:

```sh
npm ci --prefix tmp/tjsv --ignore-scripts --no-audit --no-fund
node --test scripts/test_tjsv_rpc_admission.mjs
node scripts/tjsv-rpc-admission.mjs
```

The fixed entrypoint accepts no arguments. `tmp/` and `temp/` remain ignored.
The receipt is created exclusively at `tmp/tjsv-rpc-admission.json`; an existing
receipt is never silently overwritten. Preserve or explicitly remove that
previous receipt before another run.

A receipt records every fixture verdict, all disagreement findings, the actual
source revision, reviewed validator revision, and SHA-256 digests of the schemas,
TypeSpec source, corpus, runtime manifest, decoder, and runner. Hashing TypeSpec
binds the surrounding authority context; it is not a TypeSpec validation claim.
The runner rejects sources changed during evaluation. Receipts contain no clock
value and are reproducible for the same source revision and bytes. They are
unsigned test evidence, not release authorization or runtime attestation.

Exit codes are 0 for agreement with every expectation, 2 for semantic drift, and
3 for malformed corpus/schema, missing tooling, validator refusal/crash, unexpected
runtime exceptions, pin mismatch, or filesystem failure. A decoder's own
`RpcV1Error` is the only exception counted as a runtime rejection. Execution errors
must never become successful negative evidence.

The harness unit tests use synthetic adapters only to test orchestration failure
modes. Only the separate integration step runs TJSV and the real client. The two
results must not be conflated when reporting verification.
