#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "request_surface_authority", ROOT / "scripts/check-http-request-surface-authorities.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RequestSurfaceAuthorityTests(unittest.TestCase):
    def copy(self, root: Path) -> Path:
        shutil.copytree(ROOT / "idl", root / "idl")
        shutil.copytree(ROOT / "json-schema", root / "json-schema")
        return root

    def test_current_peers_agree(self) -> None:
        self.assertEqual([], MODULE.audit(ROOT))

    def test_missing_typespec_headers_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http/request-surface.tsp"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "  headers?: Record<unknown>;\n", ""
                ),
                encoding="utf-8",
            )
            self.assertTrue(MODULE.audit(root))

    def test_typespec_field_kind_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http/request-surface.tsp"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "  headers?: Record<unknown>;",
                    "  headers?: string;",
                ),
                encoding="utf-8",
            )
            errors = MODULE.audit(root)
            self.assertTrue(any("headers type" in error for error in errors), errors)

    def test_typespec_path_constraint_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http/request-surface.tsp"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "  @minLength(1)\n", "  @minLength(2)\n", 1
                ),
                encoding="utf-8",
            )
            errors = MODULE.audit(root)
            self.assertTrue(any("pathTemplate decorators" in error for error in errors), errors)

    def test_json_validation_only_metadata_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "json-schema/http-request-surface.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["x-ores-validation-only"] = ["path", "query", "body"]
            path.write_text(json.dumps(schema), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(any("validation-only" in error for error in errors), errors)

    def test_json_header_name_constraint_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "json-schema/http-request-surface.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["properties"]["headers"]["propertyNames"]["pattern"] = ".*"
            path.write_text(json.dumps(schema), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(any("headers" in error and "shape" in error for error in errors), errors)

    def test_json_body_shape_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "json-schema/http-request-surface.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["properties"]["body"] = {"type": "object"}
            path.write_text(json.dumps(schema), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(any("body" in error and "shape" in error for error in errors), errors)

    def test_header_dispatch_extension_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "json-schema/http-request-surface.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["properties"]["routeByHeader"] = {"type": "string"}
            path.write_text(json.dumps(schema), encoding="utf-8")
            self.assertTrue(MODULE.audit(root))

    def test_missing_reviewed_delta_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/http-request-surface.expected-deltas.json"
            document = json.loads(path.read_text(encoding="utf-8"))
            document["deltas"] = [
                entry
                for entry in document["deltas"]
                if entry.get("id") != "http-request-surface-header-property-names"
            ]
            path.write_text(json.dumps(document), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(any("expected delta" in error for error in errors), errors)


if __name__ == "__main__":
    unittest.main()
