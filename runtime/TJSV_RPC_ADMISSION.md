# TJSV admission of RPC v1 wire fixtures

The `TJSV RPC v1 admission` workflow executes the real
[ORESoftware/typespec-json-schema-validator](https://github.com/ORESoftware/typespec-json-schema-validator)
implementation against both independently authored RPC envelope schemas and
`examples/rpc-v1/conformance.json`. Identical encoded fixtures pass through:

- TJSV's schema-instance validator;
- the actual TypeScript `decodeCall` / `decodeReceipt` implementation;
- `ores-api-docs-client` from `clients/rust`, via the fixed Rust oracle; and
- the actual Go client through the already-reviewed fixed probe in
  `clients/go/testdata/tjsv_probe/main.go`.

Expected valid fixtures must be accepted and expected invalid fixtures rejected.
Agreement on the wrong result still fails. Successful native decoding must preserve
the complete JSON value, including absent-versus-null fields.

## Peer authorities and TJSV

TypeSpec and authored Draft 2020-12 JSON Schema remain independent, human-authored
peer authorities. This admission gate does not generate one from the other or
select a winner. TJSV is pinned by full Git commit and is an executable contract
oracle; its generated TypeSpec witness remains comparison evidence only. Existing
peer-authority, projection and receipt-state gates remain mandatory.

## Rust-stable receipt plus additive Go evidence

The top-level `ores.api-docs.tjsv-rpc-admission/v1` result and `sourceDigests`
shape stays compatible with the hardened Rust-bound oracle consumed by the
cross-runtime verifier. That compatibility is intentional: existing downstream
evidence cannot silently reinterpret a v1 receipt merely because Go was added.

Go is nevertheless a **blocking** runtime in the same admission run. Its closed
result set, source hashes, exact Go toolchain identity and compiled probe SHA-256
are recorded under `runtimeEvidence.go`. The top-level status and findings are
recomputed from the TJSV/TypeScript/Rust result plus Go evidence, so a Go mismatch
or decoded-value change prevents a passing receipt. `coverage` records Go as an
additional executed runtime while preserving the stable Rust-oracle identity
that the cross-runtime verifier already checks.

The Go evidence block binds the workflow, Go module/client/test sources, the fixed
probe, its dedicated harness and the shared runtime-protocol helper. Each working
file is compared with its Git blob, hashed before execution and rechecked after.
The compiled probe must remain a singly linked regular file with an unchanged
SHA-256. The build uses `GOENV=off`, `GOWORK=off`, `GOTOOLCHAIN=local`,
`GOPROXY=off`, `GOSUMDB=off`, `CGO_ENABLED=0`, a repository-owned temporary
HOME/cache, and no credential-bearing inherited environment beyond PATH.
The probe itself executes through the shared protocol with only `LANG` and
`LC_ALL` in its environment.

## Fail-closed evidence

The TJSV checkout keeps its original lockfile and is byte-verified before import
and again after execution. The verifier detects tracked modifications hidden from
`git status` by assume-unchanged/skip-worktree and rejects untracked validator
source. Consumer evidence is read through bounded no-link helpers and rechecked.

Rust and Go subprocesses must execute successfully and return complete, closed,
ordered, uniquely identified evidence. Verdicts are booleans. Accepted values
must round-trip without modification; rejected values cannot invent decoded
content. Missing/extra rows, stale source, malformed output, compiler/build
failure, timeout, panic/crash, encoder failure, or an unsupported TJSV keyword is
an execution failure or `STOPPED_FOR_EVALUATION`, never a successful rejection.

## Relationship to the four-runtime gate

`TJSV RPC cross-runtime` separately executes TypeScript, Rust, Go and Dart and
performs all-to-all encoder/decoder checks. It remains a distinct broader gate.
Adding Go to the base oracle provides earlier direct Go admission and reusable
source-bound evidence; it does not replace Dart execution or the cross-runtime
matrix. The broader gate also independently binds all current client sources,
which gives a second integration check over this additive Go evidence.

## Reproduction

From a clean checkout with the exact TJSV revision in
`scripts/tjsv-rpc-admission.mjs` checked out at `tmp/tjsv`, use the toolchains
pinned by `.github/workflows/tjsv-rpc-admission.yml` and run:

```sh
npm ci --prefix tmp/tjsv --ignore-scripts --no-audit --no-fund
node --test scripts/test_tjsv_rpc_admission.mjs scripts/test_tjsv_rust_admission.mjs scripts/test_tjsv_go_admission.mjs scripts/test_tjsv_rpc_runtime_protocol.mjs scripts/test-tjsv-rpc-entrypoint.mjs scripts/test-projection-evidence-io.mjs
cargo test --locked --manifest-path clients/rust/Cargo.toml --example tjsv_admission
(cd clients/go && GOENV=off GOWORK=off GOTOOLCHAIN=local GOPROXY=off go vet ./... && GOENV=off GOWORK=off GOTOOLCHAIN=local GOPROXY=off go test -race ./...)
node scripts/tjsv-rpc-admission.mjs
```

The fixed admission entrypoint accepts no options and introduces no independent
flag parser. Receipts are create-only at `tmp/tjsv-rpc-admission.json`; an existing
output is never silently replaced. `tmp/` and `temp/` remain ignored.

## Scope limits

This finite corpus is not a proof of universal schema/runtime equivalence. The
runner does not open live HTTP/TCP/WebSocket/NATS connections, certify arbitrary
raw JSON duplicate-member behavior, validate operation-specific application
bodies, prove compiler-backed TypeSpec/JSON Schema equivalence, publish packages,
or authorize deployment. Dart, browser/WASM and additional SDK languages retain
their own executable gates. Harness tests using synthetic evidence test
orchestration only and must not be reported as native integration results.
