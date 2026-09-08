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


if __name__ == "__main__":
    raise SystemExit(core.main())
