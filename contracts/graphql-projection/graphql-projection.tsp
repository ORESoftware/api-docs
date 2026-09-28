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
  operation_key: string;
  callable_id: string;
  operation_spec: string;
  registry_contract_sha256: string;
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
  path_schema_sha256?: string;
  query_schema_sha256?: string;
  request_header_schema_sha256?: string;
  request_body_schema_sha256?: string;
  response_header_schema_sha256?: string;
  response_trailer_schema_sha256?: string;
  response_body_schema_sha256?: string;
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
