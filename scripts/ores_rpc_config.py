#!/usr/bin/env python3
"""Normalize and semantically validate .ores-rpc.toml.

TypeSpec and JSON Schema remain the peer schema authorities. This parser only
maps TOML spelling into the shared JSON shape and enforces invariants that span
multiple fields or protect the secret/role boundary.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
import tomllib

ENV_KEY = re.compile(r"^[A-Z_][A-Z0-9_]*$")
BINDING_NAME = re.compile(r"^[a-z][a-z0-9_]*$")
ALLOWED_KINDS = {"string", "bool", "integer", "double", "json", "url"}
ALLOWED_TRANSPORTS = {"http", "tcp", "websocket", "nats"}


def fail(message: str) -> "NoReturn":
    raise ValueError(message)


def normalize(path: Path) -> dict:
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    if set(raw) - {"schema_version", "mode", "strict", "flags2env", "rpc", "env"}:
        fail("unknown top-level .ores-rpc.toml key")
    if raw.get("schema_version") != 1:
        fail("schema_version must be 1")
    mode = raw.get("mode")
    if mode not in {"client", "server", "hybrid"}:
        fail("mode must be client, server, or hybrid")
    if raw.get("strict") is not True:
        fail("strict must be true")

    f2e = raw.get("flags2env")
    if not isinstance(f2e, dict) or set(f2e) != {"contract", "require_audit", "precedence"}:
        fail("flags2env must declare contract, require_audit, and precedence")
    if f2e.get("contract") != ".cli-flags.toml":
        fail("flags2env.contract must be .cli-flags.toml")
    if f2e.get("require_audit") is not True or f2e.get("precedence") != "argv-over-env":
        fail("flags2env must require audit and argv-over-env precedence")

    rpc = raw.get("rpc")
    if not isinstance(rpc, dict):
        fail("rpc table is required")
    allowed_rpc = {
        "stack", "transports", "request_validation", "response_validation",
        "client_root", "server_root",
    }
    if set(rpc) - allowed_rpc:
        fail("unknown rpc key")
    if rpc.get("stack") not in {"v1", "v2"}:
        fail("rpc.stack must be v1 or v2")
    transports = rpc.get("transports")
    if not isinstance(transports, list) or not transports or len(transports) != len(set(transports)):
        fail("rpc.transports must be a non-empty unique list")
    if any(value not in ALLOWED_TRANSPORTS for value in transports):
        fail("rpc.transports contains an unsupported transport")
    if rpc.get("request_validation") != "tjsv" or rpc.get("response_validation") != "tjsv":
        fail("RPC request and response validation must both be tjsv")
    client_root = rpc.get("client_root")
    server_root = rpc.get("server_root")
    if mode == "client" and (not client_root or server_root is not None):
        fail("client mode requires client_root and forbids server_root")
    if mode == "server" and (not server_root or client_root is not None):
        fail("server mode requires server_root and forbids client_root")
    if mode == "hybrid" and (not client_root or not server_root):
        fail("hybrid mode requires separate client_root and server_root")
    if client_root and server_root and Path(client_root) == Path(server_root):
        fail("client_root and server_root must be distinct")

    bindings = raw.get("env", [])
    if not isinstance(bindings, list):
        fail("env must be an array of tables")
    seen_names: set[str] = set()
    seen_keys: set[str] = set()
    normalized_env: list[dict] = []
    for binding in bindings:
        if not isinstance(binding, dict):
            fail("each env entry must be a table")
        allowed = {"name", "key", "kind", "required", "secret", "default", "description"}
        if set(binding) - allowed:
            fail("unknown env binding key")
        name = binding.get("name")
        key = binding.get("key")
        kind = binding.get("kind")
        if not isinstance(name, str) or not BINDING_NAME.fullmatch(name):
            fail("invalid env binding name")
        if not isinstance(key, str) or not ENV_KEY.fullmatch(key):
            fail("invalid env key")
        if kind not in ALLOWED_KINDS:
            fail("invalid env binding kind")
        if name in seen_names or key in seen_keys:
            fail("duplicate env binding name or key")
        if binding.get("secret") is True and "default" in binding:
            fail("secret env bindings must not declare defaults")
        if not isinstance(binding.get("required"), bool) or not isinstance(binding.get("secret"), bool):
            fail("required and secret must be booleans")
        seen_names.add(name)
        seen_keys.add(key)
        item = {
            "name": name,
            "key": key,
            "kind": kind,
            "required": binding["required"],
            "secret": binding["secret"],
        }
        if "default" in binding:
            item["default"] = str(binding["default"])
        if "description" in binding:
            item["description"] = str(binding["description"])
        normalized_env.append(item)

    rpc_out = {
        "stack": rpc["stack"],
        "transports": transports,
        "requestValidation": rpc["request_validation"],
        "responseValidation": rpc["response_validation"],
    }
    if client_root is not None:
        rpc_out["clientRoot"] = client_root
    if server_root is not None:
        rpc_out["serverRoot"] = server_root
    return {
        "schemaVersion": 1,
        "mode": mode,
        "strict": True,
        "flags2env": {
            "contract": f2e["contract"],
            "requireAudit": True,
            "precedence": "argv-over-env",
        },
        "rpc": rpc_out,
        "env": normalized_env,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", nargs="?", default=".ores-rpc.toml")
    parser.add_argument("--output")
    args = parser.parse_args()
    try:
        value = normalize(Path(args.path))
    except (OSError, tomllib.TOMLDecodeError, ValueError) as exc:
        print(f"ores-rpc config rejected: {exc}", file=sys.stderr)
        return 2
    encoded = json.dumps(value, indent=2, sort_keys=True) + "\n"
    if args.output:
        Path(args.output).write_text(encoded, encoding="utf-8")
    else:
        sys.stdout.write(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
