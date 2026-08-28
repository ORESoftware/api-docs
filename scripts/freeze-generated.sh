#!/usr/bin/env bash
# Re-apply chmod a-w on frozen generated files (repo-root generated/ and nested
# trees such as examples/generated/). Git only stores the executable bit, so
# clones come back writable (644). Generators (`f2e generate`, `ridl generate`)
# should freeze after write; this script does the same after checkout via the
# Python contract checker (HTML policy comments, require-readonly, schema).
set -euo pipefail
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec python3 "$ROOT/scripts/check-generated-contract.py" --root "$ROOT" --freeze --require-readonly "$@"
