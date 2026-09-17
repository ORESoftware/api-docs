#!/usr/bin/env python3
"""Generate parity-gated RPC v1 SQL, Protobuf, and gRPC artifacts."""
from __future__ import annotations

import copy
import json
import re
from typing import Any

import rpc_v1_projection_core as core

_ORIGINAL_VALIDATE = core.validate
_ORIGINAL_RENDER_PROTO = core.render_proto
_ORIGINAL_FIELD_CHECKS = core.field_checks
_OLD_SERVICE = {
    "name": "RpcGateway",
    "methods": [
        {
            "name": "Call",
            "request": "RpcCall",
            "response": "RpcReceipt",
            "clientStreaming": False,
            "serverStreaming": False,
        }
    ],
}
_EXPECTED_SERVICE = {
    "name": "RpcService",
    "methods": [
        {
            "name": "Call",
            "request": "CallRequest",
            "response": "CallResponse",
            "requestPayload": "RpcCall",
            "responsePayload": "RpcReceipt",
            "clientStreaming": False,
            "serverStreaming": False,
        }
    ],
}
_EXPECTED_WRAPPERS = {
    "CallRequest": {"field": "call", "payload": "RpcCall"},
    "CallResponse": {"field": "receipt", "payload": "RpcReceipt"},
}
_OLD_SERVICE_PROTO = """service RpcGateway {
  rpc Call(RpcCall) returns (RpcReceipt);
}
"""
_LEGACY_KEY_PATTERN = r"^[A-Za-z][A-Za-z0-9_]*$"
_CANONICAL_OR_LEGACY_KEY_PATTERN = (
    r"^(?:[A-Za-z][A-Za-z0-9_]*|[a-z][a-z0-9_-]*(?:\.[a-z][a-z0-9_-]*)+)$"
)
# JSON Schema uses ECMA-262 regex syntax, while PostgreSQL uses POSIX ARE.
# Non-capturing groups are not portable, so this mapping is deliberately exact
# and reviewed rather than performing a generic regex rewrite.
_POSTGRES_CANONICAL_OR_LEGACY_KEY_PATTERN = (
    r"^([A-Za-z][A-Za-z0-9_]*|[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)+)$"
)


def _legacy_config(config: dict[str, Any]) -> dict[str, Any]:
    legacy = copy.deepcopy(config)
    legacy["service"] = copy.deepcopy(_OLD_SERVICE)
    return legacy


def _validate_wrapper_message(
    lock: dict[str, Any],
    name: str,
    specification: dict[str, str],
) -> None:
    field = specification["field"]
    payload = specification["payload"]
    if not core.SQL_NAME_RE.fullmatch(field):
        raise core.ProjectionError(f"unsafe gRPC wrapper field name: {field!r}")
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.]*", payload):
        raise core.ProjectionError(f"unsafe gRPC wrapper payload type: {payload!r}")
    message = lock.get("messages", {}).get(f"ores.rpc.v1.{name}")
    if not isinstance(message, dict) or set(message.get("fields", {})) != {field}:
        raise core.ProjectionError(f"{name} Protobuf field ledger drift")
    number = message["fields"][field]
    if not core.valid_proto_number(number):
        raise core.ProjectionError(f"{name} Protobuf field number invalid")
    reserved = message.get("reserved", [])
    if not isinstance(reserved, list):
        raise core.ProjectionError(f"{name} reserved field ledger must be an array")
    if number in reserved:
        raise core.ProjectionError(f"{name} reuses a reserved field number")


def field_checks(
    column: str, field: dict[str, Any], enums: dict[str, list[str]]
) -> list[str]:
    pattern = field.get("pattern")
    if pattern != _CANONICAL_OR_LEGACY_KEY_PATTERN:
        return _ORIGINAL_FIELD_CHECKS(column, field, enums)

    # Reuse the reviewed legacy path for every non-regex constraint, then swap
    # only the regex check for the POSIX-equivalent canonical/legacy expression.
    translated = copy.deepcopy(field)
    translated["pattern"] = _LEGACY_KEY_PATTERN
    checks = _ORIGINAL_FIELD_CHECKS(column, translated, enums)
    legacy_check = f"{column} ~ {core.sql_literal(_LEGACY_KEY_PATTERN)}"
    postgres_check = (
        f"{column} ~ {core.sql_literal(_POSTGRES_CANONICAL_OR_LEGACY_KEY_PATTERN)}"
    )
    if checks.count(legacy_check) != 1:
        raise core.ProjectionError("reviewed key regex translation lost its SQL check")
    return [postgres_check if check == legacy_check else check for check in checks]


def validate(
    config: dict[str, Any],
    lock: dict[str, Any],
    tsp_text: str,
    schemas: dict[str, dict[str, Any]],
) -> tuple[dict[str, dict[str, dict[str, Any]]], dict[str, list[str]]]:
    if config.get("service") != _EXPECTED_SERVICE:
        raise core.ProjectionError(
            "gRPC service must remain unary RpcService.Call with "
            "CallRequest/CallResponse wrappers"
        )
    if config.get("wrapperMessages") != _EXPECTED_WRAPPERS:
        raise core.ProjectionError("gRPC wrapper message metadata drift")
    for name, specification in _EXPECTED_WRAPPERS.items():
        _validate_wrapper_message(lock, name, specification)
    fields, enums = _ORIGINAL_VALIDATE(
        _legacy_config(config),
        lock,
        tsp_text,
        schemas,
    )
    return fields, enums


def render_proto(
    config: dict[str, Any],
    lock: dict[str, Any],
    fields: dict[str, dict[str, dict[str, Any]]],
    digest: str,
) -> str:
    rendered = _ORIGINAL_RENDER_PROTO(
        _legacy_config(config),
        lock,
        fields,
        digest,
    )
    if not rendered.endswith(_OLD_SERVICE_PROTO):
        raise core.ProjectionError("legacy gRPC service block was not found")
    blocks: list[str] = []
    for wrapper_name in ("CallRequest", "CallResponse"):
        specification = config["wrapperMessages"][wrapper_name]
        field = specification["field"]
        payload = specification["payload"]
        number = lock["messages"][f"ores.rpc.v1.{wrapper_name}"]["fields"][field]
        blocks.extend(
            [
                f"message {wrapper_name} {{",
                f"  {payload} {field} = {number};",
                "}",
                "",
            ]
        )
    method = config["service"]["methods"][0]
    blocks.extend(
        [
            f"service {config['service']['name']} {{",
            f"  rpc {method['name']}({method['request']}) "
            f"returns ({method['response']});",
            "}",
            "",
        ]
    )
    return rendered[: -len(_OLD_SERVICE_PROTO)] + "\n".join(blocks)


def render_manifest(config: dict[str, Any], digest: str) -> str:
    service = config["service"]
    package = config["package"]
    methods = [
        {
            "clientStreaming": method["clientStreaming"],
            "fullName": f"{package}.{service['name']}.{method['name']}",
            "name": method["name"],
            "request": f"{package}.{method['request']}",
            "requestPayload": f"{package}.{method['requestPayload']}",
            "response": f"{package}.{method['response']}",
            "responsePayload": f"{package}.{method['responsePayload']}",
            "serverStreaming": method["serverStreaming"],
        }
        for method in service["methods"]
    ]
    value = {
        "expectedDeltaIds": sorted(core.DELTA_IDS),
        "formatVersion": 1,
        "package": package,
        "projectionSha256": digest,
        "proto": "../../idl/protobuf/ores/rpc/v1/rpc.proto",
        "semanticValidator": "../../json-schema/rpc-receipt.schema.json",
        "service": {
            "fullName": f"{package}.{service['name']}",
            "methods": methods,
            "name": service["name"],
        },
        "sql": "rpc-storage.sql",
        "wireCompatibility": {
            "bufCompliantWrappers": ["CallRequest", "CallResponse"],
            "requestPayload": "RpcCall",
            "responsePayload": "RpcReceipt",
            "stablePayloadMessagesRenamed": False,
        },
    }
    return json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"


core.validate = validate
core.render_proto = render_proto
core.render_manifest = render_manifest
core.field_checks = field_checks


if __name__ == "__main__":
    raise SystemExit(core.main())
