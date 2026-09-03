// Human-authored TypeSpec peer authority for API-docs discovery.
// JSON Schema peer: json-schema/docs-discovery.schema.json.
// Neither file is generated from or permitted to overwrite the other.

namespace Ores.ApiDocs;

model DocsProjectionRoutes {
  openapi: "/openapi.json";
  openrpc: "/openrpc.json";
  connect: "/connect.json";
}

model DocsDiscoveryManifest {
  schemaVersion: "1.0.0";

  @minLength(1)
  @maxLength(128)
  @pattern("^[A-Za-z][A-Za-z0-9._-]*$")
  service: string;

  @pattern("^[0-9a-f]{64}$")
  contractSha256: string;

  @minValue(1)
  routeCount: uint32;

  discovery: "/api-docs/manifest.json";
  html: "/docs/api";
  catalog: "/api/docs.json";
  projections: DocsProjectionRoutes;

  @minItems(4)
  aliases: string[];
}
