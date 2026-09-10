import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace Ores.ApiDocs.RpcConfig;

@id("RpcRepositoryMode")
enum RpcRepositoryMode {
  client: "client",
  server: "server",
  hybrid: "hybrid",
}

@id("RpcStack")
enum RpcStack {
  v1: "v1",
  v2: "v2",
}

@id("RpcTransport")
enum RpcTransport {
  http: "http",
  tcp: "tcp",
  websocket: "websocket",
  nats: "nats",
}

@id("RpcValidationMode")
enum RpcValidationMode {
  tjsv: "tjsv",
}

@id("RpcEnvKind")
enum RpcEnvKind {
  string: "string",
  bool: "bool",
  integer: "integer",
  double: "double",
  json: "json",
  url: "url",
}

@id("RpcFlags2EnvPrecedence")
enum RpcFlags2EnvPrecedence {
  argvOverEnv: "argv-over-env",
}

@id("RpcFlags2EnvConfig")
model RpcFlags2EnvConfig {
  @minLength(1)
  @maxLength(256)
  contract: string;

  requireAudit: boolean;
  precedence: RpcFlags2EnvPrecedence;
}

@id("RpcEnvBinding")
model RpcEnvBinding {
  @minLength(1)
  @maxLength(64)
  @pattern("^[a-z][a-z0-9_]*$")
  name: string;

  @minLength(1)
  @maxLength(128)
  @pattern("^[A-Z_][A-Z0-9_]*$")
  key: string;

  kind: RpcEnvKind;
  required: boolean;
  secret: boolean;

  @maxLength(8192)
  default?: string;

  @maxLength(512)
  description?: string;
}

@id("RpcRuntimeConfig")
model RpcRuntimeConfig {
  stack: RpcStack;
  transports: RpcTransport[];
  requestValidation: RpcValidationMode;
  responseValidation: RpcValidationMode;

  @minLength(1)
  @maxLength(256)
  clientRoot?: string;

  @minLength(1)
  @maxLength(256)
  serverRoot?: string;
}

@id("OresRpcConfig")
model OresRpcConfig {
  schemaVersion: 1;
  mode: RpcRepositoryMode;
  strict: boolean;
  flags2env: RpcFlags2EnvConfig;
  rpc: RpcRuntimeConfig;
  env: RpcEnvBinding[];
}
