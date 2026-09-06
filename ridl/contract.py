"""Runtime contract check for a JSON instance against a 2020-12 schema.

Used by unit tests. Prefers the `jsonschema` package; without it, required
keys and additionalProperties:false are still checked so CI cannot silently
skip the contract.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def check_instance(schema: dict[str, Any], instance: Any) -> list[str]:
    try:
        import jsonschema
        from jsonschema import Draft202012Validator
    except ImportError:
        return _structural_check(schema, instance)
    validator = Draft202012Validator(schema)
    return [
        f"{list(err.absolute_path)}: {err.message}"
        for err in validator.iter_errors(instance)
    ]


def check_file(schema_path: Path, instance: Any) -> list[str]:
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    return check_instance(schema, instance)


def _structural_check(schema: dict[str, Any], instance: Any) -> list[str]:
    errors: list[str] = []
    if schema.get("type") == "object" and not isinstance(instance, dict):
        return ["$: expected object"]
    if not isinstance(instance, dict):
        return errors
    for key in schema.get("required", []):
        if key not in instance:
            errors.append(f"{key}: missing required property")
    if schema.get("additionalProperties") is False:
        allowed = set(schema.get("properties", {}))
        for key in instance:
            if key not in allowed:
                errors.append(f"{key}: additional property not in schema")
    return errors
