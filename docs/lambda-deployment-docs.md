# Lambda deployment documentation interchange

`api-docs` owns a provider-neutral deployment-documentation contract so deployable applications can describe runtime/placement compatibility without coupling documentation generation to `ores-stack`, BeamScale, or Scintilla internals.

Peer authorities:

- `idl/typespec/lambda-deployment-docs.tsp`
- `json-schema/lambda-deployment-docs.schema.json`

Neither authority is generated from the other. Contract admission must compare the two using the repository's normal TJSV/`oresc audit contract` path.

## Producer vs target

The manifest deliberately separates **producer** from **deployment target**.

Producers include:

- `ores-stack`
- `bmscl-compiler`
- `scintilla`
- `external`

Deployment targets currently include:

- `beamscale`
- `scintilla`

This means an `ores-stack` Rust application can advertise Scintilla compatibility without pretending `ores-stack` itself is a hosting platform. The semantic validator rejects an `ores-stack` function that advertises BeamScale. BeamScale compatibility additionally requires a portable `beam` / `beam_process` function.

Scintilla is intentionally broader: native, interpreted/JIT, BEAM and OCI workloads can all be documented when the emitted runtime/carrier fields match the admitted artifact.

## Deterministic generation

The Rust implementation in `rust/src/lambda_deployment_docs.rs`:

- validates the JSON instance against the authored Draft 2020-12 schema;
- rejects duplicate function ids, operation keys and deployment targets;
- enforces BeamScale/`ores-stack` compatibility invariants;
- sorts functions by id, operation keys lexicographically and deployment targets deterministically;
- renders stable pretty JSON or Markdown.

The CLI joins deployment metadata to the existing API route-map authority:

```sh
cargo run --manifest-path rust/Cargo.toml --bin ores-lambda-deployment-docs -- \
  path/to/service.route-map.json \
  path/to/lambda-deployment.json \
  markdown
```

Generation fails closed unless `service` and `contractSha256` exactly match the normalized route map. This prevents documentation from describing a different API revision than the server/client contract.

Use `json` as the final argument for normalized machine-readable output.

## Integration model

Each producer should emit the same manifest after its own artifact admission:

- `ores-stack`: emit one entry per built Scintilla function/native service using the existing content-addressed build/artifact receipts;
- `bmscl-compiler`: emit BEAM function entries after compiler/policy admission, normally targeting BeamScale and optionally Scintilla;
- Scintilla-native/container build paths: emit native/interpreted/OCI entries with exact runtime, carrier and artifact digest.

`api-docs` should consume those manifests; it should not call producer-specific build code. That keeps documentation deterministic and producer-independent while still permitting strong cross-checks.

## Placement is downstream

The manifest describes deployability and execution carrier, not a particular cluster instance. A Scintilla function can remain the same documented artifact whether Scintilla later places it on Kubernetes or the bare NixOS/process fleet. Region, cluster and host placement belong to deployment evidence rather than API documentation identity.
