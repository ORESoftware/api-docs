#!/usr/bin/env python3
"""Fail-closed audit for HTTP operation-routing policy.

Operation identity is only `(HTTP method, URL path/template)`. Query parameters
and headers are request-validation surfaces, never alternate dispatch keys.
This audit complements route-map JSON Schema validation with an explicit policy
veto so a future schema edit cannot accidentally make header/query routing a
supported contract feature.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_ROUTE_FIELDS = {
    "route_by_header",
    "route_by_query",
    "header_routes",
    "query_routes",
    "dispatch_headers",
    "dispatch_query",
    "match_headers",
    "match_query",
}
FORBIDDEN_ANNOTATION = re.compile(
    r"(?i)(route[_-]?by[_-]?(?:header|query)|when[_-]?(?:header|query)|"
    r"dispatch[_-]?(?:header|query)|match[_-]?(?:header|query))"
)
HTTP_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}


def _load(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _infer_methods(key: str) -> list[str]:
    if key and key[0].isupper():
        return ["POST"]
    lower = key.lower()
    if lower.startswith("delete"):
        return ["DELETE"]
    if lower.startswith(("put", "update", "replace")):
        return ["PUT"]
    if lower.startswith("patch"):
        return ["PATCH"]
    if any(token in lower for token in ("create", "walk", "check", "ask")) or lower.startswith(
        ("post", "submit")
    ):
        return ["POST"]
    return ["GET"]


def _schema_errors(instance: dict[str, Any], schema: dict[str, Any], label: str) -> list[str]:
    try:
        from jsonschema import Draft202012Validator
    except ImportError as exc:
        raise RuntimeError("jsonschema is required for routing-policy admission") from exc
    validator = Draft202012Validator(schema)
    return [f"{label}: {error.message} at {error.json_path}" for error in validator.iter_errors(instance)]


def _audit_schema(schema: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    route_object = ((schema.get("$defs") or {}).get("routeObject") or {})
    if route_object.get("additionalProperties") is not False:
        errors.append("routeObject must remain closed with additionalProperties=false")
    properties = route_object.get("properties") or {}
    present = sorted(FORBIDDEN_ROUTE_FIELDS & set(properties))
    if present:
        errors.append(f"routeObject exposes forbidden header/query dispatch fields: {present}")
    return errors


def _audit_map(path: Path, document: dict[str, Any], schema: dict[str, Any]) -> list[str]:
    errors = _schema_errors(document, schema, str(path))
    raw_map = document.get("map")
    if not isinstance(raw_map, dict):
        return errors
    occupied: dict[tuple[str, str], str] = {}
    for key, raw in raw_map.items():
        if isinstance(raw, str):
            route_path = raw
            methods = _infer_methods(key)
            obj: dict[str, Any] = {}
        elif isinstance(raw, dict):
            obj = raw
            route_path = raw.get("path")
            methods = raw.get("methods") or _infer_methods(key)
        else:
            continue
        forbidden = sorted(FORBIDDEN_ROUTE_FIELDS & set(obj))
        if forbidden:
            errors.append(f"{path}.{key}: forbidden dispatch fields {forbidden}")
        binding = obj.get("binding")
        if isinstance(binding, dict):
            annotation = binding.get("annotation")
            if isinstance(annotation, str) and FORBIDDEN_ANNOTATION.search(annotation):
                errors.append(
                    f"{path}.{key}: binding annotation encodes header/query routing: {annotation!r}"
                )
        if not isinstance(route_path, str):
            continue
        if not isinstance(methods, list):
            errors.append(f"{path}.{key}: methods must be an array")
            continue
        for method in methods:
            if method not in HTTP_METHODS:
                continue
            slot = (route_path, method)
            other = occupied.get(slot)
            if other is not None:
                errors.append(
                    f"{path}: {key} and {other} both bind {method} {route_path}; "
                    "query/header values may not disambiguate operations"
                )
            else:
                occupied[slot] = key
    return errors


def run(root: Path | None = None) -> dict[str, Any]:
    root = root or ROOT
    schema_path = root / "json-schema" / "route-map.schema.json"
    schema = _load(schema_path)
    vetoes = _audit_schema(schema)
    audited: list[str] = []
    for path in sorted((root / "examples").glob("*.route-map.json")):
        document = _load(path)
        version = str(document.get("schema_version") or "") if isinstance(document, dict) else ""
        if version.startswith("2."):
            continue
        audited.append(str(path.relative_to(root)))
        if isinstance(document, dict):
            vetoes.extend(_audit_map(path, document, schema))
        else:
            vetoes.append(f"{path}: route map must be an object")
    unique = sorted(set(vetoes))
    return {
        "ok": not unique,
        "routingIdentity": ["http_method", "url_path_template"],
        "requestValidationOnly": ["path_params", "query_params", "headers", "json_body"],
        "auditedMaps": audited,
        "vetoes": unique,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    report = run(args.root)
    print(json.dumps(report, indent=2, sort_keys=True))
    if report["vetoes"]:
        print("HTTP routing-policy veto", file=sys.stderr)
        for veto in report["vetoes"]:
            print(f"  {veto}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
