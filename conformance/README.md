# Conformance

`conformance/check.sh` is the canonical Zed package-lifecycle checker for the shared RPC/RIDL authority in this repository. It verifies that TJSV is available from the Zed dependency graph, validates the pinned TJSV consumer lock, cross-checks RPC IDL authorities, audits v1 state and bundle drift, runs the runtime fixture suite, and executes the fixed multi-runtime TJSV admission oracle.

The broader release/test surface remains in `.zpkg.toml` `scripts.test`; current Zed runs this canonical checker automatically around package lifecycle operations when the paired `contracts/` + `conformance/` boundary is present.
