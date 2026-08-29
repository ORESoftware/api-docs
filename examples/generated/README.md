# `generated/` — frozen artifacts (read-only)

This tree is **generated** by [`ridl`](https://github.com/oresoftware/api-docs)
(`ridl generate`) and/or JSON Schema projections. Do not hand-edit adapters.

## Read-only on disk

After generate, artifact files are `chmod a-w` (0444). Git does not store
the Unix write bit (only 100644 vs 100755), so clones come back writable.
Restore with `ridl generate` or `scripts/freeze-generated.sh`.

## JSON Schema (the contract)

`json-schema/` (here or in the api-docs repo) is JSON Schema 2020-12.
Compile-time types are generated from the route map; runtime `validate()` /
schema checks must pass on real payloads. Unit tests should include valid
and invalid instances (missing required fields, wrong types, extra keys).

```sh
ridl check
ridl drift
python3 scripts/check-route-sync.py
```
