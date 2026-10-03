"""Normalized v1 RPC contract and stable public contract identity."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
SCHEMA_FIELDS = (
    "path_params",
    "query_schema",
    "request_schema",
    "response_schema",
    "error_schema",
)
EXPECTED_RIDL_EMITTERS = (
    "dart",
    "gleam",
    "go",
    "kotlin",
    "python",
    "rust",
    "swift",
    "typescript",
)


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


_ROUTE_SYNC = _load_module(
    "ores_api_docs_route_sync", ROOT / "scripts" / "check-route-sync.py"
)
_ROUTE_GEN = _load_module(
    "ores_api_docs_route_gen", ROOT / "scripts" / "generate-routes.py"
)


class ContractError(ValueError):
    """The route map cannot produce one coherent cross-language contract."""


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def sha256_hex(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def _meta_schema_errors(schema: dict[str, Any], label: str) -> list[str]:
    try:
        from jsonschema import Draft202012Validator
    except ImportError as exc:
        raise ContractError(
            "jsonschema is required for RPC bundle admission; "
            "install the pinned CI dependency"
        ) from exc
    try:
        Draft202012Validator.check_schema(schema)
    except Exception as exc:  # jsonschema raises SchemaError subclasses.
        return [f"{label}: invalid JSON Schema 2020-12: {exc}"]
    return []


def _normalized_operation(key: str, raw: Any) -> dict[str, Any]:
    entry = _ROUTE_SYNC.normalize_entry(key, raw)
    obj = raw if isinstance(raw, dict) else {}
    transports = list(entry["transports"])
    operation: dict[str, Any] = {
        "key": key,
        "path": entry["path"],
        "methods": list(entry["methods"]),
        "transports": transports,
        "delivery": obj.get("delivery") or "direct",
    }
    tcp_framing = obj.get("tcp_framing") or (
        "ndjson" if "tcp" in transports else None
    )
    if tcp_framing is not None:
        operation["tcpFraming"] = tcp_framing
    for source, target in (
        ("summary", "summary"),
        ("path_params", "pathParams"),
        ("query_schema", "querySchema"),
        ("request_schema", "requestSchema"),
        ("response_schema", "responseSchema"),
        ("error_schema", "errorSchema"),
        ("alias_of", "aliasOf"),
        ("opto_sync", "optoSync"),
    ):
        if source in obj:
            operation[target] = obj[source]
    binding = obj.get("binding")
    if isinstance(binding, dict):
        normalized_binding = {
            key: value
            for key, value in binding.items()
            if value is not None and value != []
        }
        if normalized_binding:
            operation["binding"] = normalized_binding
    return operation


def build_contract(map_path: Path) -> dict[str, Any]:
    doc = json.loads(map_path.read_text(encoding="utf-8"))
    if not isinstance(doc, dict):
        raise ContractError(f"{map_path}: route map must be an object")

    schema = json.loads(
        (ROOT / "json-schema" / "route-map.schema.json").read_text(encoding="utf-8")
    )
    errors = list(_ROUTE_SYNC.jsonschema_validate(doc, schema, str(map_path)))
    errors.extend(_ROUTE_SYNC.structural_validate(doc, str(map_path)))

    raw_map = doc.get("map")
    if not isinstance(raw_map, dict):
        errors.append(f"{map_path}: map must be an object")
        raw_map = {}

    operations: list[dict[str, Any]] = []
    for key in sorted(raw_map):
        raw = raw_map[key]
        if isinstance(raw, dict):
            for field in SCHEMA_FIELDS:
                value = raw.get(field)
                if value is not None:
                    if not isinstance(value, dict):
                        errors.append(f"{map_path}.{key}.{field}: expected object")
                    else:
                        errors.extend(
                            _meta_schema_errors(value, f"{map_path}.{key}.{field}")
                        )
            path_params = raw.get("path_params")
            if isinstance(path_params, dict):
                required = set(path_params.get("required") or [])
                properties = set((path_params.get("properties") or {}).keys())
                if required != properties:
                    errors.append(
                        f"{map_path}.{key}.path_params: every path variable must be required; "
                        f"required={sorted(required)} properties={sorted(properties)}"
                    )
        operations.append(_normalized_operation(key, raw))

    # Aliases must be acyclic, and transport/default semantics must not be hidden.
    aliases = {
        op["key"]: op["aliasOf"]
        for op in operations
        if isinstance(op.get("aliasOf"), str)
    }
    for start in aliases:
        seen: set[str] = set()
        current = start
        while current in aliases:
            if current in seen:
                errors.append(f"{map_path}.{start}: alias cycle")
                break
            seen.add(current)
            current = aliases[current]

    if errors:
        raise ContractError("\n".join(sorted(set(errors))))

    semantic = {
        "formatVersion": 1,
        "routeMapSchemaVersion": doc["schema_version"],
        "service": doc["service"],
        "title": doc.get("title") or doc["service"],
        "version": doc.get("version") or "0.1.0",
        "description": doc.get("description") or "",
        "operations": operations,
    }
    return {
        **semantic,
        "source": map_path.name,
        "contractSha256": sha256_hex(semantic),
    }
