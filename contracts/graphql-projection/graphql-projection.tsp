import "@typespec/json-schema";

using TypeSpec.JsonSchema;

namespace Ores.ApiDocs.GraphqlProjection;

@jsonSchema
model GraphqlProjectionManifest {
  schema_version: 1;
  generated_by: "ores-stack";
  endpoint: "/v1/graphql";
  authority: "graphql.rs";
  operations: GraphqlProjectionOperation[];
}

@jsonSchema
model GraphqlProjectionOperation {
  operation_key: string;
  semantic_function: string;
  semantic_authority: GraphqlSemanticAuthority;
  semantic_source: string;
  kind: GraphqlOperationKind;
  field: string;
  stream: GraphqlStreamMode;
  graphql_source: string;
}

@jsonSchema
enum GraphqlSemanticAuthority {
  handlers: "handlers.rs",
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
