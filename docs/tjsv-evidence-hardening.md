# TJSV consumer evidence hardening — DEN-3830

Scope: `scripts/rpc-v1-projection-admission.mjs`, its file boundary, and the
pinned TJSV source used before importing the validator. This complements the
existing peer-authority, route/request-surface, digest-bound bundle, and runtime
conformance gates. It does not replace them or establish a new source authority.

## Reproduced failures

Against consumer blob `da9b30f591f2b0e6bccb73b98b69aa67782320ac`, the new
consumer tests reproduce two false-green policy audits: a configured input can
traverse an ancestor symlink outside the configured root; and malformed UTF-8
inside an otherwise valid JSON string is silently replaced by `Buffer.toString`.
The revised reader rejects both, without including the submitted evidence in
its JSON parse error. Terminal symlinks and hard links were already forbidden;
that restriction is preserved and extended to ancestor directories.

The original validator loader checked `git rev-parse HEAD` before importing
source. That does not prove the imported bytes match the pinned revision. The
new verifier compares every tracked file with its Git tree blob identity before
import, rejecting non-regular tracked entries and untracked/ignored `src` or
`bin` files. Tests include tracked edits hidden with `assume-unchanged` and
`skip-worktree`; the verifier does not rely on index-based status for integrity.

## File and output boundary

`projection-evidence-io.mjs` validates normalized relative POSIX paths, rejects
symlink ancestors below a canonical caller-owned root, and requires singly
linked regular files. Reads use `O_NOFOLLOW`, bounded buffers, descriptor
identity/size/time checks, and a final path recheck. JSON uses fatal UTF-8
decoding. Windows drive paths are rejected even on POSIX hosts.

Writes create only real parent directories and preserve unowned or malformed
existing outputs. A private, randomly named temporary file is exclusively
created, flushed, and renamed after rechecking the destination. The writer
never deletes a pre-existing predictable temporary path; cleanup is limited to
the temporary file it created.

These are defense-in-depth checks for **caller-owned, non-concurrently mutated
workspaces**, not a sandbox against an adversary with directory-write access.
Node's portable path APIs do not provide a fully atomic directory-handle-based
`openat`/rename transaction. Keep evidence roots and validator checkouts private
and immutable during admission. Source verification is not dependency-store
attestation: `npm ci`, lockfile integrity, trusted runtime/Git, and an isolated
runner remain separate requirements. This does not claim duplicate-JSON-key
rejection or a proof against every filesystem race.

## TJSV integration and tests

The consumer policy and CI pin TJSV to
`4473504c4c9d2831d825919f70c03994d8ce01d2`, whose upstream Linux and macOS test
jobs passed in run `34285583068`. Both policy pin occurrences and both workflow
pin occurrences move together; no floating branch is used to execute code.

Run the dependency-free boundary suite:

```sh
node --test scripts/test-projection-evidence-io.mjs
```

It has 39 tests, including two tests of the real consumer policy-audit API. The
same suite failed those two regression cases against the original consumer and
passes with the hardened reader. Synthetic Git repositories and evidence files
are isolated test fixtures, not user worktrees.

CI runs this suite on Linux and macOS, followed by the existing compiler-backed
TJSV `runCheck` / Contract IR / projection-manifest consumer suite against the
exact pinned checkout. It retains stale-receipt, TypeSpec-drift, output-drift,
blocked-activation, and CLI write/check tests. A green boundary test is not a
substitute for that real integration run or the repository-wide checks.

## Deliberately not activated

`idl/rpc-v1-projection-admission.policy.json` stays **blocked**. The policy audit
can report `policyValid: true` while still reporting
`projectionAdmissible: false`; those are different claims. Production projection
admission still needs the exact RPC v1 peer-authority receipt and Contract IR,
and ingress/egress semantic-validator evidence for the flattened Protobuf
receipt. TypeSpec and authored JSON Schema remain independent peers; no source,
generated projection, or approval is rewritten to manufacture a green result.

The existing consumer CLI's hand-written option parser is also a separate
migration task to the canonical flags-2-env boundary; this patch introduces no
new command-line interface and does not claim that migration is complete.
