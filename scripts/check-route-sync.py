#!/usr/bin/env python3
"""Keep a route map in lockstep with HTTP handlers and JSON Schema.

The interchange contract is a JSON object whose keys are operations and whose
values are routes. This check:

1. Validates each map against JSON Schema (draft 2020-12) when `jsonschema`
   is installed; otherwise applies a structural subset of the same rules.
2. Scans Rust ` .route("...", get|post|...) ` registrations and requires a
   1:1 match with map paths (HEAD implied by GET is allowed).
3. If the source merges `docs::router()`, standard docs aliases may exist in
   code without being product map keys.

Exit 0 when in sync. Exit 1 on drift. Designed for pre-commit, pre-push, and CI.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

STANDARD_DOCS_PATHS = {
    "/docs/api",
    "/api/docs",
    "/api/docs.json",
    "/api-docs",
    "/api-docs.json",
    "/openapi.json",
    "/openrpc.json",
    "/connect.json",
}

ROUTE_CALL = re.compile(r"""\.route\(\s*["']([^"']+)["']""")
METHOD_CALL = re.compile(
    r"\b(get|post|put|patch|delete|head|options)\s*\(", re.IGNORECASE
)
DOCS_MERGE = re.compile(r"docs::router\s*\(")

# PascalCase Connect method key
PASCAL = re.compile(r"^[A-Z][A-Za-z0-9]*$")
KEY_OK = re.compile(r"^[A-Za-z][A-Za-z0-9_]*$")
PATH_OK = re.compile(r"^/\S*$")
HTTP_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}


def infer_methods(key: str) -> list[str]:
    if key and key[0].isupper():
        return ["POST"]
    lower = key.lower()
    if lower.startswith("delete"):
        return ["DELETE"]
    if lower.startswith(("put", "update", "replace")):
        return ["PUT"]
    if lower.startswith("patch"):
        return ["PATCH"]
    if any(s in lower for s in ("create", "walk", "check", "ask")) or lower.startswith(
        ("post", "submit")
    ):
        return ["POST"]
    return ["GET"]


def normalize_entry(key: str, value: Any) -> dict[str, Any]:
    if isinstance(value, str):
        return {"path": value, "methods": infer_methods(key)}
    if isinstance(value, dict) and isinstance(value.get("path"), str):
        methods = value.get("methods") or infer_methods(key)
        return {"path": value["path"], "methods": list(methods)}
    raise SystemExit(f"{key}: expected path string or object with path")


def load_map(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def structural_validate(instance: dict[str, Any], label: str) -> list[str]:
    errors: list[str] = []
    if instance.get("schema_version") != "1.0.0":
        errors.append(f"{label}: schema_version must be 1.0.0")
    if not isinstance(instance.get("service"), str) or not instance["service"]:
        errors.append(f"{label}: service required")
    raw = instance.get("map")
    if not isinstance(raw, dict) or not raw:
        errors.append(f"{label}: map must be a non-empty object")
        return errors
    for key, value in raw.items():
        if not KEY_OK.match(key):
            errors.append(f"{label}: bad key {key!r}")
            continue
        try:
            entry = normalize_entry(key, value)
        except SystemExit as exc:
            errors.append(f"{label}: {exc}")
            continue
        if not PATH_OK.match(entry["path"]):
            errors.append(f"{label}.{key}: path must start with /")
        for method in entry["methods"]:
            if method not in HTTP_METHODS:
                errors.append(f"{label}.{key}: bad method {method}")
        binding = value.get("binding") if isinstance(value, dict) else None
        if isinstance(binding, dict):
            if not (
                binding.get("annotation")
                or binding.get("param_types")
                or binding.get("return_type")
                or binding.get("function_type")
            ):
                errors.append(
                    f"{label}.{key}: binding needs annotation, param_types, return_type, and/or function_type"
                )
    return errors


def jsonschema_validate(instance: dict[str, Any], schema: dict[str, Any], label: str) -> list[str]:
    try:
        import jsonschema
    except ImportError:
        return structural_validate(instance, label)

    validator_cls = getattr(jsonschema, "Draft202012Validator", jsonschema.Draft7Validator)
    validator = validator_cls(schema)
    return [f"{label}: {e.message} at {e.json_path}" for e in validator.iter_errors(instance)]


def scan_rust_routes(source_dirs: Iterable[Path]) -> tuple[dict[str, set[str]], bool]:
    """path -> methods found in .route(...) calls. Also whether docs::router() is merged."""
    found: dict[str, set[str]] = {}
    docs_merge = False
    for root in source_dirs:
        if not root.exists():
            raise SystemExit(f"source path missing: {root}")
        files = [root] if root.is_file() else sorted(root.rglob("*.rs"))
        for path in files:
            text = path.read_text(encoding="utf-8")
            if DOCS_MERGE.search(text):
                docs_merge = True
            for line in text.splitlines():
                match = ROUTE_CALL.search(line)
                if not match:
                    continue
                route_path = match.group(1)
                methods = {m.upper() for m in METHOD_CALL.findall(line)}
                if not methods:
                    continue
                found.setdefault(route_path, set()).update(methods)
    return found, docs_merge


def compare(
    map_obj: dict[str, Any],
    scanned: dict[str, set[str]],
    *,
    allow_docs_merge: bool,
    docs_merged: bool,
    label: str,
) -> list[str]:
    errors: list[str] = []
    documented: dict[str, set[str]] = {}
    for key, value in map_obj["map"].items():
        entry = normalize_entry(key, value)
        documented.setdefault(entry["path"], set()).update(entry["methods"])

    extra_ok = STANDARD_DOCS_PATHS if (allow_docs_merge and docs_merged) else set()

    for path, methods in documented.items():
        if path not in scanned:
            errors.append(f"{label}: map path {path} is not registered in source")
            continue
        missing = methods - scanned[path]
        # Axum GET handlers also answer HEAD; maps usually omit HEAD.
        missing -= {"HEAD"}
        if missing:
            errors.append(
                f"{label}: {path} map methods {sorted(missing)} missing in source {sorted(scanned[path])}"
            )

    for path, methods in scanned.items():
        if path in extra_ok:
            continue
        if path not in documented:
            errors.append(f"{label}: source route {path} is not in the map")
            continue
        extra = methods - documented[path] - {"HEAD"}
        if extra:
            errors.append(
                f"{label}: {path} source methods {sorted(extra)} missing from map {sorted(documented[path])}"
            )
    return errors


def maps_identical(a: dict[str, Any], b: dict[str, Any], left: str, right: str) -> list[str]:
    if a == b:
        return []
    return [f"{left} is not byte-for-byte the same contract as {right}"]


def load_config(root: Path) -> dict[str, Any] | None:
    for name in ("route-sync.json", "scripts/route-sync.json"):
        p = root / name
        if p.is_file():
            return json.loads(p.read_text(encoding="utf-8"))
    return None


def resolve(root: Path, rel: str) -> Path:
    return (root / rel).resolve()


def default_schema(root: Path, explicit: str | None) -> Path | None:
    if explicit:
        return resolve(root, explicit)
    candidates = [
        root / "scripts/vendor/route-map.schema.json",
        root / "json-schema/route-map.schema.json",
        root / "../../oresoftware/api-docs/json-schema/route-map.schema.json",
        root / "../oresoftware/api-docs/json-schema/route-map.schema.json",
    ]
    for path in candidates:
        if path.resolve().is_file():
            return path.resolve()
    return None


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=None, help="repo root (default: cwd)")
    parser.add_argument("--map", action="append", dest="maps", default=[])
    parser.add_argument("--schema")
    parser.add_argument("--source", action="append", dest="sources", default=[])
    parser.add_argument("--allow-docs-merge", action="store_true")
    parser.add_argument("--identical", action="append", dest="identical", default=[])
    parser.add_argument("--skip-source", action="store_true")
    args = parser.parse_args(argv)

    root = (args.root or Path.cwd()).resolve()
    cfg = load_config(root) or {}
    map_paths = [resolve(root, p) for p in (args.maps or cfg.get("maps") or [])]
    source_dirs = [resolve(root, p) for p in (args.sources or cfg.get("sources") or [])]
    allow_docs = args.allow_docs_merge or bool(cfg.get("allow_docs_merge"))
    identical = [resolve(root, p) for p in (args.identical or cfg.get("identical_to") or [])]
    skip_source = args.skip_source or bool(cfg.get("skip_source"))

    if not map_paths:
        parser.error("no --map / config maps")

    schema_path = default_schema(root, args.schema or cfg.get("schema"))
    schema = json.loads(schema_path.read_text(encoding="utf-8")) if schema_path else None

    errors: list[str] = []
    maps: list[tuple[Path, dict[str, Any]]] = []
    for path in map_paths:
        if not path.is_file():
            errors.append(f"missing map {path}")
            continue
        instance = load_map(path)
        maps.append((path, instance))
        if schema is not None:
            errors.extend(jsonschema_validate(instance, schema, str(path)))
        else:
            errors.extend(structural_validate(instance, str(path)))

    twins_by_name = {p.name: p for p in identical if p.is_file()}
    for missing in identical:
        if not missing.is_file():
            errors.append(
                f"identical-to file missing: {missing} (clone the sibling contract repo)"
            )
    for path, instance in maps:
        twin = twins_by_name.get(path.name)
        if twin is None:
            continue
        errors.extend(maps_identical(instance, load_map(twin), str(path), str(twin)))

    if source_dirs and not skip_source:
        scanned, docs_merged = scan_rust_routes(source_dirs)
        if not scanned and not docs_merged:
            errors.append(f"no .route(...) registrations under {source_dirs}")
        for path, instance in maps:
            errors.extend(
                compare(
                    instance,
                    scanned,
                    allow_docs_merge=allow_docs,
                    docs_merged=docs_merged,
                    label=str(path),
                )
            )

    if errors:
        print("route-map sync failed:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        return 1
    print("route-map sync ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
