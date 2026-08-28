"""Project route-map types to JSON Schema 2020-12.

The language emitters are compile-time. This schema is the runtime contract:
`ridl check` already validates the map; generated clients should `validate()`
payloads against these documents (or the equivalent generated checker).
"""

from __future__ import annotations

import json

from ..model import (
    BUILTINS,
    AliasDef,
    EnumDef,
    ListOf,
    MapOf,
    Named,
    OptionOf,
    RecordDef,
    RouteMap,
    ScalarDef,
    TypeExpr,
)
from .base import Emitted, ordered_types

DRAFT = "https://json-schema.org/draft/2020-12/schema"


def emit(rmap: RouteMap) -> list[Emitted]:
    out: list[Emitted] = []
    defs: dict[str, object] = {}
    for name in ordered_types(rmap):
        defs[name] = _defn_schema(rmap, rmap.types[name])
    index = {
        "$schema": DRAFT,
        "$id": _id(rmap, "index.json"),
        "title": f"{rmap.service} types",
        "type": "object",
        "$defs": defs,
    }
    out.append(
        Emitted(
            path="json-schema/index.json",
            text=json.dumps(index, indent=2) + "\n",
        )
    )
    for name, schema in defs.items():
        doc = {
            "$schema": DRAFT,
            "$id": _id(rmap, f"{name}.schema.json"),
            "title": name,
            **schema,
        }
        out.append(
            Emitted(
                path=f"json-schema/{name}.schema.json",
                text=json.dumps(doc, indent=2) + "\n",
            )
        )
    return out


def _id(rmap: RouteMap, file: str) -> str:
    return f"https://github.com/oresoftware/api-docs/generated/json-schema/{rmap.service}/{file}"


def _defn_schema(rmap: RouteMap, defn: object) -> dict:
    if isinstance(defn, RecordDef):
        properties = {}
        required = []
        for fld in defn.fields:
            properties[fld.wire] = _type_schema(rmap, fld.type)
            if fld.required and not fld.has_default:
                required.append(fld.wire)
        result = {
            "type": "object",
            "additionalProperties": False,
            "properties": properties,
        }
        if required:
            result["required"] = required
        if defn.doc:
            result["description"] = defn.doc
        return result
    if isinstance(defn, EnumDef):
        return {"type": "string", "enum": list(defn.variants)}
    if isinstance(defn, ScalarDef):
        builtin = BUILTINS[defn.base]
        schema: dict = {"type": builtin.json_type}
        if builtin.json_format:
            schema["format"] = builtin.json_format
        return schema
    if isinstance(defn, AliasDef):
        return _type_schema(rmap, defn.target)
    return {"type": "object"}


def _type_schema(rmap: RouteMap, expr: TypeExpr) -> dict:
    if isinstance(expr, OptionOf):
        return {"anyOf": [_type_schema(rmap, expr.inner), {"type": "null"}]}
    if isinstance(expr, ListOf):
        return {"type": "array", "items": _type_schema(rmap, expr.item)}
    if isinstance(expr, MapOf):
        return {
            "type": "object",
            "additionalProperties": _type_schema(rmap, expr.value),
        }
    if isinstance(expr, Named):
        if expr.name in BUILTINS:
            builtin = BUILTINS[expr.name]
            schema = {"type": builtin.json_type}
            if builtin.json_format:
                schema["format"] = builtin.json_format
            return schema
        return {"$ref": f"#/$defs/{expr.name}"}
    return {}
