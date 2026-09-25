# Deterministic API/MCP documentation publication modes

`ores-api-docs` has one renderer and two publication ownership modes. The modes do **not** create two API contracts. Both consume the same validated route map, `Catalog`, and semantic contract SHA-256.

## `publisher_external`

Use this mode when the platform owner publishes official external-developer documentation for its own service, for example `beamscale`, `fiducia-cloud`, or `scintilla-run`.

The generated `publication.json` declares:

- `publication_mode = publisher_external`
- `authority_scope = platform_external_developers`
- `publisher_provenance_required = true`

The generated files are not considered official merely because those strings are present. The publication boundary must attach and verify publisher-controlled provenance (for example the platform repository/release/CI attestation) before exposing the bundle as official platform documentation.

## `consumer_project`

Use this mode when an end user runs a platform CLI to document the user's own application or service.

The generated `publication.json` declares:

- `publication_mode = consumer_project`
- `authority_scope = project_owned`
- `publisher_provenance_required = false`

That output is authoritative only for the project that generated it. It must never be presented as BeamScale, Fiducia Cloud, Scintilla Run, or ORES Stack platform documentation.

## CLI adapters

Platform CLIs such as `scintilla`, `fiducia`, `bmscl`, and `ores-stack` are ingress adapters, not independent documentation renderers. They may discover or build a route map and choose an explicit publication identity, but the API projections, MCP discovery artifact, MCP server documentation, canonical ordering, and contract digest come from `ores-api-docs`.

This keeps CLI UX platform-specific without creating four subtly different documentation standards or four independent canonicalization paths.

## Determinism boundary

The renderer reads only:

1. an already validated `Catalog`,
2. an explicit publication mode, and
3. an explicit producer identity.

It does not read the clock, network, request host/forwarded headers, environment, Git checkout, or an LLM/model. Equal inputs therefore yield byte-identical output maps.

The bundle contains a fixed ordered inventory:

1. `api/catalog.json`
2. `api/openapi.json`
3. `api/openrpc.json`
4. `api/connect.json`
5. `api/hyper-schema.json`
6. `api/index.html`
7. `mcp/manifest.json`
8. `mcp/server.md`
9. `publication.json`

`mcp/manifest.json` is the existing relative, digest-bound MCP/API-docs discovery document. `mcp/server.md` is derived from the same catalog and identifies the publication ownership mode explicitly.

## Representative platform fixtures

The conformance suite keeps separate route-map fixtures for BeamScale, Fiducia Cloud, and Scintilla Run under `conformance/docs-publication/`. Their representative operations mirror the corresponding service implementations rather than proving determinism only by renaming one synthetic service fixture. The publication test renders each complete platform bundle twice and compares the output maps byte-for-byte.

These fixtures are proof inputs, not a second API authority. Platform-owned route contracts remain upstream; the publication fixture must be refreshed when its representative upstream surface changes.

## Required proof

For each supported platform/service fixture, generation must run at least twice from identical inputs and compare the complete output map byte-for-byte. Tests must also prove that changing `publisher_external` to `consumer_project` does not change semantic API projections or the MCP discovery manifest; only ownership/provenance-facing artifacts may differ.
