#!/usr/bin/env python3
from __future__ import annotations
import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONFIG = ROOT / ".ores-rpc.toml"

def fail(message: str) -> None:
    raise SystemExit(f"ores-rpc config error: {message}")

cfg = tomllib.loads(CONFIG.read_text())
if cfg.get("schema_version") != "ores.rpc.config.v1":
    fail("unsupported schema_version")
if cfg.get("repository_mode") not in {"client-only", "server-only", "hybrid"}:
    fail("invalid repository_mode")
flags = cfg.get("flags2env") or {}
if flags.get("contract") != ".cli-flags.toml" or flags.get("precedence") != "argv-over-env" or flags.get("require_audit") is not True:
    fail("flags2env must point at audited .cli-flags.toml with argv-over-env precedence")
cli = ROOT / flags["contract"]
if not cli.is_file():
    fail("referenced .cli-flags.toml does not exist")
cli_cfg = tomllib.loads(cli.read_text())
flag_envs = {value.get("env") for value in (cli_cfg.get("flags") or {}).values() if isinstance(value, dict) and value.get("env")}

bindings = cfg.get("env") or []
by_name: dict[str, dict] = {}
keys: set[str] = set()
for binding in bindings:
    name = binding.get("name")
    key = binding.get("key")
    if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,63}", name):
        fail(f"invalid env binding name {name!r}")
    if name in by_name or key in keys:
        fail("env binding names and keys must be unique")
    if binding.get("secret") and key in flag_envs:
        fail(f"secret {key} must be environment-only, not argv-addressable")
    if not binding.get("secret") and key not in flag_envs:
        fail(f"non-secret binding {key} must be declared by .cli-flags.toml")
    by_name[name] = binding
    keys.add(key)

roles: set[str] = set()
for target in cfg.get("targets") or []:
    role = target.get("role")
    roles.add(role)
    roots = target.get("roots") or []
    if not roots or any(pathlib.PurePosixPath(root).is_absolute() or ".." in pathlib.PurePosixPath(root).parts for root in roots):
        fail(f"target {target.get('name')} has unsafe roots")
    for field in ("endpoint_env", "bind_env", "auth_token_env"):
        name = target.get(field)
        if name is not None and name not in by_name:
            fail(f"target {target.get('name')} references unknown {field}={name}")
    if role == "client" and target.get("bind_env") is not None:
        fail("client target cannot declare bind_env")
    if role == "server" and target.get("endpoint_env") is not None:
        fail("server target cannot declare endpoint_env")
    if target.get("max_retries", 0) and role != "client":
        fail("server target cannot configure client retry behavior")

mode = cfg["repository_mode"]
expected = {"client-only": {"client"}, "server-only": {"server"}, "hybrid": {"client", "server"}}[mode]
if roles != expected:
    fail(f"repository_mode={mode} requires target roles {sorted(expected)}, got {sorted(roles)}")
print(f"validated {len(cfg.get('targets') or [])} RPC targets and {len(bindings)} env bindings")
