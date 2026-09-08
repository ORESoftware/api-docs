# RPC v1 projection admission

The RPC v1 generator already produces a deterministic, Buf-checked Proto, gRPC metadata, and SQL projection from `idl/rpc-v1.projection.json` plus the permanent field-number lock. This document defines the additional consumer boundary introduced by `typespec-json-schema-validator` issue 20.

The projection manifest is evidence, not another contract authority. TypeSpec and independently authored JSON Schema remain peer authorities. The generated TypeSpec JSON Schema is comparison evidence only, and the Contract IR is non-editable compiler data admitted only after an exact-input parity pass.

## Consumer command

`node scripts/rpc-v1-projection-admission.mjs` has three modes:

```sh
# Validate the reviewed policy and its explicit blocked/enabled state.
node scripts/rpc-v1-projection-admission.mjs \
  --mode=audit-policy \
  --root=. \
  --validator-root=_tools/typespec-json-schema-validator \
  --policy=idl/rpc-v1-projection-admission.policy.json

# After the product gates below are complete, produce the exact manifest/report.
node scripts/rpc-v1-projection-admission.mjs \
  --mode=write \
  --root=. \
  --validator-root=_tools/typespec-json-schema-validator \
  --policy=idl/rpc-v1-projection-admission.policy.json \
  --contract-ir=generated/rpc-v1/contract-ir.json \
  --parity-receipt=generated/rpc-v1/parity-receipt.json \
  --typespec=idl/typespec/v1.tsp \
  --generated-schema=generated/rpc-v1/typespec.generated.schema.json \
  --authored-schema=generated/rpc-v1/authored-authority

# Re-hash every input, tool source, and output and reject stale evidence.
node scripts/rpc-v1-projection-admission.mjs \
  --mode=check \
  --root=. \
  --validator-root=_tools/typespec-json-schema-validator \
  --policy=idl/rpc-v1-projection-admission.policy.json \
  --contract-ir=generated/rpc-v1/contract-ir.json \
  --parity-receipt=generated/rpc-v1/parity-receipt.json \
  --typespec=idl/typespec/v1.tsp \
  --generated-schema=generated/rpc-v1/typespec.generated.schema.json \
  --authored-schema=generated/rpc-v1/authored-authority
```

The validator checkout must be a Git worktree at the exact 40-character revision pinned in the policy. A branch name, tag, floating package range, or different checkout is rejected even when its files happen to look compatible.

## Evidence that is independently rechecked

The consumer reconstructs the Contract IR from the supplied receipt and the current TypeSpec, generated-witness, and authored-schema files. It does not trust the Contract IR self-digest alone. After that verification succeeds, it binds:

- the current Contract IR and parity receipt;
- the operation inventory (`idl/rpc-v1.projection.json`);
- the permanent field-number and identity ledger (`idl/protobuf.lock.json`);
- the emitter policy/configuration;
- an aggregate digest of the exact generator source closure;
- an aggregate digest of the exact validator source closure at the pinned commit;
- the complete admitted declaration set;
- every required projection target;
- the actual bytes, size, media type, and owning target for the Proto, gRPC metadata, and SQL outputs;
- independently reviewed representation-loss records; and
- executable runtime-validator, negative-fixture, and complete ingress/egress coverage evidence.

The manifest grades none of those claims itself. Trusted inputs, output re-hashing, loss approvals, and runtime evidence are supplied separately to `verifyProjectionManifest`.

## Current activation state

`idl/rpc-v1-projection-admission.policy.json` is deliberately `blocked`. A green policy audit means only that the blocked state is internally coherent and no stale green manifest/report remains. It explicitly returns:

```text
policyValid = true
projectionAdmissible = false
activationStatus = blocked
```

Two product gates remain:

1. Export a current validator-native parity receipt and Contract IR for the exact RPC v1 peer-authority closure. Existing custom audits remain useful independent evidence, but they are not silently relabeled as the validator receipt.
2. Generate or wire concrete Protobuf/gRPC/Connect adapters and prove that the receipt-state semantic validator executes on every applicable decode and encode path. SQL constraints already execute and are tested, but SQL evidence does not prove adapter ingress/egress coverage.

The blocked approval and runtime evidence documents preserve that distinction:

- `idl/rpc-v1-projection-delta-approvals.json`
- `runtime/rpc-v1-projection-validator-evidence.json`

The historical prose ledger in `idl/rpc-v1-receipt-state-deltas.json` is not discarded. It remains the source for a future exact approval record, but a prose reason or `reviewed: true` flag is not sufficient. The enabled record must bind the current Contract IR declaration assertion digest, exact source pointer, immutable approval digest, validator artifact, negative fixture corpus, and full boundary-coverage digest.

## Safety and preservation

The CLI accepts normalized relative POSIX paths and singly linked regular files only. It rejects traversal, absolute paths, backslashes, symbolic links, hard links, duplicate identities, unsupported properties, malformed JSON, stale source closures, copied receipts, wrong validator commits, output drift, unmanifested files, and attempts to replace unrelated output documents. Diagnostics are bounded and do not echo malformed file contents.

`--write` uses atomic replacement and writes only validator-owned manifest/report schemas. `--check` recomputes all evidence and compares canonical JSON. Neither mode changes TypeSpec, JSON Schema, the projection recipe, the protobuf lock, generated Proto numbers, SQL, or runtime policy to make a gate pass.

## Ownership boundary

This slice is the `api-docs` consumer for the merged validator projection-admission API. It does not claim that the full operation compiler, streaming gRPC/Connect generator, tRPC adapter generator, or every language target is complete. Those remain separate issue-20 acceptance items and must extend the existing digest-bound bundle rather than create parallel authorities or independent business logic.
