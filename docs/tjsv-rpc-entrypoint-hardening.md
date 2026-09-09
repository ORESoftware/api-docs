# TJSV RPC v1 entrypoint hardening — DEN-3830

The RPC admission job executes the real
`ORESoftware/typespec-json-schema-validator` source at reviewed revision
`d60d0d79d83e075077382623ec9e23a401ab601f` against the authored call/receipt
schemas and the shipped TypeScript decoder. This change hardens that existing
entrypoint; it does not introduce another validator or replace either contract
authority.

## Source and evidence boundaries

The entrypoint reuses `verifyValidatorSource` before dynamic import and after
evaluation. A matching HEAD and clean `git status` are not sufficient: Git index
flags can hide edits, and ignored files can add executable source. The shared
verifier checks tracked bytes against pinned Git blobs and refuses untracked
files under `src/` and `bin/`, including ignored files. The installed dependency
boundary remains the workflow's locked `npm ci --ignore-scripts`; source-byte
verification does not independently attest installed dependencies.

All declared input snapshots and their post-evaluation checks use
`readSafeBytes`. Each file must be a singly linked regular file, no larger than
8 MiB, beneath real directories. Link and file-identity checks reject changed
or redirected evidence. Corpus and schema bytes are decoded as strict UTF-8;
malformed bytes cannot be silently replaced with U+FFFD and treated as different
but valid evidence. Both shared helper sources are included in receipt digests.

Receipt publication uses the shared bounded, staged writer with an empty set
of replaceable schemas. Publication is therefore create-only: an existing
receipt, even one with this tool's own schema identifier, is preserved and
causes failure. Output symlinks are refused. Temporary output is private to the
writer and is cleaned without removing an existing receipt.

These checks detect specific source/evidence corruption and observed file
changes. They are not a sandbox or proof against a privileged concurrent
writer. CI must still use an isolated, trusted runner and immutable dependency
inputs.

## Regression evidence

Run the dependency-free orchestration suites with Node 22.9 or newer:

```sh
node --test scripts/test_tjsv_rpc_admission.mjs scripts/test-tjsv-rpc-entrypoint.mjs
```

The entrypoint tests create owned temporary Git repositories and **synthetic
oracles**, with a local revision substituted only in the copied test runner.
There is no production pin-override flag or environment variable. These tests
exercise the real entrypoint and IO helpers, not actual TJSV schema semantics.
The existing CI job separately installs and executes the real pinned TJSV.

The new cases cover clean deterministic receipts, hidden tracked edits,
ignored extra source, wrong revisions, changes during module evaluation,
linked and oversized evidence, malformed UTF-8, existing receipts, output
symlinks, unknown arguments, and runtime disagreement. Against the old
entrypoint, four selected cases reproduced false passes: assume-unchanged,
skip-worktree, ignored source, and malformed UTF-8. The patched entrypoint and
existing harness pass all 52 tests locally on Node 22.16.0.

## Scope and integration

TypeSpec and authored JSON Schema remain independent peer authorities. Their
parity and projection gates remain separate. This finite-corpus admission
receipt does not prove universal equivalence, service-specific body contracts,
native-language execution, or real network transport behavior.

When integrating native-runtime admission changes, preserve the byte verifier,
bounded snapshot reads, helper digest binding, create-only output policy, and
entrypoint regressions alongside the native oracle and its source inventory.
Neither side may be discarded merely to resolve a textual merge conflict.
