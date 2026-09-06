// Human-authored TypeSpec peer for parsed HTTP request validation.
// JSON Schema peer: json-schema/http-request-surface.schema.json.
// Operation selection is method + pathTemplate only. All remaining fields are
// validation inputs and may never select an operation.

namespace Ores.Http.RequestSurface.V1;

enum HttpMethod {
  GET,
  POST,
  PUT,
  PATCH,
  DELETE,
  HEAD,
  OPTIONS,
}

model RequestSurface {
  method: HttpMethod;

  @minLength(1)
  @pattern("^/[^\\s]*$")
  pathTemplate: string;

  path?: Record<unknown>;
  query?: Record<unknown>;
  headers?: Record<unknown>;
  body?: unknown;
}
