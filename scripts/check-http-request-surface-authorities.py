#!/usr/bin/env python3
"""Fail closed when the independent HTTP request-surface authorities drift."""
from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DRAFT = "https://json-schema.org/draft/2020-12/schema"
EXPECTED_DECLARATIONS = ("HttpMethod", "RequestValueMap", "RequestSurface")
EXPECTED_FIELDS = ("method", "pathTemplate", "path", "query", "headers", "body")
EXPECTED_REQUIRED = ("method", "pathTemplate")
EXPECTED_VALIDATION_ONLY = ("path", "query", "headers", "body")
EXPECTED_METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")
EXPECTED_PATH_PATTERN = r"^/[^\s]*$"
EXPECTED_HEADER_PATTERN = r"^[!#$%&'*+.^_`|~0-9a-z-]+$"
EXPECTED_TSP_TYPES = {
    "method": "HttpMethod",
    "pathTemplate": "string",
    "path": "RequestValueMap",
    "query": "RequestValueMap",
    "headers": "RequestValueMap",
    "body": "unknown",
}
EXPECTED_VALUE_MAP: dict[str, Any] = {
    "type": "object",
    "properties": {},
    "unevaluatedProperties": {},
}
EXPECTED_JSON_FIELDS: dict[str, Any] = {
    "method": {"$ref": "HttpMethod"},
    "pathTemplate": {
        "type": "string",
        "minLength": 1,
        "pattern": EXPECTED_PATH_PATTERN,
    },
    "path": {"$ref": "RequestValueMap"},
    "query": {"$ref": "RequestValueMap"},
    "headers": {
        "$ref": "RequestValueMap",
        "propertyNames": {"pattern": EXPECTED_HEADER_PATTERN},
    },
    "body": {},
}

MODEL_RE = re.compile(r"model\s+RequestSurface\s*\{(?P<body>.*?)\n\}", re.S)
VALUE_MAP_RE = re.compile(
    r"model\s+RequestValueMap\s+is\s+Record<unknown>\s*\{\s*\}", re.S
)
ENUM_RE = re.compile(r"enum\s+HttpMethod\s*\{(?P<body>.*?)\n\}", re.S)
FIELD_RE = re.compile(r"^\s*([A-Za-z][A-Za-z0-9]*)(\?)?:\s*([^;]+);\s*$")
DECORATOR_RE = re.compile(r"^\s*@([A-Za-z][A-Za-z0-9]*)(?:\((.*)\))?\s*$")


def _compact(value: str | None) -> str | None:
    if value is None:
        return None
    return re.sub(r"\s+", "", value)


def _parse_decorator(
    name: str,
    argument: str | None,
    where: str,
    errors: list[str],
) -> Any:
    if name == "jsonSchema":
        if argument is not None:
            errors.append(f"{where}: @jsonSchema must not have arguments")
        return True
    if name == "id":
        if argument is None:
            errors.append(f"{where}: @id requires one JSON string argument")
            return None
        try:
            value = json.loads(argument)
        except json.JSONDecodeError:
            errors.append(f"{where}: @id argument is not a JSON string")
            return None
        if not isinstance(value, str) or not value:
            errors.append(f"{where}: @id requires a non-empty string")
            return None
        return value
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
    if name == "extension":
        if argument is None or "," not in argument:
            errors.append(f"{where}: @extension requires a key and value")
            return None
        return _compact(argument)
    errors.append(f"{where}: unreviewed TypeSpec decorator @{name}")
    return None


def _leading_decorators(
    tsp: str,
    declaration: str,
    errors: list[str],
) -> list[tuple[str, Any]]:
    match = re.search(
        rf"(?P<decorators>(?:^[ \t]*@[^\n]+\n)+)^[ \t]*{re.escape(declaration)}\b",
        tsp,
        re.M,
    )
    if not match:
        errors.append(f"TypeSpec {declaration} decorators not found")
        return []
    parsed: list[tuple[str, Any]] = []
    for raw in match.group("decorators").splitlines():
        decorator = DECORATOR_RE.fullmatch(raw)
        if not decorator:
            errors.append(f"TypeSpec {declaration}: unparsed decorator {raw!r}")
            continue
        name = decorator.group(1)
        value = _parse_decorator(
            name,
            decorator.group(2),
            f"TypeSpec {declaration}",
            errors,
        )
        parsed.append((name, value))
    return parsed


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
                f"TypeSpec RequestSurface line {line_number}: "
                f"unparsed declaration {line!r}"
            )
            pending.clear()
            continue

        name = field.group(1)
        if name in fields:
            errors.append(f"TypeSpec RequestSurface has duplicate field {name!r}")
        decorators: list[tuple[str, Any]] = []
        for decorator_name, argument in pending:
            decorators.append(
                (
                    decorator_name,
                    _parse_decorator(
                        decorator_name,
                        argument,
                        f"TypeSpec RequestSurface.{name}",
                        errors,
                    ),
                )
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


def _definitions(schema: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    definitions = schema.get("$defs")
    if not isinstance(definitions, dict):
        errors.append("JSON Schema request authority needs a $defs object")
        return {}
    if tuple(definitions) != EXPECTED_DECLARATIONS:
        errors.append(
            f"JSON Schema declaration inventory {tuple(definitions)} "
            f"!= {EXPECTED_DECLARATIONS}"
        )
    return definitions


def _assert_definition(
    definitions: dict[str, Any],
    name: str,
    expected: dict[str, Any],
    errors: list[str],
) -> dict[str, Any]:
    definition = definitions.get(name)
    if not isinstance(definition, dict):
        errors.append(f"JSON Schema {name} definition missing")
        return {}
    if definition.get("$schema") != DRAFT:
        errors.append(f"JSON Schema {name} must use Draft 2020-12")
    if definition.get("$id") != name:
        errors.append(f"JSON Schema {name} $id must be {name!r}")
    assertions = {
        key: value
        for key, value in definition.items()
        if key not in {"$schema", "$id", "title", "description"}
    }
    if assertions != expected:
        errors.append(f"JSON Schema {name} shape drifted: {assertions!r} != {expected!r}")
    return definition


def _audit_delta_ledger(root: Path, errors: list[str]) -> None:
    path = root / "idl/http-request-surface.expected-deltas.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("formatVersion") != 1:
        errors.append("request-surface delta ledger formatVersion must be 1")
    if not isinstance(document.get("philosophy"), str) or not document["philosophy"].strip():
        errors.append("request-surface delta ledger philosophy is required")
    raw = document.get("deltas")
    if raw != []:
        errors.append(
            "request-surface authorities must have zero active expected deltas; "
            f"found {raw!r}"
        )


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

    if schema.get("$schema") != DRAFT:
        errors.append("JSON Schema request authority must use Draft 2020-12")
    if schema.get("$ref") != "#/$defs/RequestSurface":
        errors.append("JSON Schema root must resolve to $defs/RequestSurface")

    definitions = _definitions(schema, errors)
    _assert_definition(
        definitions,
        "HttpMethod",
        {"type": "string", "enum": list(EXPECTED_METHODS)},
        errors,
    )
    _assert_definition(definitions, "RequestValueMap", EXPECTED_VALUE_MAP, errors)
    request = definitions.get("RequestSurface")
    if not isinstance(request, dict):
        errors.append("JSON Schema RequestSurface definition missing")
        request = {}
    if request.get("$schema") != DRAFT:
        errors.append("JSON Schema RequestSurface must use Draft 2020-12")
    if request.get("$id") != "RequestSurface":
        errors.append("JSON Schema RequestSurface $id must be 'RequestSurface'")
    if request.get("type") != "object":
        errors.append("JSON Schema request envelope must be an object")
    if request.get("unevaluatedProperties") is not False:
        errors.append("JSON Schema request envelope must be closed")
    if "additionalProperties" in request:
        errors.append(
            "JSON Schema request envelope must use unevaluatedProperties, "
            "matching the TypeSpec emitter"
        )

    properties = request.get("properties")
    if not isinstance(properties, dict):
        errors.append("JSON Schema RequestSurface needs a properties object")
        properties = {}
    if tuple(properties) != EXPECTED_FIELDS:
        errors.append(f"JSON Schema fields {tuple(properties)} != {EXPECTED_FIELDS}")
    if tuple(request.get("required", ())) != EXPECTED_REQUIRED:
        errors.append("JSON Schema required fields must be method + pathTemplate")
    if tuple(request.get("x-ores-routing-identity", ())) != EXPECTED_REQUIRED:
        errors.append("routing identity must be method + pathTemplate only")
    if tuple(request.get("x-ores-validation-only", ())) != EXPECTED_VALIDATION_ONLY:
        errors.append("validation-only fields must be path + query + headers + body")
    for name, expected in EXPECTED_JSON_FIELDS.items():
        if properties.get(name) != expected:
            errors.append(
                f"JSON Schema field {name!r} shape "
                f"{properties.get(name)!r} != {expected!r}"
            )

    if 'import "@typespec/json-schema";' not in tsp:
        errors.append("TypeSpec request authority must import @typespec/json-schema")
    if "using TypeSpec.JsonSchema;" not in tsp:
        errors.append("TypeSpec request authority must use TypeSpec.JsonSchema")
    if "namespace Ores.Http.RequestSurface.V1;" not in tsp:
        errors.append("TypeSpec request authority namespace drifted")

    expected_simple_decorators = {
        "enum HttpMethod": [("jsonSchema", True), ("id", "HttpMethod")],
        "model RequestValueMap": [
            ("jsonSchema", True),
            ("id", "RequestValueMap"),
        ],
    }
    for declaration, expected in expected_simple_decorators.items():
        actual = _leading_decorators(tsp, declaration, errors)
        if actual != expected:
            errors.append(f"TypeSpec {declaration} decorators {actual!r} != {expected!r}")
    if not VALUE_MAP_RE.search(tsp):
        errors.append("TypeSpec RequestValueMap must be exactly an open Record<unknown> model")

    model_decorators = _leading_decorators(tsp, "model RequestSurface", errors)
    expected_model_decorators = [
        ("jsonSchema", True),
        ("id", "RequestSurface"),
        ("extension", '"unevaluatedProperties",false'),
        (
            "extension",
            '"x-ores-routing-identity",#["method","pathTemplate"]',
        ),
        (
            "extension",
            '"x-ores-validation-only",#["path","query","headers","body"]',
        ),
    ]
    if model_decorators != expected_model_decorators:
        errors.append(
            f"TypeSpec RequestSurface decorators {model_decorators!r} "
            f"!= {expected_model_decorators!r}"
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
                f"TypeSpec RequestSurface.{name} type "
                f"{actual!r} != {expected_type!r}"
            )

    expected_path_decorators = [
        ("minLength", 1),
        ("pattern", EXPECTED_PATH_PATTERN),
    ]
    actual_path_decorators = fields.get("pathTemplate", {}).get("decorators", [])
    if actual_path_decorators != expected_path_decorators:
        errors.append(
            "TypeSpec RequestSurface.pathTemplate decorators "
            f"{actual_path_decorators!r} != {expected_path_decorators!r}"
        )

    expected_header_decorators = [
        (
            "extension",
            '"propertyNames",#{pattern:'
            f'"{EXPECTED_HEADER_PATTERN}"'
            "}",
        )
    ]
    actual_header_decorators = fields.get("headers", {}).get("decorators", [])
    if actual_header_decorators != expected_header_decorators:
        errors.append(
            "TypeSpec RequestSurface.headers decorators "
            f"{actual_header_decorators!r} != {expected_header_decorators!r}"
        )
    for name in ("method", "path", "query", "body"):
        decorators = fields.get(name, {}).get("decorators", [])
        if decorators:
            errors.append(
                f"TypeSpec RequestSurface.{name} has unreviewed decorators "
                f"{decorators!r}"
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
    for token in forbidden:
        if token in tsp:
            errors.append(f"TypeSpec request authority exposes forbidden {token}")

    _audit_delta_ledger(root, errors)
    return errors


def main() -> int:
    errors = audit()
    if errors:
        print("HTTP request-surface authority mismatch:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("HTTP request-surface TypeSpec/JSON Schema peers agree without waivers")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
