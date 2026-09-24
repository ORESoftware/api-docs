// Human-authored TypeSpec peer authority for provider-neutral lambda deployment docs.
// JSON Schema peer: json-schema/lambda-deployment-docs.schema.json.
// Neither authority is generated from or permitted to overwrite the other.
//
// This contract intentionally does not import ores-stack, BeamScale, or Scintilla
// implementation code. Producers emit this interchange shape; api-docs validates,
// normalizes and renders it together with the existing route-map contract.

import "@typespec/json-schema";

namespace Ores.ApiDocs;

enum LambdaDocsProducer {
  ores_stack: "ores-stack",
  bmscl_compiler: "bmscl-compiler",
  scintilla: "scintilla",
  external: "external",
}

enum LambdaDocsTarget {
  beamscale,
  scintilla,
}

enum LambdaDocsRuntimeFamily {
  beam,
  native,
  interpreted,
  jvm,
  dotnet,
  wasm,
  oci,
}

enum LambdaDocsCarrier {
  beam_process,
  native_process,
  oci_container,
}

enum LambdaDocsArchitecture {
  portable,
  x86_64,
  arm64,
  wasm32,
}

model LambdaFunctionDeploymentDocs {
  @minLength(1)
  @maxLength(160)
  @pattern("^[A-Za-z0-9][A-Za-z0-9._/-]*$")
  id: string;

  producer: LambdaDocsProducer;
  runtimeFamily: LambdaDocsRuntimeFamily;

  @minLength(1)
  @maxLength(64)
  @pattern("^[A-Za-z][A-Za-z0-9+._-]*$")
  runtimeLanguage: string;

  carrier: LambdaDocsCarrier;
  architecture: LambdaDocsArchitecture;

  @minLength(1)
  @maxLength(512)
  entrypoint: string;

  @pattern("^[0-9a-f]{64}$")
  artifactSha256: string;

  @pattern("^[0-9a-f]{64}$")
  policySha256: string;

  operationKeys: string[];

  @minItems(1)
  deployTargets: LambdaDocsTarget[];
}

model LambdaDeploymentDocsManifest {
  schemaVersion: "ores.api-docs.lambda-deployment.v1";

  @minLength(1)
  @maxLength(128)
  @pattern("^[A-Za-z][A-Za-z0-9._-]*$")
  service: string;

  @pattern("^[0-9a-f]{64}$")
  contractSha256: string;

  functions: LambdaFunctionDeploymentDocs[];
}
