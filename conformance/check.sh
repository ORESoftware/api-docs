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

# api-docs deliberately keeps .zpkg.toml runtime dependencies empty. TJSV is
# admitted through the repository's exact consumer lock and fixed RPC oracle,
# so Zed can execute this canonical checker during install/build/test/pack
# without introducing a registry dependency or weakening pinned provenance.
node scripts/check-tjsv-consumer-lock.mjs
cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check -- cross-check-rpc-idl
cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check -- audit-rpc-idl
python3 scripts/audit-rpc-v1-state.py
cargo run --quiet --locked --manifest-path rust/Cargo.toml --bin api-docs-check -- rpc-contract-bundle --check
python3 scripts/test_rpc_v1_runtime.py
node scripts/tjsv-rpc-admission.mjs
