namespace Ores.ApiDocs.Publication;

/**
 * Deterministic API/MCP documentation publication identity.
 *
 * The route map and its semantic digest remain the API authority. This model
 * only says who owns a rendered documentation bundle and which audience may
 * treat that publication as authoritative.
 */
model PublicationBase {
  schema_version: "ores.api-docs.publication.v1";
  generator: "ores-api-docs";
  service: string;
  producer: string;
  contract_sha256: string;
  route_count: uint32;
  mcp_discovery: "/api-docs/manifest.json";
  artifacts: [
    "api/catalog.json",
    "api/openapi.json",
    "api/openrpc.json",
    "api/connect.json",
    "api/hyper-schema.json",
    "api/index.html",
    "mcp/manifest.json",
    "mcp/server.md",
    "publication.json"
  ];
}

/** Official external-developer documentation emitted by the platform owner. */
model PublisherExternalPublication extends PublicationBase {
  publication_mode: "publisher_external";
  authority_scope: "platform_external_developers";
  publisher_provenance_required: true;
}

/** Documentation emitted by an end user for that user's own project/service. */
model ConsumerProjectPublication extends PublicationBase {
  publication_mode: "consumer_project";
  authority_scope: "project_owned";
  publisher_provenance_required: false;
}

alias DocsPublicationManifest =
  PublisherExternalPublication | ConsumerProjectPublication;
