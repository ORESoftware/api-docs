#!/usr/bin/env python3
"""Fail closed when the independent TypeSpec/JSON Schema request peers drift."""
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_FIELDS = ("method", "pathTemplate", "path", "query", "headers", "body")
EXPECTED_REQUIRED = ("method", "pathTemplate")
EXPECTED_METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")


def audit(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    schema = json.loads((root / "json-schema/http-request-surface.schema.json").read_text())
    tsp = (root / "idl/typespec/http/request-surface.tsp").read_text()

    properties = tuple(schema.get("properties", {}).keys())
    if properties != EXPECTED_FIELDS:
        errors.append(f"JSON Schema fields {properties} != {EXPECTED_FIELDS}")
    if tuple(schema.get("required", ())) != EXPECTED_REQUIRED:
        errors.append("JSON Schema required fields must be method + pathTemplate")
    if schema.get("additionalProperties") is not False:
        errors.append("JSON Schema request envelope must be closed")
    if tuple(schema.get("x-ores-routing-identity", ())) != EXPECTED_REQUIRED:
        errors.append("routing identity must be method + pathTemplate only")
    if tuple(schema.get("properties", {}).get("method", {}).get("enum", ())) != EXPECTED_METHODS:
        errors.append("JSON Schema HTTP method enum drift")

    model = re.search(r"model\s+RequestSurface\s*\{(?P<body>.*?)\n\}", tsp, re.S)
    if not model:
        errors.append("TypeSpec RequestSurface model not found")
    else:
        fields = tuple(re.findall(r"^\s*([A-Za-z][A-Za-z0-9]*)(?:\?)?:", model.group("body"), re.M))
        if fields != EXPECTED_FIELDS:
            errors.append(f"TypeSpec fields {fields} != {EXPECTED_FIELDS}")
        optional = set(re.findall(r"^\s*([A-Za-z][A-Za-z0-9]*)\?:", model.group("body"), re.M))
        required = tuple(field for field in fields if field not in optional)
        if required != EXPECTED_REQUIRED:
            errors.append(f"TypeSpec required fields {required} != {EXPECTED_REQUIRED}")

    enum = re.search(r"enum\s+HttpMethod\s*\{(?P<body>.*?)\n\}", tsp, re.S)
    if not enum:
        errors.append("TypeSpec HttpMethod enum not found")
    else:
        methods = tuple(re.findall(r"^\s*([A-Z]+),?\s*$", enum.group("body"), re.M))
        if methods != EXPECTED_METHODS:
            errors.append(f"TypeSpec HTTP methods {methods} != {EXPECTED_METHODS}")

    forbidden = {"routeByHeader", "routeByQuery", "dispatchHeaders", "dispatchQuery"}
    if forbidden & set(properties):
        errors.append("request authority exposes forbidden dispatch selectors")
    return errors


def main() -> int:
    errors = audit()
    if errors:
        print("HTTP request-surface authority mismatch:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("HTTP request-surface TypeSpec/JSON Schema peers agree")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
