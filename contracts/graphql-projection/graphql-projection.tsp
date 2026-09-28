import "@typespec/json-schema";

using TypeSpec.JsonSchema;

namespace Ores.ApiDocs.GraphqlProjection;

@jsonSchema
model GraphqlProjectionManifest {
  schema_version: 2;
  generated_by: "ores-stack";
  endpoint: "/v1/graphql";
  authority: "resolver.rs";
  operations: GraphqlProjectionOperation[];
}

@jsonSchema
model GraphqlProjectionOperation {
  @pattern("^[a-z0-9]+(?:[._-][a-z0-9]+)+$")
  operation_key: string;
  semantic_function: string;
  semantic_authority: GraphqlSemanticAuthority;
  semantic_source: string;
  binding: GraphqlSemanticBinding;
  kind: GraphqlOperationKind;
  field: string;
  stream: GraphqlStreamMode;
  graphql_source: string;
}

@jsonSchema
model GraphqlSemanticBinding {
  schema_version: 1;
  @pattern("^[a-z0-9]+(?:[._-][a-z0-9]+)+$")
  operation_key: string;
  @pattern("^ores-callable-v1:[0-9a-f]{64}$")
  callable_id: string;
  @minLength(1)
  operation_spec: string;
  @pattern("^[0-9a-f]{64}$")
  registry_contract_sha256: string;
  @pattern("^[0-9a-f]{64}$")
  operation_contract_sha256: string;
  policy: GraphqlPolicyIdentity;
  types: GraphqlTypeIdentity;
}

@jsonSchema
model GraphqlPolicyIdentity {
  scope: GraphqlOperationScope;
  audiences: GraphqlClientAudience[];
}

@jsonSchema
model GraphqlTypeIdentity {
  @pattern("^[0-9a-f]{64}$")
  path_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  query_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  request_header_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  request_body_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  response_header_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  response_trailer_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  response_body_schema_sha256?: string;
  @pattern("^[0-9a-f]{64}$")
  error_schema_sha256?: string;
}

@jsonSchema
enum GraphqlSemanticAuthority {
  handlers: "handlers.rs",
  funcs: "funcs.rs",
}

@jsonSchema
enum GraphqlOperationKind {
  query: "query",
  mutation: "mutation",
  subscription: "subscription",
}

@jsonSchema
enum GraphqlStreamMode {
  unary: "unary",
  server_stream: "server_stream",
}

@jsonSchema
enum GraphqlOperationScope {
  regular: "regular",
  admin: "admin",
}

@jsonSchema
enum GraphqlClientAudience {
  browser: "browser",
  server: "server",
}
