# MCP and client API-documentation discovery

Tracking: [ORESoftware/api-docs#7](https://github.com/ORESoftware/api-docs/issues/7) · Linear: `DEN-4078`

## Stable entry point

A service that mounts the `ores-api-docs` Axum router exposes:

```text
GET /api-docs/manifest.json
HEAD /api-docs/manifest.json
```

The response is the host-agnostic `DocsDiscoveryManifest` contract authored independently in:

- `idl/typespec/docs-discovery.tsp`; and
- `json-schema/docs-discovery.schema.json`.

Neither authority is generated from the other. Rust tests validate the emitted instance against the JSON Schema authority and compare every fixed route literal across the TypeSpec, JSON Schema, and Rust representations. A discrepancy stops promotion; no authority wins automatically.

## Why routes are relative

The manifest never constructs absolute URLs from `Host`, `Forwarded`, `X-Forwarded-*`, request scheme, or another inbound header. Those values may be attacker-controlled or may describe an internal proxy hop. The MCP server starts with an origin supplied through its own reviewed configuration, fetches the relative discovery path, and resolves only the returned relative paths against that trusted origin.

Consumers must reject a manifest route that:

- lacks a single leading `/`;
- starts with `//`;
- contains a scheme, authority, userinfo, backslash, query, fragment, control character, or dot-segment;
- escapes the configured service origin after URL resolution;
- redirects to a different origin without an explicit allowlist decision.

Do not copy authorization headers or cookies across an origin change. The documentation router itself does not reflect credentials in responses or diagnostics.

## Manifest fields

| Field | Meaning |
|---|---|
| `schemaVersion` | Discovery-envelope version; currently `1.0.0`. |
| `service` | Validated route-map service identifier. |
| `contractSha256` | SHA-256 of the normalized semantic RPC contract shared by the generated projections and language surfaces. |
| `routeCount` | Number of route-map operations represented by the catalog. |
| `discovery` | This manifest's canonical relative route. |
| `html` | Canonical interactive/static HTML route. |
| `catalog` | Full route-map catalog and embedded projections. |
| `projections.openapi` | OpenAPI 3.1 projection. |
| `projections.openrpc` | OpenRPC projection. |
| `projections.connect` | Connect JSON-unary projection. |
| `aliases` | Compatibility routes; consumers should prefer the canonical fields above. |

The manifest does not advertise write operations, deployment metadata, internal hosts, filesystem paths, tokens, or provider credentials.

## MCP query flow

An MCP integration should:

1. obtain a reviewed service origin and authentication policy from configuration, not from model text;
2. request the fixed discovery path with a bounded timeout, response size, redirect policy, and cancellation;
3. require HTTP 200 and JSON content type for `GET`; use `HEAD` only as an availability probe;
4. validate the complete response against the Draft 2020-12 schema;
5. require the expected discovery schema major version;
6. validate every relative route and same-origin resolution;
7. fetch the catalog or one declared projection with independent size and timeout limits;
8. verify that `contractSha256` agrees with the selected projection's `x-ores-rpc.contractSha256` values or the catalog's normalized contract evidence;
9. build a read-only tool/resource index from documented operations;
10. stop and report a structured discrepancy when service, route count, digest, schema, projection, or authority evidence disagrees.

The model must never invent a route omitted by the manifest or choose a projection because it returned first. A stale cached manifest may be used only under an explicit cache policy keyed by trusted origin, service, schema version, and contract digest.

## HTTP behavior

The discovery route uses the same hardening headers as other JSON documentation routes:

- `Cache-Control: no-store`;
- JSON content type;
- clickjacking/content-sniffing/referrer hardening inherited from the router;
- `GET` and `HEAD` only;
- deterministic `405` with `Allow: GET, HEAD` for `POST` and other unsupported methods;
- bounded, credential-free encode failure response.

The manifest is generated from the already validated in-memory `Catalog`; it does not reread files, consult environment variables, call a network, or use request headers.

## Fleet rollout boundary

This repository supplies the contract and Axum router. Issue #7 remains open until target Rust servers:

- pin an immutable `oresoftware/api-docs` revision/version containing the manifest;
- mount the router at the real application boundary without shadowing or rewriting the canonical path;
- demonstrate exact-head tests for `GET`, `HEAD`, `405`, headers, schema validation, relative routes, digest parity, cancellation, and bounded responses;
- publish a service inventory and MCP query receipt;
- update the corresponding Linear and GitHub Project evidence.

A merged contract PR here is not proof that every service has adopted or deployed it.
