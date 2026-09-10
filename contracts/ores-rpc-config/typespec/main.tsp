import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace OresApiDocsRpcConfig;

// Independent top-level authority for normalized `.ores-rpc.toml` orchestration.
// This file is human-authored and must not be generated from JSON Schema.

@id("OresRpcRepositoryMode")
enum OresRpcRepositoryMode {
  serverOnly: "server-only",
  clientOnly: "client-only",
  hybrid: "hybrid",
}

@id("OresRpcTargetRole")
enum OresRpcTargetRole {
  server: "server",
  client: "client",
}

@id("OresRpcVersion")
enum OresRpcVersion {
  v1: "v1",
  v2: "v2",
}

@id("OresRpcTransport")
enum OresRpcTransport {
  http: "http",
  tcp: "tcp",
  websocket: "websocket",
  nats: "nats",
}

@id("OresRpcFraming")
enum OresRpcFraming {
  json: "json",
  ndjson: "ndjson",
  lengthPrefixed: "length-prefixed",
  frame: "frame",
}

@id("OresRpcEnvValueType")
enum OresRpcEnvValueType {
  string: "string",
  integer: "integer",
  boolean: "boolean",
}

@id("OresRpcEnvBindingV1")
model OresRpcEnvBindingV1 {
  @minLength(1)
  @maxLength(64)
  @pattern("^[a-z][a-zA-Z0-9]{0,63}$")
  name: string;

  @minLength(1)
  @maxLength(128)
  @pattern("^[A-Z][A-Z0-9_]{0,127}$")
  env: string;

  valueType: OresRpcEnvValueType;
  required: boolean;
  secret: boolean;
  allowArgv: boolean;
}

@id("OresRpcTargetV1")
model OresRpcTargetV1 {
  @minLength(1)
  @maxLength(128)
  @pattern("^[a-z0-9](?:[a-z0-9._-]{0,126}[a-z0-9])?$")
  name: string;

  role: OresRpcTargetRole;
  roots: string[];
  rpcVersion: OresRpcVersion;
  transports: OresRpcTransport[];
  framing: OresRpcFraming;

  @minLength(1)
  @maxLength(255)
  routeMap?: string;

  @minLength(1)
  @maxLength(128)
  @pattern("^[A-Z][A-Z0-9_]{0,127}$")
  endpointEnv?: string;

  propagateHeaders?: string[];
}

@id("OresRpcConfigV1")
model OresRpcConfigV1 {
  schemaVersion: "ores.rpc.config.v1";
  repositoryMode: OresRpcRepositoryMode;
  strict: true;

  @minLength(1)
  @maxLength(255)
  flagsContract?: string;

  @minLength(1)
  @maxLength(128)
  defaultTarget?: string;

  allowOverlappingRoots?: boolean;
  env?: OresRpcEnvBindingV1[];
  targets: OresRpcTargetV1[];
}
