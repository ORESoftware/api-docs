# TJSV admission of RPC v1 wire fixtures

The `TJSV RPC v1 admission` workflow executes the real
[ORESoftware/typespec-json-schema-validator](https://github.com/ORESoftware/typespec-json-schema-validator)
implementation against both independently authored RPC envelope schemas and
`examples/rpc-v1/conformance.json`. The same encoded fixtures pass through the
actual TypeScript `decodeCall` / `decodeReceipt` implementation and the native
Go client's `DecodeCall` / `DecodeReceipt` plus `Encode` methods. Every oracle
must match each fixture's expected verdict; agreement on the wrong result fails.
Successful decoding and Go re-encoding must preserve the entire JSON value,
including absent-versus-null fields. Object member order is not significant.

The shared corpus includes null rejection for every typed scalar envelope field.
In particular, `ok: null` is not boolean false. Explicit `body: null`, missing
optional status, and actual boolean false remain supported where the authored
schemas permit them. The Go adapter imports the public client; it does not
reimplement validation or receive the expected verdicts.

The validator is a full-SHA-pinned checkout with its original `package-lock.json`;
`npm ci --ignore-scripts` installs locked dependencies. The entrypoint verifies
the pin and rejects modified tracked validator files. It does not install an
unversioned npm package or duplicate TJSV's validator implementation.

This gate is an additional admission oracle, not a replacement for the independent
TypeSpec authority, strict RPC audits, receipt-state checks, digest-bound bundle,
SQL/Protobuf projections, or the Rust/Dart/Go/TypeScript runtime-conformance jobs.
TypeSpec and human-authored JSON Schema remain peer authorities. This gate does
**not** claim compiler-backed TypeSpec/JSON Schema equivalence or direct execution
of Rust or Dart. Those remain separately evidenced gates. This finite corpus is
not a proof of universal schema/runtime equivalence. TCP prefix metadata is
checked, but this runner does not open TCP/WebSocket/NATS connections, validate
operation-specific bodies, or replace transport/duplicate-member tests.

## Reproduction

From a clean repository checkout with Go installed, install the exact validator
revision recorded in `scripts/tjsv-rpc-admission.mjs` at `tmp/tjsv`, then run:

```sh
npm ci --prefix tmp/tjsv --ignore-scripts --no-audit --no-fund
node --test scripts/test_tjsv_rpc_admission.mjs scripts/test_tjsv_rpc_go.mjs
(cd clients/go && go vet ./... && go test -race ./...)
node scripts/tjsv-rpc-admission.mjs
```

The entrypoint freshly builds `tmp/tjsv-rpc-go` from the candidate on every run;
it never trusts a pre-existing binary. The Go build uses the installed toolchain,
with workspace files, GOFLAGS, user GOENV settings, module proxy access, and CGO
disabled. It fails rather than falling back to a different executable or runtime.
The workflow pins Go 1.27.1 and runs formatting, vet, and race-enabled tests.

Both fixed entrypoints accept no arguments. `tmp/` and `temp/` remain ignored.
The receipt is created exclusively at `tmp/tjsv-rpc-admission.json`; an existing
receipt is never silently overwritten. Preserve or explicitly remove that
previous receipt before another run.

The `ores.api-docs.tjsv-rpc-admission/v2` receipt records every fixture verdict,
all disagreement findings, the actual source revision, reviewed validator
revision, and SHA-256 digests of the schemas, TypeSpec source, corpus, runtime
manifest, TypeScript decoder, Go implementation and module, adapter, runners,
and workflow. It also records the native Go toolchain and executable SHA-256.
This receipt format version does not change the RPC v1 wire protocol.
Hashing TypeSpec binds the surrounding authority context; it is not a TypeSpec
validation claim. The runner rejects changed source bytes, changed candidate
revisions, a dirty Go source inventory, or changed executable bytes during
evaluation. Receipts have no clock value; binary reproducibility additionally
requires the same Go toolchain, platform, and build inputs. They are unsigned
test evidence, not release authorization or runtime attestation.

Native evidence must have a closed envelope, the exact case count and order,
matching names and kinds, boolean verdicts, and lossless accepted values. Missing,
duplicate, reordered, extra, or malformed results fail closed. The adapter's
control input is generated internally by the runner; it is not an externally
exposed RPC endpoint or a claim of strict arbitrary-wire JSON parsing.

Exit codes are 0 for agreement with every expectation, 2 for semantic drift, and
3 for malformed corpus/schema/evidence, missing tooling, build failure, validator
refusal/crash, unexpected runtime exceptions, pin mismatch, or filesystem failure.
Only TypeScript `RpcV1Error` and actual Go decoder error returns count as runtime
rejections. Encoder errors, native process crashes, timeouts, and invalid output
must never become successful negative evidence.

Harness unit tests use synthetic verdicts to test orchestration failure modes.
The native adapter tests additionally invoke the real Go client. Only the final
integration step runs TJSV, TypeScript, and the freshly built native Go client
together. These results must not be conflated when reporting verification.
