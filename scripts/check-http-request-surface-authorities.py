#!/usr/bin/env python3
"""Fail closed when the independent TypeSpec/JSON Schema request peers drift."""
from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_FIELDS = ("method", "pathTemplate", "path", "query", "headers", "body")
EXPECTED_REQUIRED = ("method", "pathTemplate")
EXPECTED_VALIDATION_ONLY = ("path", "query", "headers", "body")
EXPECTED_METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")
EXPECTED_PATH_PATTERN = r"^/[^\s]*$"
EXPECTED_HEADER_PATTERN = r"^[!#$%&'*+.^_`|~0-9a-z-]+$"
EXPECTED_TSP_TYPES = {
    "method": "HttpMethod",
    "pathTemplate": "string",
    "path": "Record<unknown>",
    "query": "Record<unknown>",
    "headers": "Record<unknown>",
    "body": "unknown",
}
EXPECTED_JSON_FIELDS: dict[str, Any] = {
    "method": {"type": "string", "enum": list(EXPECTED_METHODS)},
    "pathTemplate": {
        "type": "string",
        "minLength": 1,
        "pattern": EXPECTED_PATH_PATTERN,
    },
    "path": {"type": "object"},
    "query": {"type": "object"},
    "headers": {
        "type": "object",
        "propertyNames": {"pattern": EXPECTED_HEADER_PATTERN},
    },
    "body": True,
}
EXPECTED_DELTAS = {
    "http-request-surface-additional-properties": {
        "id": "http-request-surface-additional-properties",
        "kind": "constraint_absent",
        "field": "additionalProperties",
        "left": "json-schema:http-request-surface",
        "right": "typespec:Ores.Http.RequestSurface.V1.RequestSurface",
        "reason": (
            "JSON Schema closes the parsed request envelope. The authored TypeSpec "
            "model has no equivalent additionalProperties=false constraint, so runtime "
            "admission must preserve JSON Schema closedness."
        ),
    },
    "http-request-surface-header-property-names": {
        "id": "http-request-surface-header-property-names",
        "kind": "constraint_absent",
        "field": "headers.propertyNames.pattern",
        "left": "json-schema:http-request-surface.headers",
        "right": "typespec:Ores.Http.RequestSurface.V1.RequestSurface.headers",
        "reason": (
            "JSON Schema constrains canonical lower-case HTTP header names. TypeSpec "
            "Record<unknown> cannot attach a pattern to record keys; RIDL semantic "
            "validation and generated per-operation schemas enforce the same rule."
        ),
    },
}

MODEL_RE = re.compile(r"model\s+RequestSurface\s*\{(?P<body>.*?)\n\}", re.S)
ENUM_RE = re.compile(r"enum\s+HttpMethod\s*\{(?P<body>.*?)\n\}", re.S)
FIELD_RE = re.compile(
    r"^\s*([A-Za-z][A-Za-z0-9]*)(\?)?:\s*([^;]+);\s*$"
)
DECORATOR_RE = re.compile(r"^\s*@([A-Za-z][A-Za-z0-9]*)(?:\((.*)\))?\s*$")


def _parse_decorator(name: str, argument: str | None, where: str, errors: list[str]) -> Any:
    if name == "minLength":
        if argument is None or not re.fullmatch(r"[0-9]+", argument.strip()):
            errors.append(f"{where}: @minLength must have one integer argument")
            return None
        return int(argument)
    if name == "pattern":
        if argument is None:
            errors.append(f"{where}: @pattern must have one JSON string argument")
            return None
        try:
            value = json.loads(argument)
        except json.JSONDecodeError:
            errors.append(f"{where}: @pattern argument is not a JSON string")
            return None
        if not isinstance(value, str):
            errors.append(f"{where}: @pattern argument must decode to a string")
            return None
        return value
    errors.append(f"{where}: unreviewed TypeSpec decorator @{name}")
    return None


def _parse_typespec_fields(tsp: str, errors: list[str]) -> dict[str, dict[str, Any]]:
    model = MODEL_RE.search(tsp)
    if not model:
        errors.append("TypeSpec RequestSurface model not found")
        return {}

    fields: dict[str, dict[str, Any]] = {}
    pending: list[tuple[str, str | None]] = []
    for line_number, raw in enumerate(model.group("body").splitlines(), 1):
        line = raw.split("//", 1)[0].strip()
        if not line:
            continue
        decorator = DECORATOR_RE.fullmatch(line)
        if decorator:
            pending.append((decorator.group(1), decorator.group(2)))
            continue
        field = FIELD_RE.fullmatch(line)
        if not field:
            errors.append(
                f"TypeSpec RequestSurface line {line_number}: unparsed declaration {line!r}"
            )
            pending.clear()
            continue

        name = field.group(1)
        if name in fields:
            errors.append(f"TypeSpec RequestSurface has duplicate field {name!r}")
        decorators: dict[str, Any] = {}
        for decorator_name, argument in pending:
            if decorator_name in decorators:
                errors.append(
                    f"TypeSpec RequestSurface.{name}: duplicate @{decorator_name}"
                )
                continue
            decorators[decorator_name] = _parse_decorator(
                decorator_name,
                argument,
                f"TypeSpec RequestSurface.{name}",
                errors,
            )
        pending.clear()
        fields[name] = {
            "optional": field.group(2) == "?",
            "type": field.group(3).strip(),
            "decorators": decorators,
        }
    if pending:
        errors.append("TypeSpec RequestSurface has decorators not attached to a field")
    return fields


def _audit_delta_ledger(root: Path, errors: list[str]) -> None:
    path = root / "idl/http-request-surface.expected-deltas.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("formatVersion") != 1:
        errors.append("request-surface delta ledger formatVersion must be 1")
    raw = document.get("deltas")
    if not isinstance(raw, list):
        errors.append("request-surface delta ledger must contain a deltas array")
        return

    by_id: dict[str, dict[str, Any]] = {}
    for index, entry in enumerate(raw):
        if not isinstance(entry, dict) or not isinstance(entry.get("id"), str):
            errors.append(f"request-surface delta ledger entry {index} needs a string id")
            continue
        delta_id = entry["id"]
        if delta_id in by_id:
            errors.append(f"request-surface delta ledger has duplicate id {delta_id!r}")
        by_id[delta_id] = entry

    if set(by_id) != set(EXPECTED_DELTAS):
        errors.append(
            "request-surface expected delta ids "
            f"{sorted(by_id)} != {sorted(EXPECTED_DELTAS)}"
        )
    for delta_id, expected in EXPECTED_DELTAS.items():
        if by_id.get(delta_id) != expected:
            errors.append(f"request-surface delta entry {delta_id!r} drifted")


def audit(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    schema = json.loads(
        (root / "json-schema/http-request-surface.schema.json").read_text(
            encoding="utf-8"
        )
    )
    tsp = (root / "idl/typespec/http/request-surface.tsp").read_text(
        encoding="utf-8"
    )

    if schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
        errors.append("JSON Schema request authority must use Draft 2020-12")
    if schema.get("type") != "object":
        errors.append("JSON Schema request envelope must be an object")
    if schema.get("additionalProperties") is not False:
        errors.append("JSON Schema request envelope must be closed")

    properties = schema.get("properties")
    if not isinstance(properties, dict):
        errors.append("JSON Schema request authority needs a properties object")
        properties = {}
    if tuple(properties) != EXPECTED_FIELDS:
        errors.append(f"JSON Schema fields {tuple(properties)} != {EXPECTED_FIELDS}")
    if tuple(schema.get("required", ())) != EXPECTED_REQUIRED:
        errors.append("JSON Schema required fields must be method + pathTemplate")
    if tuple(schema.get("x-ores-routing-identity", ())) != EXPECTED_REQUIRED:
        errors.append("routing identity must be method + pathTemplate only")
    if tuple(schema.get("x-ores-validation-only", ())) != EXPECTED_VALIDATION_ONLY:
        errors.append(
            "validation-only fields must be path + query + headers + body"
        )
    for name, expected in EXPECTED_JSON_FIELDS.items():
        if properties.get(name) != expected:
            errors.append(
                f"JSON Schema field {name!r} shape {properties.get(name)!r} != {expected!r}"
            )

    fields = _parse_typespec_fields(tsp, errors)
    if tuple(fields) != EXPECTED_FIELDS:
        errors.append(f"TypeSpec fields {tuple(fields)} != {EXPECTED_FIELDS}")
    required = tuple(
        name for name, field in fields.items() if not bool(field.get("optional"))
    )
    if required != EXPECTED_REQUIRED:
        errors.append(f"TypeSpec required fields {required} != {EXPECTED_REQUIRED}")
    for name, expected_type in EXPECTED_TSP_TYPES.items():
        actual = fields.get(name, {}).get("type")
        if actual != expected_type:
            errors.append(
                f"TypeSpec RequestSurface.{name} type {actual!r} != {expected_type!r}"
            )

    expected_path_decorators = {
        "minLength": 1,
        "pattern": EXPECTED_PATH_PATTERN,
    }
    actual_path_decorators = fields.get("pathTemplate", {}).get("decorators", {})
    if actual_path_decorators != expected_path_decorators:
        errors.append(
            "TypeSpec RequestSurface.pathTemplate decorators "
            f"{actual_path_decorators!r} != {expected_path_decorators!r}"
        )
    for name in ("method", "path", "query", "headers", "body"):
        decorators = fields.get(name, {}).get("decorators", {})
        if decorators:
            errors.append(
                f"TypeSpec RequestSurface.{name} has unreviewed decorators {decorators!r}"
            )

    enum = ENUM_RE.search(tsp)
    if not enum:
        errors.append("TypeSpec HttpMethod enum not found")
    else:
        methods = tuple(
            re.findall(r"^\s*([A-Z]+),?\s*$", enum.group("body"), re.M)
        )
        if methods != EXPECTED_METHODS:
            errors.append(f"TypeSpec HTTP methods {methods} != {EXPECTED_METHODS}")

    forbidden = {"routeByHeader", "routeByQuery", "dispatchHeaders", "dispatchQuery"}
    if forbidden & set(properties):
        errors.append("request authority exposes forbidden dispatch selectors")

    _audit_delta_ledger(root, errors)
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
