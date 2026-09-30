# WIT producer inventory

Tracking: #316.

| Producer | Output | Classification | Release rule |
| --- | --- | --- | --- |
| `scripts/generate-rpc-v1-formal-projections.mjs` | `generated/rpc-v1/wit/ores-rpc-v1.wit` | generated downstream semantic evidence | stale-generation + syntax parse + ores-wit/TJSV admission |
| `rust/src/module_interface_codegen.rs::render_wit` | `ores:module-contract@1.0.0` package / `guest-module` world | generated downstream semantic evidence | syntax parse + provenance + ores-wit/TJSV admission |

Neither producer is a third authored structural authority. The admitted TypeSpec/authored Draft 2020-12 JSON Schema/Contract-IR closure remains upstream.

All release-relevant generated WIT uses one reviewed `wasm-tools` identity (currently 1.259.0), exact generated-tree stale checks, and evidence that binds source/generator revision, WIT source/canonical digest, Contract IR closure, normalized projection digest, baseline digest, and TJSV receipt.

Byte-oriented `list<u8>` fields are transport/component representation boundaries only; codec/stream semantics remain explicit upstream and may not be erased by the WIT container.