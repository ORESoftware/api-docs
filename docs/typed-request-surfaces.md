# Typed HTTP request surfaces

ORES route contracts distinguish **operation identity** from request validation:

- routing identity is exactly HTTP method plus URL path/template;
- path variables, query parameters, request headers, and JSON bodies are typed
  validation inputs and may never select a different operation;
- duplicate method+path slots are a build veto even when their query or header
  schemas differ.

## Compile-time path

RIDL v2 accepts `path_params`, `query_params`, `header_params`, and a typed
`request` record. The eight generated language surfaces carry all four inputs
into the transport request. Header names are retained exactly on the wire while
language identifiers are derived deterministically.

Headers are limited to canonical lower-case HTTP tokens with scalar, enum, or
list-of-scalar values. Authentication, cookies, tracing, proxy forwarding,
content framing, and hop-by-hop headers remain runtime-owned and cannot be
introduced by a business route map.

## Runtime and pre-deploy path

The JSON Schema emitter writes one Draft 2020-12 parsed-request schema per
operation under `examples/generated/json-schema/operations/`. These schemas
validate the coerced logical values after HTTP parsing and before a handler
runs. Each schema is closed and records:

```json
{
  "x-ores-routing-identity": ["method", "pathTemplate"],
  "x-ores-validation-only": ["path", "query", "headers", "body"]
}
```

CI regenerates the artifacts, checks drift, compiles generated targets, and
executes positive and mutation cases. Deploy pipelines should run the same
`ridl check`, `ridl drift`, Draft 2020-12 validation, and TJSV admission suite
before promotion.

## Independent peer authorities

`idl/typespec/http/request-surface.tsp` and
`json-schema/http-request-surface.schema.json` are independent, human-authored
peer authorities for the generic parsed envelope. Neither authority is
generated from the other.

The TypeSpec source uses the official `@typespec/json-schema` decorators only to
state its own JSON Schema semantics, including the closed envelope, canonical
header-key pattern, and the routing/validation annotations. The authored JSON
Schema remains separately editable and authoritative. TypeSpec-generated JSON
Schema is comparison evidence only.

Both sources declare the same `HttpMethod` and `RequestSurface` shapes. The
JSON Schema root resolves to the authored `RequestSurface` definition for
runtime use. Record-valued path, query, and header members stay open to typed
operation-specific keys, while the outer request envelope is closed.

## TJSV admission

The permanent `request-surface-contracts` workflow checks out
`ORESoftware/typespec-json-schema-validator` at the reviewed immutable revision
`d60d0d79d83e075077382623ec9e23a401ab601f` and runs its canonical `tjsv check`
path against the two authored sources.

Admission requires all of the following at the same candidate commit:

- official TypeSpec compilation and JSON Schema emission;
- Draft 2020-12 structural validation;
- identical declaration inventory and normalized semantics;
- zero unexplained TJSV findings;
- differential agreement over synthesized probes;
- agreement over the checked-in valid/invalid `RequestSurface` corpus;
- a non-empty digest-bound Contract IR and retained parity report.

`idl/http-request-surface.expected-deltas.json` is intentionally empty. A new
representation waiver is not an automatic escape hatch: it is a release veto
until the authorities are reconciled or a separately reviewed architecture
change updates the invariant and tests.

`scripts/check-http-request-surface-authorities.py` remains a focused
domain-policy gate beside TJSV. It enforces exact routing identity, validation
metadata, TypeSpec decorators, JSON Schema field shapes, zero active deltas,
and the prohibition on query/header/body dispatch selectors.
