# `.ores-rpc.toml` runtime configuration

`api-docs` uses a repository-root `.ores-rpc.toml` to declare **how** RPC contracts are consumed in this checkout. The TOML file is runtime/configuration policy; it is not a third contract authority.

The normalized configuration has two independent human-authored peer authorities in their own closed config-contract namespace:

- `contracts/ores-rpc-config/main.tsp`
- `contracts/ores-rpc-config/authored.schema.json` (JSON Schema Draft 2020-12)

They intentionally live outside `idl/typespec/` and `json-schema/`, whose strict inventories are reserved for the reviewed wire-RPC/document contract sets. CI admits the config peers separately with the immutable `ORESoftware/typespec-json-schema-validator` revision already governed by `contracts/tjsv-consumer.lock.json`. A discrepancy is a stop condition; neither source overwrites the other.

## Repository roles

`repositoryMode` is explicit and supports `client-only`, `server-only`, and `combined` repositories. The `client` and `server` tables separately name their code roots, and the checker rejects a mode/role contradiction. `api-docs` is `combined`: client implementations live under `clients/` and the Rust server/docs library lives under `rust/`.

This avoids guessing role from repository names and handles repositories where client and server code intentionally share one checkout.

## Environment and `flags-2-env`

The configuration declares environment bindings rather than values. The current binding is:

```toml
[[environment]]
name = "ORES_RPC_MAX_FRAME_BYTES"
target = "rpc.maxFrameBytes"
valueType = "uint32"
source = "flags-2-env"
secret = false
```

The checker applies declared environment values after TOML defaults. Executables are expected to normalize CLI arguments through `flags-2-env` first, so effective precedence is:

```text
flags -> environment -> .ores-rpc.toml
```

`api-docs` itself is a library/tooling repository and currently has no repository-root `.cli-flags.toml`, so `flags2env.contractPresent = false`. A consumer that adds an executable flips that field to `true`, checks in `.cli-flags.toml`, rejects unknown flags, and keeps the same precedence.

No secret value is permitted in `.ores-rpc.toml`. The normalized `EnvironmentBinding.secret` field is fixed to `false`, the semantic checker rejects any attempt to change it, and `secretsFromEnvironmentOnly = true` reserves credentials for the environment/secret store rather than TOML or argv.

## Admission

`scripts/check-ores-rpc-config.py` is a fixed entrypoint with no ad-hoc CLI parser. It:

- parses TOML with Python `tomllib`;
- rejects unknown tables/keys;
- rejects absolute paths, `..` traversal, missing enabled role roots, missing route maps, and missing bundle/consumer-lock inputs;
- verifies the TJSV consumer-lock repository identity;
- enforces the exact transport/framing inventory and the 16 MiB frame safety ceiling;
- enforces `client-only`, `server-only`, or `combined` role consistency;
- validates unique `flags-2-env` environment names/targets and typed overrides;
- refuses secret bindings;
- writes the normalized `OresRpcConfig` JSON instance under `tmp/` for TJSV differential admission.

The adversarial unit suite is `scripts/test-ores-rpc-config.py`. The existing TJSV RPC workflow runs both the semantic checker and the isolated config peer-authority/differential gate on the exact pull-request head and retains machine-readable evidence.
