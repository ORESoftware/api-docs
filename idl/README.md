# Dual-primary RPC IDL

Same idea as persistence in `*-lib-core` and `declarative-migrations`: **two
independently authored primaries** plus a third wire-identity ledger. Generators
disagree in small, predictable ways. Those disagreements are **catalogued**.
Unexpected disagreement **vetoes** a release. Nothing auto-wins.

| Tier | Source | Role |
| --- | --- | --- |
| **P0** | Authored TypeSpec in `idl/typespec/` | Semantic AST / compiler lineage |
| **P1** | Authored JSON Schema in `json-schema/` | Human witness with **release veto**. Never overwritten by TypeSpec. |
| **P2** | Authored Protobuf in `idl/protobuf/` | Stable field numbers for framed / Connect-adjacent encodings |

`declarative-migrations` (`dpm`) is the SQL apply engine, not an RPC author.
This crate still does **not** open NATS, opto-sync, or ores-otel.

## Do not mix stacks

- v1 unary: `rpc-call` / `rpc-receipt` (`op`)
- v2 ridl: `rpc-frame` (`t`)
- HTTP never uses the v2 envelope (HTTP already has method/path/body)

## Workflow

1. Change the **same semantic fact** in TypeSpec **and** JSON Schema (and proto
   if it is a new field).
2. Run `python3 scripts/cross-check-rpc-idl.py`.
3. If the report shows a new delta, either fix a primary or add it to
   `idl/expected-deltas.json` with a reason (lossy edge, not a silent skip).
4. New proto fields **append** numbers in `idl/protobuf.lock.json`. Never reuse.

```sh
python3 scripts/cross-check-rpc-idl.py
python3 scripts/test_cross_check_rpc_idl.py -v
```

Optional TypeSpec compile (Node, CI only — not required for the Python gate):

```sh
npm --prefix idl/typespec ci
npx --prefix idl/typespec tsp compile idl/typespec
```

Projected JSON Schema from TypeSpec, if emitted, lives under
`generated/idl/witnesses/` and **must not** replace `json-schema/*.schema.json`.
