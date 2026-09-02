"""OpenAPI, OpenRPC, Connect, and Hyper-Schema projections."""
from __future__ import annotations

import re
from typing import Any

from .model import ContractError


def _rpc_extension(operation: dict[str, Any], digest: str) -> dict[str, Any]:
    out: dict[str, Any] = {
        "contractSha256": digest,
        "key": operation["key"],
        "transports": operation["transports"],
        "delivery": operation["delivery"],
    }
    for field in ("tcpFraming", "aliasOf", "optoSync"):
        if operation.get(field) is not None:
            out[field] = operation[field]
    return out


def project_openapi(contract: dict[str, Any]) -> dict[str, Any]:
    paths: dict[str, dict[str, Any]] = {}
    digest = contract["contractSha256"]
    for op in contract["operations"]:
        item = paths.setdefault(op["path"], {})
        for method in op["methods"]:
            operation: dict[str, Any] = {
                "operationId": op["key"],
                "responses": {"200": {"description": "ok"}},
                "x-ores-rpc": _rpc_extension(op, digest),
            }
            if op.get("summary"):
                operation["summary"] = op["summary"]
            parameters: list[dict[str, Any]] = []
            for location, schema_field in (
                ("path", "pathParams"),
                ("query", "querySchema"),
            ):
                schema = op.get(schema_field)
                if not isinstance(schema, dict):
                    continue
                required = set(schema.get("required") or [])
                for name, property_schema in (schema.get("properties") or {}).items():
                    parameters.append(
                        {
                            "name": name,
                            "in": location,
                            "required": location == "path" or name in required,
                            "schema": property_schema,
                        }
                    )
            if parameters:
                operation["parameters"] = parameters
            if "requestSchema" in op:
                operation["requestBody"] = {
                    "required": True,
                    "content": {
                        "application/json": {"schema": op["requestSchema"]}
                    },
                }
            if "responseSchema" in op:
                operation["responses"]["200"]["content"] = {
                    "application/json": {"schema": op["responseSchema"]}
                }
            item[method.lower()] = operation
    return {
        "openapi": "3.1.0",
        "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
        "info": {
            "title": contract["title"],
            "version": contract["version"],
            "description": contract["description"],
        },
        "paths": paths,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": contract["routeMapSchemaVersion"],
    }


def project_openrpc(contract: dict[str, Any]) -> dict[str, Any]:
    digest = contract["contractSha256"]
    methods: list[dict[str, Any]] = []
    for op in contract["operations"]:
        method: dict[str, Any] = {
            "name": op["key"],
            "paramStructure": "by-name",
            "x-http-path": op["path"],
            "x-http-methods": op["methods"],
            "x-ores-rpc": _rpc_extension(op, digest),
        }
        if op.get("summary"):
            method["summary"] = op["summary"]
        params: list[dict[str, Any]] = []
        for schema_field in ("pathParams", "querySchema"):
            schema = op.get(schema_field)
            if not isinstance(schema, dict):
                continue
            required = set(schema.get("required") or [])
            for name, property_schema in (schema.get("properties") or {}).items():
                params.append(
                    {
                        "name": name,
                        "required": (
                            schema_field == "pathParams" or name in required
                        ),
                        "schema": property_schema,
                    }
                )
        if "requestSchema" in op:
            params.append(
                {
                    "name": "body",
                    "required": True,
                    "schema": op["requestSchema"],
                }
            )
        if params:
            method["params"] = params
        if "responseSchema" in op:
            method["result"] = {
                "name": "result",
                "schema": op["responseSchema"],
            }
        methods.append(method)
    return {
        "openrpc": "1.3.2",
        "info": {
            "title": contract["title"],
            "version": contract["version"],
        },
        "methods": methods,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": contract["routeMapSchemaVersion"],
    }


def project_connect(contract: dict[str, Any]) -> dict[str, Any]:
    digest = contract["contractSha256"]
    services: dict[str, dict[str, Any]] = {}
    for op in contract["operations"]:
        key = op["key"]
        if not re.fullmatch(r"[A-Z][A-Za-z0-9]*", key):
            continue
        parts = [part for part in op["path"].split("/") if part]
        if len(parts) != 2:
            raise ContractError(
                f"{contract['source']}.{key}: Connect path must be /service/Method"
            )
        service, method_name = parts
        if method_name != key:
            raise ContractError(
                f"{contract['source']}.{key}: Connect path method {method_name!r} "
                "must equal the route-map key"
            )
        method: dict[str, Any] = {
            "path": op["path"],
            "httpMethod": "POST",
            "idempotency": "unknown",
            "x-ores-rpc": _rpc_extension(op, digest),
        }
        if "requestSchema" in op:
            method["request"] = op["requestSchema"]
        if "responseSchema" in op:
            method["response"] = op["responseSchema"]
        services.setdefault(service, {"methods": {}})["methods"][method_name] = method
    return {
        "protocol": "connect",
        "codec": "json",
        "contentType": "application/json",
        "streaming": False,
        "services": services,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": contract["routeMapSchemaVersion"],
    }


def project_hyper_schema(contract: dict[str, Any]) -> dict[str, Any]:
    digest = contract["contractSha256"]
    links: list[dict[str, Any]] = []
    for op in contract["operations"]:
        for method in op["methods"]:
            link: dict[str, Any] = {
                "rel": op["key"],
                "href": op["path"],
                "method": method,
                "x-ores-rpc": _rpc_extension(op, digest),
            }
            if "requestSchema" in op:
                link["submissionSchema"] = op["requestSchema"]
            if "responseSchema" in op:
                link["targetSchema"] = op["responseSchema"]
            if "pathParams" in op:
                link["hrefSchema"] = op["pathParams"]
            links.append(link)
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "links": links,
        "x-ores-rpc-contract-sha256": digest,
        "x-ores-rpc-schema-version": contract["routeMapSchemaVersion"],
    }
