#!/usr/bin/env python3
"""Mutation tests for the fail-closed HTTP routing-policy audit."""
from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "audit_http_routing_policy", ROOT / "scripts" / "audit-http-routing-policy.py"
)
audit = importlib.util.module_from_spec(SPEC)
assert SPEC is not None and SPEC.loader is not None
sys.modules["audit_http_routing_policy"] = audit
SPEC.loader.exec_module(audit)


class HttpRoutingPolicyAudit(unittest.TestCase):
    def test_current_tree_is_green(self) -> None:
        report = audit.run(ROOT)
        self.assertTrue(report["ok"], report["vetoes"])
        self.assertEqual(report["routingIdentity"], ["http_method", "url_path_template"])
        self.assertIn("headers", report["requestValidationOnly"])

    def test_schema_cannot_be_opened_for_new_dispatch_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_minimum(Path(tmp))
            path = root / "json-schema" / "route-map.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["$defs"]["routeObject"]["additionalProperties"] = True
            path.write_text(json.dumps(schema), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(any("additionalProperties=false" in item for item in report["vetoes"]))

    def test_explicit_header_dispatch_field_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_minimum(Path(tmp))
            path = root / "json-schema" / "route-map.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["$defs"]["routeObject"]["properties"]["route_by_header"] = {
                "type": "object"
            }
            path.write_text(json.dumps(schema), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(any("forbidden header/query dispatch" in item for item in report["vetoes"]))

    def test_query_values_cannot_disambiguate_duplicate_method_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_minimum(Path(tmp))
            example = {
                "schema_version": "1.0.0",
                "service": "x",
                "map": {
                    "active": {
                        "path": "/v1/items",
                        "methods": ["GET"],
                        "query_schema": {
                            "type": "object",
                            "properties": {"state": {"const": "active"}},
                        },
                    },
                    "archived": {
                        "path": "/v1/items",
                        "methods": ["GET"],
                        "query_schema": {
                            "type": "object",
                            "properties": {"state": {"const": "archived"}},
                        },
                    },
                },
            }
            (root / "examples" / "duplicate.route-map.json").write_text(
                json.dumps(example), encoding="utf-8"
            )
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(any("may not disambiguate operations" in item for item in report["vetoes"]))

    def test_annotation_cannot_smuggle_query_or_header_routing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_minimum(Path(tmp))
            example = {
                "schema_version": "1.0.0",
                "service": "x",
                "map": {
                    "get_item": {
                        "path": "/v1/items/{id}",
                        "methods": ["GET"],
                        "binding": {
                            "annotation": "@route_by_header('X-Tenant', 'a')"
                        },
                    }
                },
            }
            (root / "examples" / "annotation.route-map.json").write_text(
                json.dumps(example), encoding="utf-8"
            )
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(any("binding annotation encodes" in item for item in report["vetoes"]))

    def _copy_minimum(self, tmp: Path) -> Path:
        root = tmp / "repo"
        (root / "json-schema").mkdir(parents=True)
        (root / "examples").mkdir(parents=True)
        shutil.copy(
            ROOT / "json-schema" / "route-map.schema.json",
            root / "json-schema" / "route-map.schema.json",
        )
        # Keep one known-valid v1 map so the audit still exercises real admission.
        shutil.copy(
            ROOT / "examples" / "canonical-api.route-map.json",
            root / "examples" / "canonical-api.route-map.json",
        )
        return root


if __name__ == "__main__":
    unittest.main()
