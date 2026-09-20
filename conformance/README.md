# Conformance

`conformance/check.sh` is the canonical Zed package-lifecycle checker for the shared RPC/RIDL authority in this repository. It validates the exact pinned TJSV consumer lock, cross-checks RPC IDL authorities, audits v1 state and bundle drift, runs the runtime fixture suite, and executes the fixed multi-runtime TJSV admission oracle.

TJSV intentionally remains outside `.zpkg.toml [dependencies]`: `api-docs` has an existing dependency-boundary contract requiring that block to stay empty, while its TJSV revision/provenance is already locked and verified by repository-owned tooling. Current Zed still executes this canonical checker automatically around package lifecycle operations when the paired `contracts/` + `conformance/` boundary is present.

The broader release/test surface remains in `.zpkg.toml` `scripts.test`.
