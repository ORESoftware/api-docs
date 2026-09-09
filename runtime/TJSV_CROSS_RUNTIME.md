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

`scripts/tjsv-rpc-cross-runtime.mjs` first verifies the independently produced
runtime-toolchain receipt, then runs and verifies the existing TJSV/TypeScript/Rust
oracle. It checks exact source revision, reviewed validator pin, input hashes,
complete fixture coverage and every expectation before executing the same corpus
against all four real decoders. An accepted frame must preserve its entire JSON
value, including missing versus explicit null members. Every valid result is encoded
by each real runtime and then decoded by all four runtimes, again requiring complete
value preservation. Those re-encoded values must equal the TJSV-admitted originals;
this is not a second, independently authored contract.

For the current 39-case corpus, the intended direct coverage is 156 decoder
observations (39 fixtures times 4 runtimes). Producer-to-consumer coverage is
computed from the valid fixture count at execution time, and the native probes
must also reject the malformed-protocol controls. These are counts of observations,
not a claim that a particular commit passed. Only the corresponding successful
hosted run and its receipts establish passing results.

## Runtime toolchain identity

`scripts/tjsv-rpc-runtime-toolchains.mjs` records a create-only receipt,
`tmp/tjsv-runtime-toolchains.json`, before the four-runtime runner executes. It
captures the actual Node, Rust, Go and Dart version command output with fixed
no-shell invocations, parses the semantic version from each bounded one-line
result, and compares it with the exact versions pinned in the workflow. Missing,
extra, malformed, unrecognized or version-drifted toolchain evidence fails closed;
a command failure is infrastructure failure rather than negative contract evidence.

The toolchain receipt binds the exact source revision, reviewed TJSV revision,
workflow, shared corpus, runtime protocol, cross-runtime runner, evidence-IO helper,
and its own test/implementation bytes. Its expected version ledger and exact source
closure are immutable and adversarially tested. The verifier rejects unknown receipt
fields, a stale commit, a different TJSV repository/revision, missing/extra/stale
source digests, changed raw or parsed runtime versions, nonempty findings and evidence
that overclaims reproducible-build attestation or universal equivalence.

The cross-runtime runner now requires that receipt as an input rather than merely
retaining it beside the main evidence. It verifies the receipt against current source
bytes before running the codecs, rechecks that its bytes did not change afterward,
and publishes its SHA-256 plus the verified Node/Rust/Go/Dart versions inside the
`ores.api-docs.tjsv-cross-runtime/v2` receipt. A copied, stale or swapped toolchain
receipt therefore cannot independently accompany a passing cross-runtime receipt.

Toolchain identity is evidence about the executed compiler/interpreter environment.
It is not remote attestation, a reproducible-build proof, or a new contract authority.
TypeSpec and authored JSON Schema remain the independent peer authorities; TJSV and
all runtime/toolchain receipts remain derived admission evidence.

## Failure and evidence boundaries

Missing/duplicate/reordered responses, wrong runtime identity, extra fields,
non-boolean verdicts, missing decoded values, stale receipts and changed input
hashes fail closed. Native probes execute without a shell or inherited credential
environment, with time and output bounds. Malformed probe inputs must return
exit code 3 and no stdout; crashes cannot satisfy those negative controls.

The main runtime receipt is `tmp/tjsv-cross-runtime.json`. Version 2 binds the exact
source commit, all selected tracked client/core/runner/contract/toolchain files,
compiled probe hashes, the fresh TJSV oracle receipt, and the verified runtime-
toolchain receipt hash and versions. Input, executable, oracle and toolchain-receipt
hashes are rechecked after execution. Existing receipts are never silently overwritten.
These are unsigned CI observations in a trusted runner, not cryptographic remote
attestation or proof against an attacker controlling the process/toolchain. Binary
hashes bind the observed executables; they are not claims of reproducible builds.

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
node --test scripts/test_tjsv_rpc_admission.mjs scripts/test-tjsv-rpc-entrypoint.mjs scripts/test_tjsv_rpc_runtime_protocol.mjs scripts/test_tjsv_rpc_runtime_toolchains.mjs
node scripts/tjsv-rpc-runtime-toolchains.mjs
node scripts/tjsv-rpc-cross-runtime.mjs
```

All Node entrypoints are fixed CI programs with no command-line options or
independent argv parsers. `tmp/` and `temp/` remain ignored. Preserve or explicitly
remove old receipts before another run. Unit harness tests use synthetic evidence
and must not be reported as native-runtime integration results.

## Hardened oracle receipt compatibility

The closed `ORACLE_INPUTS` manifest is shared by receipt verification and source
snapshot selection. It requires the complete current oracle closure, including
`scripts/tjsv-source-integrity.mjs` and `scripts/projection-evidence-io.mjs` from
the hardened base oracle. Both helper hashes must match current candidate bytes;
accepting an arbitrary extra hash or checking only an older subset would bypass
that evidence boundary. Legacy incomplete receipts, missing helpers, stale helpers
and absent current snapshots are rejected by regression tests.

The workflow triggers on both helper files and the base entrypoint regressions,
and executes those regressions alongside the runtime and toolchain protocol harnesses.
Native probes, all-to-all codec checks, strict result fields and the reviewed TJSV pin
remain mandatory. Harness tests are orchestration evidence; a fresh actual four-runtime
run must still pass on the exact integrated commit.
