namespace Ores.ApiDocs.BuildReceipt;

/**
 * Explicit build provenance for a deterministic API/MCP documentation bundle.
 *
 * These fields identify the source and generator inputs used by one build. They
 * are evidence about generation and never replace authored API contract sources.
 */
model DocsBuildProvenance {
  source_repository: string;
  source_ref: string;
  source_sha: string;
  typespec_sha256: string;
  json_schema_sha256: string;
  config_sha256: string;
  generator_commit: string;
  toolchain_identity: string;
}

model DocsBuildReceiptBase {
  schema_version: "ores.api-docs.build-receipt.v1";
  generator: "ores-api-docs";
  service: string;
  producer: string;
  contract_sha256: string;
  provenance: DocsBuildProvenance;
  artifact_sha256: Record<string>;
  receipt_sha256: string;
}

/** Build evidence for official external-developer documentation. */
model PublisherExternalDocsBuildReceipt extends DocsBuildReceiptBase {
  publication_mode: "publisher_external";
  authority_scope: "platform_external_developers";
}

/** Build evidence for end-user/project-owned documentation. */
model ConsumerProjectDocsBuildReceipt extends DocsBuildReceiptBase {
  publication_mode: "consumer_project";
  authority_scope: "project_owned";
}

alias DocsBuildReceipt =
  PublisherExternalDocsBuildReceipt | ConsumerProjectDocsBuildReceipt;
