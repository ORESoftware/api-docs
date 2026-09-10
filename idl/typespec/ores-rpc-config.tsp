import "@typespec/json-schema";
using TypeSpec.JsonSchema;

// Human-authored peer authority for normalized .ores-rpc.toml configuration.
// JSON Schema peer: json-schema/ores-rpc-config.schema.json.
// Neither source is generated from, ranked below, or permitted to overwrite the other.
namespace Ores.Rpc.Config;

@id("RepositoryMode")
enum RepositoryMode {
  clientOnly: "client-only",
  serverOnly: "server-only",
  combined: "combined",
}

@id("RpcTransport")
enum RpcTransport {
  http: "http",
  tcp: "tcp",
  websocket: "websocket",
  nats: "nats",
}

@id("RpcFraming")
enum RpcFraming {
  json: "json",
  ndjson: "ndjson",
  lengthPrefixed: "length-prefixed",
}

@id("TjsvConfig")
model TjsvConfig {
  repository: "ORESoftware/typespec-json-schema-validator";
  @minLength(1)
  consumerLock: string;
  failClosed: true;
}

@id("Flags2EnvConfig")
model Flags2EnvConfig {
  provider: "flags-2-env";
  @minLength(1)
  contract: string;
  contractPresent: boolean;
  requiredWhenExecutable: true;
  rejectUnknownFlags: true;
  secretsFromEnvironmentOnly: true;
  precedence: "flags>env>toml";
}

@id("RpcConfig")
model RpcConfig {
  @minLength(1)
  routeMapGlob: string;
  @minLength(1)
  contractBundle: string;
  transports: RpcTransport[];
  framings: RpcFraming[];
  @minValue(1)
  @maxValue(16777216)
  maxFrameBytes: uint32;
}

@id("RoleConfig")
model RoleConfig {
  enabled: boolean;
  @minLength(1)
  root: string;
}

@id("EnvironmentBinding")
model EnvironmentBinding {
  @pattern("^[A-Z][A-Z0-9_]*$")
  name: string;
  @pattern("^[a-z][A-Za-z0-9]*(?:\\.[a-z][A-Za-z0-9]*)+$")
  target: string;
  valueType: "uint32";
  source: "flags-2-env";
  secret: false;
}

@id("OresRpcConfig")
model OresRpcConfig {
  schemaVersion: "ores.rpc.config/v1";
  repositoryMode: RepositoryMode;
  strict: true;
  tjsv: TjsvConfig;
  flags2env: Flags2EnvConfig;
  rpc: RpcConfig;
  client: RoleConfig;
  server: RoleConfig;
  environment: EnvironmentBinding[];
}
