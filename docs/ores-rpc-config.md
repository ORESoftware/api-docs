# `.ores-rpc.toml` RPC orchestration

`.ores-rpc.toml` selects where and how the `api-docs` RPC contracts are consumed inside a repository. It is **orchestration**, not a new wire-contract authority.

The normalized configuration has two independent, human-authored peer authorities:

- `contracts/ores-rpc-config/typespec/main.tsp`
- `contracts/ores-rpc-config/json-schema/ores-rpc-config.schema.json`

`ORESoftware/typespec-json-schema-validator` (TJSV) compares those authorities fail-closed at an immutable reviewed revision. Generated JSON Schema, Contract IR, runtime projections, and receipts are evidence only.

The existing RPC wire authorities remain unchanged: TypeSpec under `idl/typespec/`, the JSON Schema/OpenAPI track under `json-schema/`, and the reviewed route-map operation inventories. `.ores-rpc.toml` does not redefine call/receipt/frame fields or operation schemas.

## Repository roles

A repository declares one of:

- `server-only`
- `client-only`
- `hybrid`

Each target then declares an explicit `role = "server" | "client"`. A hybrid repository must contain both roles. Client/server identity is never inferred from a directory name.

Separate roots are straightforward:

```toml
schemaVersion = "ores.rpc.config.v1"
repositoryMode = "hybrid"
strict = true

[[targets]]
name = "api"
role = "server"
roots = ["server"]
rpcVersion = "v1"
transports = ["http"]
framing = "json"

[[targets]]
name = "browser"
role = "client"
roots = ["web"]
rpcVersion = "v1"
transports = ["http", "websocket"]
framing = "json"
propagateHeaders = ["traceparent", "x-request-id"]
```

When client and server code intentionally share a root, overlap must be explicit:

```toml
schemaVersion = "ores.rpc.config.v1"
repositoryMode = "hybrid"
strict = true
allowOverlappingRoots = true

[[targets]]
name = "server"
role = "server"
roots = ["."]
rpcVersion = "v1"
transports = ["http"]
framing = "json"

[[targets]]
name = "client"
role = "client"
roots = ["."]
rpcVersion = "v1"
transports = ["http"]
framing = "json"
```

A path that matches more than one target fails closed. The caller must resolve the target explicitly:

```sh
python3 scripts/ores_rpc_config.py resolve --target server
python3 scripts/ores_rpc_config.py resolve --target client
```

## `flags-2-env` and environment declarations

`.ores-rpc.toml` is not an argv parser and must not become a second option schema. Executable consumers continue to use repository-root `.cli-flags.toml` and the official `flags-2-env` binding as their sole argv boundary.

A consumer may declare environment-key references:

```toml
schemaVersion = "ores.rpc.config.v1"
repositoryMode = "client-only"
strict = true
flagsContract = ".cli-flags.toml"

[[env]]
name = "apiBaseUrl"
env = "API_BASE_URL"
valueType = "string"
required = true
secret = false
allowArgv = true

[[env]]
name = "serviceCredential"
env = "RPC_SERVICE_CREDENTIAL"
valueType = "string"
required = true
secret = true
allowArgv = false

[[targets]]
name = "client"
role = "client"
roots = ["."]
rpcVersion = "v1"
transports = ["http"]
framing = "json"
endpointEnv = "API_BASE_URL"
propagateHeaders = ["traceparent", "x-request-id"]
```

Rules:

- TOML stores environment-variable **names**, never environment values.
- `allowArgv = true` is legal only when `flagsContract = ".cli-flags.toml"` is present and the file exists.
- `secret = true` requires `allowArgv = false`; credentials must not enter argv/process listings or shell history.
- A client `endpointEnv` must name a declared non-secret environment binding.
- The executable audits/parses `.cli-flags.toml` with `flags-2-env`, then passes only resolved typed values inward. `.ores-rpc.toml` never reparses argv.

`api-docs` itself does not declare `flagsContract`, because it is the contract/library repository rather than one application argv authority.

## RPC versions and framing

The manifest keeps the existing v1/v2 protocol boundary explicit:

- v1 HTTP/WebSocket/NATS targets use `framing = "json"`.
- v1 TCP is a separate target using exactly one of `ndjson` or `length-prefixed`; a target may not mix TCP with JSON transports because one target has one framing contract.
- v2 uses `framing = "frame"`.
- v2 NATS is rejected until the underlying reviewed v2 runtime declares and proves that transport; the config must not create support by assertion.

This prevents a v2 frame from being admitted as a v1 call/receipt or an unsupported transport from appearing enabled through configuration alone.

## Paths and references

Roots, route maps, and the optional flags contract are repository-relative portable `/` paths. Absolute paths, drive-letter paths, `..`, backslashes, symlink traversal, missing referenced files, and repository escape are rejected. Referenced route maps are regular files inside the repository.

Client-only propagation fields are structurally separated from server targets. Server targets reject `endpointEnv` and `propagateHeaders`.

## Commands

```sh
python3 -m unittest scripts/test_ores_rpc_config.py -v
python3 scripts/ores_rpc_config.py check
python3 scripts/ores_rpc_config.py normalize
python3 scripts/ores_rpc_config.py resolve --target rust-docs-server
```

Errors are bounded codes and do not echo TOML bodies, environment values, credentials, or parser payloads.
