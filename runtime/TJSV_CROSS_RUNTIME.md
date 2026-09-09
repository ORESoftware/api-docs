# TJSV RPC v1 cross-runtime conformance

This extends the TJSV oracle from `TJSV_RPC_ADMISSION.md` with actual Rust, Go,
and Dart execution, and all-to-all encoder/decoder round trips with TypeScript.
It uses the real pinned `ORESoftware/typespec-json-schema-validator` implementation,
not a copied validator or a passing placeholder.

## Execution

The `TJSV RPC cross-runtime` workflow checks out the exact candidate, installs the
existing reviewed TJSV revision and lockfile, and builds three test-only probes:

- Rust: `clients/rust/examples/tjsv_rpc_probe.rs` imports the public client facade
  without Axum; it uses existing shared decoders and encoders.
- Go: `clients/go/testdata/tjsv_probe/main.go` imports the actual Go client module.
- Dart: `clients/dart/tool/tjsv_rpc_probe.dart` imports the actual Dart client and
  compiles to a native executable. It does not change the Flutter-safe library.

The probes take a bounded JSON request over stdin and emit a closed JSON response.
They receive only fixture names, kinds and encoded bytes: never expected verdicts,
valid/invalid group membership, or pre-decoded payloads. They contain no replacement
validation rules. Codec errors produce explicit rejection observations; encoder
failures after successful decoding, crashes and process failures stop evaluation.

`scripts/tjsv-rpc-cross-runtime.mjs` first runs the existing TJSV/TypeScript oracle,
then verifies its revision, validator pin, input hashes, complete fixture coverage
and every expectation. It executes the same corpus against all four real decoders.
An accepted frame must preserve its entire JSON value, including missing versus
explicit null members. Every valid result is encoded by each real runtime and
then decoded by all four runtimes, again requiring complete value preservation.
Those re-encoded values must equal the TJSV-admitted originals; this is not a
second, independently authored contract.

For the current 22-case corpus, the intended coverage is 88 direct decoder
observations and 112 producer-to-consumer observations (7 valid fixtures times
4 producers times 4 consumers), plus 27 failing-protocol controls across the
native probes. These are counts of observations, not a claim that a particular
commit passed. Only the corresponding successful hosted run and its receipt
establish passing results.

## Failure and evidence boundaries

Missing/duplicate/reordered responses, wrong runtime identity, extra fields,
non-boolean verdicts, missing decoded values, stale receipts and changed input
hashes fail closed. Native probes execute without a shell or inherited credential
environment, with time and output bounds. Malformed probe inputs must return
exit code 3 and no stdout; crashes cannot satisfy those negative controls.

The new receipt is `tmp/tjsv-cross-runtime.json`. It binds the exact source commit,
all selected tracked client/core/runner/contract files, compiled probe hashes and
the fresh TJSV oracle receipt. Input and executable hashes are rechecked after
execution. Existing receipts are never silently overwritten. These are unsigned
CI observations in a trusted runner, not cryptographic remote attestation or
proof against an attacker controlling the process/toolchain. Binary hashes bind
the observed executables; they are not claims of reproducible builds.

Native formatter output is checked after execution and must already be committed.
A first draft run may retain a format patch and formatted sources for correction;
a passing runtime step does not override that failed formatting gate. Merge
requires a clean rerun on the resulting final commit.

TypeSpec and authored JSON Schema stay independent authorities; neither source is
rewritten. Existing peer-authority and receipt-state audits remain mandatory.
This runtime gate does not itself prove compiler-backed TypeSpec equivalence,
universal validation equivalence, duplicate-member parity, live HTTP/TCP/WebSocket/
NATS transport behavior, operation-specific body validation, or production
rollout. Gleam, Bun, Deno and browser/WASM execution are not in this four-runtime
matrix. No deployment or package publication is implied.

## Reproduction

Build each probe into `tmp/tjsv-probes/{rust,go,dart}` as shown in the workflow,
with the reviewed TJSV checkout at `tmp/tjsv`, then run from a clean tracked tree:

```sh
node --test scripts/test_tjsv_rpc_admission.mjs scripts/test_tjsv_rpc_runtime_protocol.mjs
node scripts/tjsv-rpc-cross-runtime.mjs
```

Both Node entrypoints are fixed CI programs with no command-line options or
independent argv parsers. `tmp/` and `temp/` remain ignored. Preserve or explicitly
remove old receipts before another run. Unit harness tests use synthetic evidence
and must not be reported as native-runtime integration results.
