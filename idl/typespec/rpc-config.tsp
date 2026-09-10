import "@typespec/json-schema";
using TypeSpec.JsonSchema;

@jsonSchema
namespace OresRpc.Config;

enum RepositoryMode {
  client_only: "client-only",
  server_only: "server-only",
  hybrid: "hybrid",
}

enum TargetRole {
  client: "client",
  server: "server",
}

enum Transport {
  http: "http",
  tcp: "tcp",
  websocket: "websocket",
  nats: "nats",
}

enum Framing {
  json: "json",
  ndjson: "ndjson",
  length_prefixed: "length-prefixed",
  rpc_frame_v2: "rpc-frame-v2",
}

enum EnvKind {
  string: "string",
  boolean: "bool",
  integer: "integer",
  double: "double",
  json: "json",
  url: "url",
}

@extension("additionalProperties", false)
model Flags2EnvConfig {
  @minLength(1) contract: string;
  require_audit: boolean;
  precedence: "argv-over-env";
}

@extension("additionalProperties", false)
model EnvBinding {
  @pattern("^[a-z][a-z0-9_]{0,63}$") name: string;
  @pattern("^[A-Z][A-Z0-9_]{0,127}$") key: string;
  kind: EnvKind;
  required: boolean;
  secret: boolean;
}

@extension("additionalProperties", false)
model RpcTarget {
  @pattern("^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$") name: string;
  role: TargetRole;
  @minItems(1) @maxItems(16) roots: string[];
  @minLength(1) route_map: string;
  @minItems(1) @maxItems(4) transports: Transport[];
  framing: Framing;
  endpoint_env?: string;
  bind_env?: string;
  auth_token_env?: string;
  @minValue(1) @maxValue(600000) timeout_ms: integer;
  @minValue(0) @maxValue(20) max_retries: integer;
}

@extension("additionalProperties", false)
model OresRpcConfig {
  schema_version: "ores.rpc.config.v1";
  repository_mode: RepositoryMode;
  strict: boolean;
  flags2env: Flags2EnvConfig;
  @maxItems(64) env: EnvBinding[];
  @minItems(1) @maxItems(64) targets: RpcTarget[];
}
