// Human-authored TypeSpec peer for parsed HTTP request validation.
// JSON Schema peer: json-schema/http-request-surface.schema.json.
// Operation selection is method + pathTemplate only. All remaining fields are
// validation inputs and may never select an operation.

import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace Ores.Http.RequestSurface.V1;

@jsonSchema
@id("HttpMethod")
enum HttpMethod {
  GET,
  POST,
  PUT,
  PATCH,
  DELETE,
  HEAD,
  OPTIONS,
}

@jsonSchema
@id("RequestSurface")
@extension("unevaluatedProperties", false)
@extension("x-ores-routing-identity", #["method", "pathTemplate"])
@extension("x-ores-validation-only", #["path", "query", "headers", "body"])
model RequestSurface {
  method: HttpMethod;

  @minLength(1)
  @pattern("^/[^\\s]*$")
  pathTemplate: string;

  path?: Record<unknown>;
  query?: Record<unknown>;

  @extension("propertyNames", #{ pattern: "^[!#$%&'*+.^_`|~0-9a-z-]+$" })
  headers?: Record<unknown>;

  body?: unknown;
}
