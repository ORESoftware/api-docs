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

audit_peer_contract() {
  local name="$1"
  local typespec="$2"
  local schema="$3"

  echo "[oresc] ${name} TypeSpec / JSON Schema peer-authority admission"
  "$ORESC_BIN" --no-json audit contract \
    --typespec "$typespec" \
    --schema "$schema" \
    --report "$REPORT_DIR/${name}.json"
}

# Both inputs below are independently human-authored first-class authorities.
# `oresc audit contract` delegates to canonical TJSV: TypeSpec is compiled to a
# comparison-only JSON Schema witness, that witness is compared with the authored
# Draft 2020-12 JSON Schema, and fresh Contract IR/receipt evidence is verified.
audit_peer_contract \
  docs-discovery \
  idl/typespec/docs-discovery.tsp \
  json-schema/docs-discovery.schema.json

audit_peer_contract \
  http-request-surface \
  idl/typespec/http/request-surface.tsp \
  json-schema/http-request-surface.schema.json

audit_peer_contract \
  ores-rpc-config \
  contracts/ores-rpc-config/typespec/main.tsp \
  contracts/ores-rpc-config/json-schema/ores-rpc-config.schema.json

audit_peer_contract \
  form-validation \
  form-validation/contracts/main.tsp \
  form-validation/contracts/authored.schema.json

audit_peer_contract \
  form-validation-admission-profiles \
  form-validation/admission-profiles/main.tsp \
  form-validation/admission-profiles/authored.schema.json

# Root Cargo.toml is a workspace-only manifest. Existing API-docs authority,
# projection, route-map and zed-package scripts remain authoritative for their
# broader multi-package checks until ores-cli models workspace packages.
echo "[oresc] workspace package audit deferred to repository-owned package gates"
