<!-- generated-policy: frozen -->

# Generated files — read-only

Do **not** hand-edit files in this directory. They are produced by tooling such as:

- `python3 scripts/generate-routes.py` (TypeScript, Dart, Rust, Gleam `RouteKey` objects from `examples/*.route-map.json`)
- https://github.com/flags-2-env/flags-2-env (typical Dart path: `generated/dart/env.dart`)
- JSON Schema / OpenAPI / route-map generators in this repository

```sh
python3 scripts/generate-routes.py
python3 scripts/generate-routes.py --check
python3 scripts/check-generated-contract.py --freeze --require-readonly
```

## Disk permissions

After generation, files here are frozen with `chmod a-w` (not writable). Directories
and this `README.md` stay writable so generators can replace files.

Git does **not** persist the write bit (only the executable bit). A fresh clone is
writable until you re-freeze:

```sh
python3 scripts/check-generated-contract.py --freeze --require-readonly
```

To regenerate, change the **primary source** (`.cli-flags.toml`, `examples/*.route-map.json`,
OpenAPI, `json-schema/*.schema.json`, …) and re-run the generator. Preferred generators thaw,
write, then `chmod a-w` themselves.

If `generated/` is listed in `.gitignore`, the artifacts stay local — still commit this
`README.md` (`git add -f generated/README.md` or a `.gitignore` exception) so the freeze vs
writable policy is visible in VCS.

## Runtime contract (not just compile-time)

JSON Schema is a **cross-check**, not always the primary generator input. Compile-time
`RouteKey` enums are not enough: `scripts/check-generated-contract.py` and
`scripts/check-route-sync.py` validate maps and fixtures at runtime (valid must pass,
invalid must fail). Call/receipt/telemetry fixtures live in
`tests/generated-contract/{valid,invalid}/` and are paired to `*.schema.json` by filename stem.

```sh
python3 scripts/check-generated-contract.py --freeze --require-readonly
```

opto-sync and ores-otel stay **decoupled**: this tree does not git- or zed-depend on them.
Route maps travel as opto-sync envelopes (`ores.api-docs.route-map`); RPC calls are not
opto-sync records. Telemetry is a small attribute bag ores-otel may copy into log-context
`fields`.
