# TJSV admission of RPC v1 wire fixtures

The `TJSV RPC v1 admission` workflow executes the real
[ORESoftware/typespec-json-schema-validator](https://github.com/ORESoftware/typespec-json-schema-validator)
implementation against both independently authored RPC envelope schemas and
`examples/rpc-v1/conformance.json`. Identical encoded fixtures pass through:

- TJSV's schema-instance validator;
- the actual TypeScript `decodeCall` / `decodeReceipt` implementation; and
- `ores-api-docs-client` from `clients/rust`, via the fixed
  `clients/rust/examples/tjsv_admission.rs` oracle.

Expected valid fixtures must be accepted by all three, and expected invalid
fixtures rejected by all three. Agreement on the wrong result still fails.
Successful decoding must preserve the complete JSON value, including
absent-versus-null fields. Rust results are decoded and re-encoded through the
public client API, not a replacement validator or a direct core-only test.

## Fail-closed evidence

The validator is a full-SHA-pinned checkout with its original `package-lock.json`;
`npm ci --ignore-scripts` installs locked dependencies. The entrypoint calls the
shared `verifyValidatorSource` before import and after execution. That verifier
checks tracked bytes against the pinned Git tree, rejects unsafe file paths and
untracked source, and detects modifications hidden from `git status` by
assume-unchanged or skip-worktree. Dependencies remain a separate npm-ci boundary.
No unversioned npm package or local imitation of TJSV is used.

The Rust subprocess must exit successfully and return exactly one versioned JSON
report. Its result set must be complete, ordered, uniquely identified, and match
every fixture name and kind. Verdicts must be actual booleans; rejected fixtures
cannot invent a decoded value. Accepted fixtures must include an unchanged
decoded value. Missing/extra rows, unknown fields, wrong identities, a stale
result set, and malformed output fail rather than becoming negative evidence.
The final findings are recomputed from all three verdict columns; a summary
`passed` flag is not trusted.

## Authority and runtime boundaries

This gate is an additional admission oracle, not a replacement for the independent
TypeSpec authority, strict RPC audits, receipt-state checks, digest-bound bundle,
SQL/Protobuf projections, or the Rust/Dart/Go/TypeScript runtime-conformance jobs.
It does **not** claim compiler-backed TypeSpec/JSON Schema equivalence or execution
of Dart/Go here. Those remain separately evidenced gates. TypeSpec and JSON Schema
remain peer authored authorities; neither is overwritten. Existing Contract IR,
projection and production activation gates remain in force.

This finite corpus is not a proof of universal schema/runtime equivalence. TCP
prefix metadata is checked, but this runner does not open TCP/WebSocket/NATS
connections, validate operation-specific bodies, or replace transport and
raw-wire duplicate-member tests. Native Rust execution is covered; browser/WASM
or no-std support is not implied.

## Reproduction

From a clean repository checkout, install the exact validator revision recorded
in `scripts/tjsv-rpc-admission.mjs` at `tmp/tjsv`, and use the Node/Rust toolchain
versions pinned in `.github/workflows/tjsv-rpc-admission.yml`, then run:

```sh
npm ci --prefix tmp/tjsv --ignore-scripts --no-audit --no-fund
node --test scripts/test_tjsv_rpc_admission.mjs scripts/test_tjsv_rust_admission.mjs scripts/test-projection-evidence-io.mjs
cargo test --locked --manifest-path clients/rust/Cargo.toml --example tjsv_admission
node scripts/tjsv-rpc-admission.mjs
```

The fixed entrypoints accept no arguments and introduce no independent flag
parser. The Node runner executes `cargo run --locked` for the actual Rust oracle;
Cargo failure, panic, timeout or malformed stdout aborts admission. There is no
fallback to a successful TypeScript-only receipt when Rust tooling is absent.

The workflow triggers on client/core Rust source, root Cargo manifests and lock,
Rust toolchain configuration, other clients, schemas, corpus and gate sources.
`tmp/` and `temp/` remain ignored. The receipt is created exclusively at
`tmp/tjsv-rpc-admission.json`; an existing receipt is never silently overwritten.
Preserve or explicitly remove that previous receipt before another run.

A receipt records every fixture's TJSV, TypeScript and Rust verdict, all findings,
the source revision, reviewed validator revision, and SHA-256 digests of the
schemas, TypeSpec context, corpus, runtime manifest, decoder and gate sources,
root Cargo manifests/lock, and all tracked Rust core/client files. The source
revision, Rust input inventory and file bytes must remain unchanged during the
run. Hashing TypeSpec binds context; it is not a TypeSpec validation claim.
Receipts contain no clock value and are reproducible for the same revision and
bytes. They are unsigned test evidence, not release authorization or attestation.

Exit codes are 0 for agreement with every expectation, 2 for semantic drift, and
3 for malformed corpus/schema/evidence, missing tooling, validator refusal/crash,
unexpected runtime exceptions, pin mismatch or filesystem failure. Only a
TypeScript `RpcV1Error` or a Rust decoder's own error counts as a rejection.
Rust encoding and report failures abort. Execution errors cannot become
successful negative evidence.

Harness unit tests use synthetic results only to test orchestration failure
modes. Rust adapter tests exercise the public client; the separate integration
step invokes actual TJSV and both real runtimes. These evidence categories must
not be conflated when reporting verification.
