#!/usr/bin/env python3
"""Adversarial request-surface tests for v1 route contracts.

These tests intentionally separate *routing identity* (HTTP method + URL path)
from request validation (path params, query params, JSON body, and headers).
Query/header-dependent operation selection is an anti-pattern and must remain
unrepresentable in the route-map contract.
"""
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


route_sync = _load("request_surface_route_sync", ROOT / "scripts" / "check-route-sync.py")
route_gen = _load("request_surface_route_gen", ROOT / "scripts" / "generate-routes.py")
bundle = _load("request_surface_bundle", ROOT / "scripts" / "rpc-contract-bundle.py")


class RequestSurfaceContracts(unittest.TestCase):
    def _schema(self) -> dict:
        return json.loads((ROOT / "json-schema" / "route-map.schema.json").read_text())

    def test_query_cannot_be_used_to_disambiguate_routes(self) -> None:
        instance = {
            "schema_version": "1.0.0",
            "service": "x",
            "map": {
                "list_active": {
                    "path": "/v1/items",
                    "methods": ["GET"],
                    "query_schema": {
                        "type": "object",
                        "properties": {"state": {"const": "active"}},
                    },
                },
                "list_archived": {
                    "path": "/v1/items",
                    "methods": ["GET"],
                    "query_schema": {
                        "type": "object",
                        "properties": {"state": {"const": "archived"}},
                    },
                },
            },
        }
        errors = route_sync.structural_validate(instance, "t")
        self.assertTrue(any("both bind GET /v1/items" in error for error in errors), errors)

    def test_header_or_query_routing_extensions_are_schema_errors(self) -> None:
        for field in ("route_by_header", "route_by_query", "dispatch_headers", "dispatch_query"):
            instance = {
                "schema_version": "1.0.0",
                "service": "x",
                "map": {
                    "get_item": {
                        "path": "/v1/items/{id}",
                        "methods": ["GET"],
                        field: {"X-Tenant": "a"},
                    }
                },
            }
            errors = route_sync.jsonschema_validate(instance, self._schema(), "t")
            self.assertTrue(errors, f"{field} unexpectedly accepted")

    def test_path_query_and_body_types_reach_typescript_handlers(self) -> None:
        mapping = {
            "update_item": {
                "path": "/v1/items/{id}",
                "methods": ["PATCH"],
                "path_params": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {"id": {"type": "string"}},
                },
                "query_schema": {
                    "type": "object",
                    "properties": {
                        "dryRun": {"type": "boolean"},
                        "limit": {"type": "integer"},
                    },
                },
                "request_schema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {"name": {"type": "string"}},
                },
                "response_schema": {
                    "type": "object",
                    "required": ["ok"],
                    "properties": {"ok": {"type": "boolean"}},
                },
            }
        }
        generated = route_gen.gen_typescript("x", mapping)
        self.assertIn('"id": string', generated)
        self.assertIn('"dryRun"?: boolean', generated)
        self.assertIn('"limit"?: number', generated)
        self.assertIn('"name": string', generated)
        self.assertIn('path: RouteTypes[K]["path"]', generated)
        self.assertIn('query: RouteTypes[K]["query"]', generated)
        self.assertIn('body: RouteTypes[K]["body"]', generated)

    def test_path_query_and_body_types_reach_rust_compile_surface(self) -> None:
        mapping = {
            "update_item": {
                "path": "/v1/items/{id}",
                "methods": ["PATCH"],
                "path_params": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {"id": {"type": "string"}},
                },
                "query_schema": {
                    "type": "object",
                    "properties": {"limit": {"type": "integer"}},
                },
                "request_schema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {"name": {"type": "string"}},
                },
            }
        }
        generated = route_gen.gen_rust("x", mapping)
        self.assertIn("pub struct UpdateItemPath", generated)
        self.assertIn("pub id: String", generated)
        self.assertIn("pub struct UpdateItemQuery", generated)
        self.assertIn("pub limit: Option<i64>", generated)
        self.assertIn("pub struct UpdateItemRequest", generated)
        self.assertIn("pub name: String", generated)

    def test_openapi_keeps_path_query_and_json_body_separate(self) -> None:
        document = {
            "schema_version": "1.0.0",
            "service": "x",
            "map": {
                "update_item": {
                    "path": "/v1/items/{id}",
                    "methods": ["PATCH"],
                    "path_params": {
                        "type": "object",
                        "required": ["id"],
                        "properties": {"id": {"type": "string"}},
                    },
                    "query_schema": {
                        "type": "object",
                        "properties": {"dryRun": {"type": "boolean"}},
                    },
                    "request_schema": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {"name": {"type": "string"}},
                    },
                }
            },
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.route-map.json"
            path.write_text(json.dumps(document), encoding="utf-8")
            contract = bundle.build_contract(path)
            openapi = bundle.project_openapi(contract)
        operation = openapi["paths"]["/v1/items/{id}"]["patch"]
        parameters = {(p["name"], p["in"]): p for p in operation["parameters"]}
        self.assertTrue(parameters[("id", "path")]["required"])
        self.assertFalse(parameters[("dryRun", "query")]["required"])
        self.assertEqual(
            operation["requestBody"]["content"]["application/json"]["schema"]["properties"]["name"]["type"],
            "string",
        )

    def test_path_parameter_schema_must_exactly_match_template(self) -> None:
        instance = {
            "schema_version": "1.0.0",
            "service": "x",
            "map": {
                "get_item": {
                    "path": "/v1/items/{id}",
                    "methods": ["GET"],
                    "path_params": {
                        "type": "object",
                        "required": ["wrong"],
                        "properties": {"wrong": {"type": "string"}},
                    },
                }
            },
        }
        errors = route_sync.structural_validate(instance, "t")
        self.assertTrue(any("path_params" in error and "template" in error for error in errors), errors)

    def test_invalid_nested_json_schema_is_build_veto(self) -> None:
        document = {
            "schema_version": "1.0.0",
            "service": "x",
            "map": {
                "get_item": {
                    "path": "/v1/items/{id}",
                    "methods": ["GET"],
                    "path_params": {
                        "type": "object",
                        "required": ["id"],
                        "properties": {"id": {"type": "definitely-invalid"}},
                    },
                }
            },
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.route-map.json"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaises(bundle.ContractError):
                bundle.build_contract(path)

    def test_nats_only_query_is_rejected_before_runtime(self) -> None:
        instance = {
            "schema_version": "1.0.0",
            "service": "x",
            "map": {
                "list_items": {
                    "path": "/v1/items",
                    "methods": ["GET"],
                    "transports": ["nats"],
                    "query_schema": {
                        "type": "object",
                        "properties": {"q": {"type": "string"}},
                    },
                }
            },
        }
        errors = route_sync.structural_validate(instance, "t")
        self.assertTrue(any("NATS" in error for error in errors), errors)


if __name__ == "__main__":
    unittest.main()
