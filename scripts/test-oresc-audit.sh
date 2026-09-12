#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

calls="$tmp/calls.txt"
fake_oresc="$tmp/oresc"
cat >"$fake_oresc" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${ORESC_TEST_CALLS:?}"
report=''
while (($#)); do
  if [[ "$1" == '--report' ]]; then
    shift
    report="${1:?missing report path}"
    break
  fi
  shift
done
if [[ -n "$report" ]]; then
  mkdir -p "$(dirname "$report")"
  printf '{"status":"passed","producer":"fake-oresc"}\n' >"$report"
fi
SH
chmod +x "$fake_oresc"

report_dir="$tmp/reports"
(
  cd "$repo_root"
  ORESC_BIN="$fake_oresc" \
  ORESC_REPORT_DIR="$report_dir" \
  ORESC_TEST_CALLS="$calls" \
    bash scripts/oresc-audit.sh
)

mapfile -t actual <"$calls"
[[ ${#actual[@]} -eq 6 ]] || {
  printf 'expected 6 ores-cli invocations, got %s\n' "${#actual[@]}" >&2
  exit 1
}
[[ "${actual[0]}" == '--no-json audit repo --path . --profile standards' ]] || {
  printf 'unexpected repository audit invocation: %s\n' "${actual[0]}" >&2
  exit 1
}

names=(
  docs-discovery
  http-request-surface
  ores-rpc-config
  form-validation
  form-validation-admission-profiles
)
typespec_paths=(
  idl/typespec/docs-discovery.tsp
  idl/typespec/http/request-surface.tsp
  contracts/ores-rpc-config/typespec/main.tsp
  form-validation/contracts/main.tsp
  form-validation/admission-profiles/main.tsp
)
schema_paths=(
  json-schema/docs-discovery.schema.json
  json-schema/http-request-surface.schema.json
  contracts/ores-rpc-config/json-schema/ores-rpc-config.schema.json
  form-validation/contracts/authored.schema.json
  form-validation/admission-profiles/authored.schema.json
)

for index in "${!names[@]}"; do
  name="${names[$index]}"
  expected="--no-json audit contract --typespec ${typespec_paths[$index]} --schema ${schema_paths[$index]} --report $report_dir/$name.json"
  actual_index=$((index + 1))
  [[ "${actual[$actual_index]}" == "$expected" ]] || {
    printf 'unexpected %s contract audit invocation: %s\n' "$name" "${actual[$actual_index]}" >&2
    exit 1
  }
  [[ -s "$report_dir/$name.json" ]] || {
    printf 'expected %s receipt was not created\n' "$name" >&2
    exit 1
  }
done

set +e
missing_output="$(
  cd "$repo_root"
  ORESC_BIN="$tmp/does-not-exist" bash scripts/oresc-audit.sh 2>&1
)"
missing_status=$?
set -e
[[ $missing_status -eq 70 ]] || {
  printf 'missing ores-cli must exit 70, got %s\n' "$missing_status" >&2
  exit 1
}
[[ "$missing_output" == *'oresc is required'* ]] || {
  echo 'missing ores-cli error did not explain the dependency' >&2
  exit 1
}

printf 'api-docs oresc audit wrapper contract: ok\n'
