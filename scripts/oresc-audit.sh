#!/usr/bin/env bash
set -euo pipefail

ORESC_BIN="${ORESC_BIN:-oresc}"
REPORT_DIR="${ORESC_REPORT_DIR:-target/oresc-audit}"

if ! command -v "$ORESC_BIN" >/dev/null 2>&1; then
  echo "oresc is required; install the canonical ORESoftware/ores-cli package before running this audit" >&2
  exit 70
fi

mkdir -p "$REPORT_DIR"

echo "[oresc] repository standards"
"$ORESC_BIN" --no-json audit repo --path . --profile standards

echo "[oresc] docs-discovery TypeSpec / JSON Schema peer-authority admission"
"$ORESC_BIN" --no-json audit contract \
  --typespec idl/typespec/docs-discovery.tsp \
  --schema json-schema/docs-discovery.schema.json \
  --report "$REPORT_DIR/docs-discovery.json"

# Root Cargo.toml is a workspace-only manifest. Existing API-docs authority,
# projection, route-map and zed-package scripts remain authoritative for their
# broader multi-package checks until ores-cli models workspace packages.
echo "[oresc] workspace package audit deferred to repository-owned package gates"
