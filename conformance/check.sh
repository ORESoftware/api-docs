#!/bin/sh
set -eu
root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
cd "$root"

fail(){ echo "[api-docs conformance] $*" >&2; exit 1; }
for boundary in contracts conformance; do
  [ -d "$boundary" ] && [ ! -L "$boundary" ] || fail "$boundary/ must be a real non-symlink directory"
done
escaped=$(find contracts conformance -type l -print -quit 2>/dev/null || true)
[ -z "$escaped" ] || fail "symlink inside contract/conformance boundary: $escaped"
command -v zed >/dev/null 2>&1 || fail "zed is required so TJSV is resolved through the package graph"

# TJSV is part of the package graph, while api-docs retains its stronger fixed
# RPC oracle and consumer-lock integrity checks below.
zed run tjsv doctor --quiet
node scripts/check-tjsv-consumer-lock.mjs
cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check -- cross-check-rpc-idl
cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check -- audit-rpc-idl
python3 scripts/audit-rpc-v1-state.py
cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check -- rpc-contract-bundle --check
python3 scripts/test_rpc_v1_runtime.py
node scripts/tjsv-rpc-admission.mjs
