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
operation under `json-schema/operations/`. These schemas validate the coerced
logical values after HTTP parsing and before a handler runs. Each schema is
closed and records:

```json
{
  "x-ores-routing-identity": ["method", "pathTemplate"],
  "x-ores-validation-only": ["path", "query", "headers", "body"]
}
```

CI regenerates the artifacts, checks drift, compiles generated targets, and
executes positive and mutation cases. Deploy pipelines should run the same
`ridl check`, `ridl drift`, and Draft 2020-12 validation suite before promotion.

## Peer authorities

`idl/typespec/http-request-surface.tsp` and
`json-schema/http-request-surface.schema.json` are independent, human-authored
peer authorities for the generic parsed envelope. Neither is generated from the
other. `scripts/check-http-request-surface-authorities.py` normalizes their
public field/required/method shapes and fails closed on disagreement.
