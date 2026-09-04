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
AXUM_COLON_PARAM = re.compile(r":([A-Za-z_][A-Za-z0-9_]*)")

# PascalCase Connect method key
PASCAL = re.compile(r"^[A-Z][A-Za-z0-9]*$")
KEY_OK = re.compile(r"^[A-Za-z][A-Za-z0-9_]*$")
PATH_OK = re.compile(r"^/\S*$")
PATH_VAR = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")
HTTP_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
HTTP_HEADER_NAME_RE = re.compile(r"^[!#$%&'*+.^_`|~0-9a-z-]+$")
RUNTIME_OWNED_REQUEST_HEADERS = {
    "authorization", "baggage", "connection", "content-encoding",
    "content-length", "content-type", "cookie", "forwarded", "host",
    "keep-alive", "proxy-authenticate", "proxy-authorization", "set-cookie",
    "te", "traceparent", "tracestate", "trailer", "transfer-encoding",
    "upgrade", "x-forwarded-for", "x-forwarded-host", "x-forwarded-proto",
    "x-real-ip",
}


def path_template_vars(path: str) -> list[str]:
    if path.count("{") != path.count("}"):
        raise SystemExit(f"unbalanced braces in path {path}")
    vars_: list[str] = []
    seen: set[str] = set()
    for match in PATH_VAR.finditer(path):
        name = match.group(1)
        if name in seen:
            raise SystemExit(f"duplicate path placeholder {{{name}}} in {path}")
        seen.add(name)
        vars_.append(name)
    if "{" in PATH_VAR.sub("", path) or "}" in PATH_VAR.sub("", path):
        raise SystemExit(f"invalid path placeholders in {path}")
    return vars_


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


def infer_transports(key: str, path: str) -> list[str]:
    lower = (key or "").lower()
    if path in ("/ws", "/websocket") or "websocket" in lower:
        return ["websocket"]
    return ["http"]


def normalize_entry(key: str, value: Any) -> dict[str, Any]:
    if isinstance(value, str):
        return {
            "path": value,
            "methods": infer_methods(key),
            "transports": infer_transports(key, value),
        }
    if isinstance(value, dict) and isinstance(value.get("path"), str):
        methods = value.get("methods") or infer_methods(key)
        transports = value.get("transports") or infer_transports(key, value["path"])
        if not isinstance(transports, list) or not transports:
            raise SystemExit(f"{key}: transports must be a non-empty array")
        for item in transports:
            if item not in ("http", "tcp", "websocket", "nats"):
                raise SystemExit(f"{key}: unknown transport {item!r}")
        return {
            "path": value["path"],
            "methods": list(methods),
            "transports": list(transports),
        }
    raise SystemExit(f"{key}: expected path string or object with path")


OPTO_TABLE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{0,62}$")
MUTATING = {"POST", "PUT", "PATCH", "DELETE"}


def _delivery_errors(label: str, key: str, value: dict[str, Any], entry: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    delivery = value.get("delivery") or "direct"
    opto = value.get("opto_sync")
    if delivery not in ("direct", "opto_sync_queued"):
        errors.append(f"{label}.{key}: delivery must be direct or opto_sync_queued")
        return errors
    if delivery == "direct":
        if opto is not None:
            errors.append(f"{label}.{key}: opto_sync settings require delivery: opto_sync_queued")
        return errors
    if any(m not in MUTATING for m in entry["methods"]):
        errors.append(f"{label}.{key}: only mutating methods can be queued through opto-sync")
    if not isinstance(opto, dict):
        errors.append(f"{label}.{key}: delivery opto_sync_queued requires an opto_sync block")
        return errors
    table = opto.get("table")
    if not isinstance(table, str) or not OPTO_TABLE.match(table):
        errors.append(f"{label}.{key}: opto_sync.table is not a SQL-safe identifier")
    op = opto.get("operation")
    if op not in ("upsert", "delete"):
        errors.append(f"{label}.{key}: opto_sync.operation must be upsert or delete")
    elif op == "upsert" and not isinstance(value.get("request_schema"), dict):
        errors.append(f"{label}.{key}: a queued upsert needs a request_schema")
    elif op == "delete" and value.get("request_schema") is not None:
        errors.append(f"{label}.{key}: a queued delete must not carry a request body")
    return errors


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
        try:
            vars_ = path_template_vars(entry["path"])
        except SystemExit as exc:
            errors.append(f"{label}.{key}: {exc}")
            vars_ = []
        for method in entry["methods"]:
            if method not in HTTP_METHODS:
                errors.append(f"{label}.{key}: bad method {method}")
        if PASCAL.match(key) and any(m != "POST" for m in entry["methods"]):
            errors.append(f"{label}.{key}: Connect JSON unary keys must be POST-only")
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
        if isinstance(value, dict):
            path_params = value.get("path_params")
            if isinstance(path_params, dict):
                props = path_params.get("properties")
                if not isinstance(props, dict):
                    errors.append(f"{label}.{key}: path_params needs properties")
                elif set(props) != set(vars_):
                    errors.append(
                        f"{label}.{key}: path_params {sorted(props)} != template {vars_}"
                    )
            header_schema = value.get("header_schema")
            if isinstance(header_schema, dict):
                if set(entry["transports"]) != {"http"}:
                    errors.append(
                        f"{label}.{key}: request headers are HTTP-only; use an HTTP-only operation"
                    )
                if header_schema.get("type") != "object":
                    errors.append(f"{label}.{key}: header_schema.type must be object")
                if header_schema.get("additionalProperties") is not False:
                    errors.append(
                        f"{label}.{key}: header_schema must set additionalProperties: false"
                    )
                properties = header_schema.get("properties")
                if not isinstance(properties, dict):
                    errors.append(f"{label}.{key}: header_schema needs properties")
                    properties = {}
                required = header_schema.get("required") or []
                if not isinstance(required, list) or not set(required).issubset(properties):
                    errors.append(
                        f"{label}.{key}: header_schema.required must name declared properties"
                    )
                for header_name in properties:
                    if not HTTP_HEADER_NAME_RE.fullmatch(header_name):
                        errors.append(
                            f"{label}.{key}.header_schema.{header_name}: header name must be canonical lower-case"
                        )
                    if header_name in RUNTIME_OWNED_REQUEST_HEADERS:
                        errors.append(
                            f"{label}.{key}.header_schema.{header_name}: runtime-owned header is forbidden"
                        )
            alias = value.get("alias_of")
            if isinstance(alias, str) and alias not in raw:
                errors.append(f"{label}.{key}: alias_of {alias!r} is not a map key")
            errors.extend(_delivery_errors(label, key, value, entry))
            if entry["transports"] == ["nats"] and isinstance(value.get("query_schema"), dict):
                errors.append(
                    f"{label}.{key}: query parameters have no NATS encoding; add http or tcp"
                )
    occupied: dict[tuple[str, str], str] = {}
    if isinstance(raw, dict):
        for key, value in raw.items():
            try:
                entry = normalize_entry(key, value)
            except SystemExit:
                continue
            for method in entry["methods"]:
                slot = (entry["path"], method)
                other = occupied.get(slot)
                if other:
                    errors.append(
                        f"{label}: {key} and {other} both bind {method} {entry['path']}"
                    )
                else:
                    occupied[slot] = key
    return errors


def jsonschema_validate(instance: dict[str, Any], schema: dict[str, Any], label: str) -> list[str]:
    try:
        import jsonschema
    except ImportError:
        return structural_validate(instance, label)

    validator_cls = getattr(jsonschema, "Draft202012Validator", jsonschema.Draft7Validator)
    validator = validator_cls(schema)
    return [f"{label}: {e.message} at {e.json_path}" for e in validator.iter_errors(instance)]


def matching_paren(text: str, open_idx: int) -> int | None:
    """`open_idx` points at `(`. Returns the matching `)` or None if unbalanced."""
    depth = 0
    i = open_idx
    in_str = False
    escape = False
    while i < len(text):
        ch = text[i]
        if in_str:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == '"':
                in_str = False
            i += 1
            continue
        if ch == '"':
            in_str = True
            i += 1
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def normalize_axum_path(path: str) -> str:
    """Axum 0.7 `:id` and 0.8 `{id}` are the same placeholder."""
    return AXUM_COLON_PARAM.sub(r"{\1}", path)


def scan_rust_routes(source_dirs: Iterable[Path]) -> tuple[dict[str, set[str]], bool]:
    """path -> methods found in .route(...) calls. Also whether docs::router() is merged.

    Parses with balanced parentheses so a rustfmt wrap does not turn a registered
    route into a missing one. Colon params are normalized to `{name}`.
    """
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
            i = 0
            while True:
                idx = text.find(".route(", i)
                if idx < 0:
                    break
                open_idx = idx + 6  # '(' of `.route(`
                close = matching_paren(text, open_idx)
                if close is None:
                    i = idx + 7
                    continue
                args = text[open_idx + 1 : close]
                i = close + 1
                lit = re.match(r"""\s*["']([^"']+)["']""", args)
                if not lit:
                    continue
                route_path = normalize_axum_path(lit.group(1))
                methods = {m.upper() for m in METHOD_CALL.findall(args)}
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
        if str(instance.get("schema_version") or "").startswith("2."):
            print(
                f"note: skipping RIDL v2 map {path} (use python3 -m ridl.cli check)",
                file=sys.stderr,
            )
            continue
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
