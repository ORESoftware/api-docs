#!/usr/bin/env python3
"""Strict parser/resolver for repository-local .ores-rpc.toml.

This is orchestration tooling only. TypeSpec and JSON Schema remain the two
human-authored peer authorities; this parser enforces repository semantics that
are intentionally not encoded as a third schema authority.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tomllib
from typing import Any

CONFIG_FILE = ".ores-rpc.toml"
SCHEMA_VERSION = "ores.rpc.config.v1"
MAX_BYTES = 256 * 1024
MAX_TARGETS = 64
MAX_ENV = 128
NAME_RE = re.compile(r"^[a-z0-9](?:[a-z0-9._-]{0,126}[a-z0-9])?$")
ENV_RE = re.compile(r"^[A-Z][A-Z0-9_]{0,127}$")
LOGICAL_RE = re.compile(r"^[a-z][a-zA-Z0-9]{0,63}$")
HEADER_RE = re.compile(r"^[a-z0-9][a-z0-9!#$%&'*+.^_`|~-]*$")

TOP_KEYS = {
    "schemaVersion", "repositoryMode", "strict", "flagsContract",
    "defaultTarget", "allowOverlappingRoots", "env", "targets",
}
ENV_KEYS = {"name", "env", "valueType", "required", "secret", "allowArgv"}
TARGET_KEYS = {
    "name", "role", "roots", "rpcVersion", "transports", "framing",
    "routeMap", "endpointEnv", "propagateHeaders",
}


class ConfigError(Exception):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def fail(code: str) -> None:
    raise ConfigError(code)


def _exact_keys(value: dict[str, Any], allowed: set[str], code: str) -> None:
    if any(key not in allowed for key in value):
        fail(code)


def _safe_relative(value: Any, *, allow_dot: bool, code: str) -> str:
    if not isinstance(value, str) or not value or len(value) > 255:
        fail(code)
    if "\\" in value or value.startswith("/") or re.match(r"^[A-Za-z]:", value):
        fail(code)
    path = PurePosixPath(value)
    if any(part in ("", "..") for part in path.parts):
        fail(code)
    normalized = path.as_posix().rstrip("/") or "."
    if normalized == "." and not allow_dot:
        fail(code)
    if normalized.startswith("../") or "/../" in normalized:
        fail(code)
    return normalized


def _inside_repo(repo: Path, relative: str, *, require_file: bool = False, require_dir: bool = False) -> Path:
    current = repo
    if relative != ".":
        for part in PurePosixPath(relative).parts:
            current = current / part
            try:
                st = current.lstat()
            except OSError:
                fail("path-missing")
            if os.path.islink(current):
                fail("path-symlink")
    resolved = current.resolve()
    try:
        resolved.relative_to(repo.resolve())
    except ValueError:
        fail("path-escape")
    if require_file and not resolved.is_file():
        fail("path-not-file")
    if require_dir and not resolved.is_dir():
        fail("path-not-dir")
    return resolved


def _unique_strings(values: Any, *, minimum: int, maximum: int, code: str) -> list[str]:
    if not isinstance(values, list) or not (minimum <= len(values) <= maximum):
        fail(code)
    if not all(isinstance(item, str) and item for item in values):
        fail(code)
    if len(set(values)) != len(values):
        fail(code)
    return values


def _overlap(left: str, right: str) -> bool:
    if left == "." or right == ".":
        return True
    return left == right or left.startswith(right + "/") or right.startswith(left + "/")


def _parse_env(rows: Any, *, flags_contract: str | None) -> list[dict[str, Any]]:
    if rows is None:
        return []
    if not isinstance(rows, list) or len(rows) > MAX_ENV:
        fail("env-count")
    names: set[str] = set()
    keys: set[str] = set()
    result: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, dict):
            fail("env-shape")
        _exact_keys(row, ENV_KEYS, "env-unknown-key")
        if set(row) != ENV_KEYS:
            fail("env-missing-key")
        name, env = row["name"], row["env"]
        if not isinstance(name, str) or not LOGICAL_RE.fullmatch(name):
            fail("env-name")
        if not isinstance(env, str) or not ENV_RE.fullmatch(env):
            fail("env-key")
        if name in names or env in keys:
            fail("env-duplicate")
        names.add(name); keys.add(env)
        if row["valueType"] not in {"string", "integer", "boolean"}:
            fail("env-type")
        for boolean_key in ("required", "secret", "allowArgv"):
            if not isinstance(row[boolean_key], bool):
                fail("env-boolean")
        if row["secret"] and row["allowArgv"]:
            fail("secret-argv")
        if row["allowArgv"] and flags_contract != ".cli-flags.toml":
            fail("argv-without-flags-contract")
        result.append(dict(row))
    return result


def _parse_targets(rows: Any, *, repo: Path, env_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if not isinstance(rows, list) or not (1 <= len(rows) <= MAX_TARGETS):
        fail("target-count")
    env_by_key = {row["env"]: row for row in env_rows}
    names: set[str] = set()
    result: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, dict):
            fail("target-shape")
        _exact_keys(row, TARGET_KEYS, "target-unknown-key")
        required = {"name", "role", "roots", "rpcVersion", "transports", "framing"}
        if not required.issubset(row):
            fail("target-missing-key")
        name = row["name"]
        if not isinstance(name, str) or not NAME_RE.fullmatch(name):
            fail("target-name")
        if name in names:
            fail("target-duplicate")
        names.add(name)
        role = row["role"]
        if role not in {"client", "server"}:
            fail("target-role")
        roots = _unique_strings(row["roots"], minimum=1, maximum=16, code="target-roots")
        normalized_roots = []
        for root in roots:
            normalized = _safe_relative(root, allow_dot=True, code="target-root-path")
            _inside_repo(repo, normalized, require_dir=True)
            normalized_roots.append(normalized)
        if len(set(normalized_roots)) != len(normalized_roots):
            fail("target-roots")
        version = row["rpcVersion"]
        if version not in {"v1", "v2"}:
            fail("rpc-version")
        transports = _unique_strings(row["transports"], minimum=1, maximum=4, code="transports")
        if any(item not in {"http", "tcp", "websocket", "nats"} for item in transports):
            fail("transport")
        framing = row["framing"]
        if framing not in {"json", "ndjson", "length-prefixed", "frame"}:
            fail("framing")
        if version == "v1":
            if "tcp" in transports:
                if len(transports) != 1 or framing not in {"ndjson", "length-prefixed"}:
                    fail("v1-tcp-framing")
            elif framing != "json":
                fail("v1-json-framing")
        else:
            if framing != "frame":
                fail("v2-frame-required")
            if "nats" in transports:
                fail("v2-nats-unsupported")
        route_map = row.get("routeMap")
        if route_map is not None:
            route_map = _safe_relative(route_map, allow_dot=False, code="route-map-path")
            _inside_repo(repo, route_map, require_file=True)
        endpoint_env = row.get("endpointEnv")
        headers = row.get("propagateHeaders")
        if role == "server" and (endpoint_env is not None or headers is not None):
            fail("server-client-field")
        if role == "client":
            if endpoint_env is not None:
                if not isinstance(endpoint_env, str) or not ENV_RE.fullmatch(endpoint_env):
                    fail("endpoint-env")
                binding = env_by_key.get(endpoint_env)
                if binding is None:
                    fail("endpoint-env-undeclared")
                if binding["secret"]:
                    fail("endpoint-env-secret")
            if headers is not None:
                values = _unique_strings(headers, minimum=1, maximum=32, code="propagate-headers")
                if any(not HEADER_RE.fullmatch(header) for header in values):
                    fail("propagate-header")
        normalized = dict(row)
        normalized["roots"] = normalized_roots
        if route_map is not None:
            normalized["routeMap"] = route_map
        result.append(normalized)
    return result


def load_config(path: Path, repo_root: Path) -> dict[str, Any]:
    try:
        raw = path.read_bytes()
    except OSError:
        fail("config-read")
    if not raw or len(raw) > MAX_BYTES:
        fail("config-size")
    try:
        value = tomllib.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError):
        fail("config-syntax")
    if not isinstance(value, dict):
        fail("config-shape")
    _exact_keys(value, TOP_KEYS, "config-unknown-key")
    if value.get("schemaVersion") != SCHEMA_VERSION:
        fail("schema-version")
    if value.get("repositoryMode") not in {"server-only", "client-only", "hybrid"}:
        fail("repository-mode")
    if value.get("strict") is not True:
        fail("strict-required")
    flags_contract = value.get("flagsContract")
    if flags_contract is not None:
        flags_contract = _safe_relative(flags_contract, allow_dot=False, code="flags-contract-path")
        if flags_contract != ".cli-flags.toml":
            fail("flags-contract-canonical")
        _inside_repo(repo_root, flags_contract, require_file=True)
    env_rows = _parse_env(value.get("env"), flags_contract=flags_contract)
    targets = _parse_targets(value.get("targets"), repo=repo_root, env_rows=env_rows)
    mode = value["repositoryMode"]
    roles = {target["role"] for target in targets}
    if mode == "server-only" and roles != {"server"}:
        fail("repository-role-mismatch")
    if mode == "client-only" and roles != {"client"}:
        fail("repository-role-mismatch")
    if mode == "hybrid" and roles != {"client", "server"}:
        fail("repository-role-mismatch")
    default_target = value.get("defaultTarget")
    names = {target["name"] for target in targets}
    if default_target is not None and default_target not in names:
        fail("default-target")
    allow_overlap = value.get("allowOverlappingRoots", False)
    if not isinstance(allow_overlap, bool):
        fail("overlap-boolean")
    root_rows: list[tuple[str, str]] = []
    for target in targets:
        for root in target["roots"]:
            for other_name, other_root in root_rows:
                if _overlap(root, other_root) and not allow_overlap:
                    fail("root-overlap")
            root_rows.append((target["name"], root))
    normalized: dict[str, Any] = {
        "schemaVersion": SCHEMA_VERSION,
        "repositoryMode": mode,
        "strict": True,
        "allowOverlappingRoots": allow_overlap,
        "targets": targets,
    }
    if flags_contract is not None:
        normalized["flagsContract"] = flags_contract
    if default_target is not None:
        normalized["defaultTarget"] = default_target
    if env_rows:
        normalized["env"] = env_rows
    return normalized


def resolve_target(config: dict[str, Any], *, target_name: str | None, source_path: str | None) -> dict[str, Any]:
    targets = config["targets"]
    if target_name is not None:
        matches = [target for target in targets if target["name"] == target_name]
        if len(matches) != 1:
            fail("target-not-found")
        return matches[0]
    if source_path is None:
        default = config.get("defaultTarget")
        if default is None:
            fail("target-required")
        return resolve_target(config, target_name=default, source_path=None)
    source = _safe_relative(source_path, allow_dot=True, code="source-path")
    matches = []
    for target in targets:
        for root in target["roots"]:
            if root == "." or source == root or source.startswith(root + "/"):
                matches.append(target)
                break
    if len(matches) != 1:
        fail("target-ambiguous" if len(matches) > 1 else "target-not-found")
    return matches[0]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="ores-rpc-config")
    parser.add_argument("--manifest", default=CONFIG_FILE)
    parser.add_argument("--repo-root", default=".")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    sub.add_parser("normalize")
    resolve = sub.add_parser("resolve")
    group = resolve.add_mutually_exclusive_group()
    group.add_argument("--target")
    group.add_argument("--path")
    args = parser.parse_args(argv)
    repo = Path(args.repo_root).resolve()
    manifest_rel = _safe_relative(args.manifest, allow_dot=False, code="manifest-path")
    manifest = repo / manifest_rel
    try:
        config = load_config(manifest, repo)
        if args.command == "check":
            print("ores-rpc-config: ok")
        elif args.command == "normalize":
            print(json.dumps(config, sort_keys=True, separators=(",", ":")))
        else:
            print(json.dumps(resolve_target(config, target_name=args.target, source_path=args.path), sort_keys=True, separators=(",", ":")))
        return 0
    except ConfigError as exc:
        print(f"ores-rpc-config: {exc.code}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
