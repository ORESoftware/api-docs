// Human-authored peer authority for source-bound server compatibility receipts.
// JSON Schema peer: ../json-schema/server-compatibility-receipt.schema.json.
// Neither authority is generated from or permitted to overwrite the other.

import "@typespec/json-schema";

namespace Ores.ApiDocs;

enum ServerRole {
  web,
  api,
  admin,
}

enum DeploymentTargetKind {
  standalone,
  scintilla,
  aws_lambda,
  gcp_cloud_run,
  gcp_cloud_run_functions,
}

enum EvidenceState {
  passed,
  failed,
  blocked,
  skipped,
  not_run,
}

enum CheckOutcome {
  passed,
  failed,
  blocked,
  skipped,
  not_run,
}

model SourceIdentity {
  @minLength(3)
  @maxLength(200)
  @pattern("^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
  repository: string;

  @pattern("^[0-9a-f]{40}$")
  sha: string;
}

model ServerIdentity {
  source: SourceIdentity;

  @minLength(1)
  @maxLength(160)
  binary: string;

  role: ServerRole;
}

model InfraIdentity {
  source: SourceIdentity;

  @pattern("^[0-9a-f]{64}$")
  configSha256: string;
}

model ToolchainIdentity {
  oresStack: SourceIdentity;
  oresCompose: SourceIdentity;
}

model DeploymentTarget {
  kind: DeploymentTargetKind;

  @minLength(1)
  @maxLength(160)
  adapter: string;

  @pattern("^[0-9a-f]{64}$")
  artifactSha256: string;
}

model EvidenceCheck {
  @minLength(1)
  @maxLength(160)
  id: string;

  executed: boolean;
  outcome: CheckOutcome;
}

model ServerCompatibilityReceipt {
  schemaVersion: "ores.api-docs.server-compatibility.v1";
  server: ServerIdentity;
  infra: InfraIdentity;
  toolchain: ToolchainIdentity;

  @pattern("^[0-9a-f]{64}$")
  contractSha256: string;

  target: DeploymentTarget;
  capabilities: string[];
  evidenceState: EvidenceState;
  executedChecks: uint32;
  checks: EvidenceCheck[];
}
