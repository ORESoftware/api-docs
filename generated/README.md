<!-- generated-policy: frozen -->

# Generated compile-time route objects — read-only

Do **not** hand-edit these files. They are produced from `examples/*.route-map.json`:

```sh
python3 scripts/generate-routes.py
python3 scripts/generate-routes.py --check
python3 scripts/check-generated-contract.py --freeze --require-readonly
```

The generator writes TypeScript, Dart, Rust, and Gleam, then `chmod a-w` on those files. This `README.md` stays writable. Git does **not** persist the write bit, so re-run the checker with `--freeze` after clone.

If `generated/` is listed in `.gitignore`, the artifacts stay local — still commit this `README.md` (`git add -f generated/README.md` or a `.gitignore` exception) so the freeze vs writable policy is visible in VCS.

JSON Schema in `json-schema/route-map.schema.json` is a **runtime cross-check** of the route-map JSON (the primary generator input). Compile-time `RouteKey` enums are not enough: `scripts/check-generated-contract.py` and `scripts/check-route-sync.py` validate maps and fixtures at runtime. Call/receipt/telemetry fixtures live in `tests/generated-contract/{valid,invalid}/` and are paired to `*.schema.json` by filename stem.

opto-sync and ores-otel stay **decoupled**: this tree does not git- or zed-depend on them. Route maps travel as opto-sync envelopes (`ores.api-docs.route-map`); RPC calls are not opto-sync records. Telemetry is a small attribute bag ores-otel may copy into log-context `fields`.
