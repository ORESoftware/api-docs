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
    "request_surface_authority",
    ROOT / "scripts/check-http-request-surface-authorities.py",
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RequestSurfaceAuthorityTests(unittest.TestCase):
    def copy(self, root: Path) -> Path:
        shutil.copytree(ROOT / "idl", root / "idl")
        shutil.copytree(ROOT / "json-schema", root / "json-schema")
        return root

    @staticmethod
    def request_schema(root: Path) -> tuple[Path, dict]:
        path = root / "json-schema/http-request-surface.schema.json"
        document = json.loads(path.read_text(encoding="utf-8"))
        return path, document

    def test_current_peers_agree_without_waivers(self) -> None:
        self.assertEqual([], MODULE.audit(ROOT))

    def test_missing_typespec_headers_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http/request-surface.tsp"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    '  @extension("propertyNames", '
                    '#{ pattern: "^[!#$%&\'*+.^_`|~0-9a-z-]+$" })\n'
                    "  headers?: Record<unknown>;\n",
                    "",
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
                    "  @minLength(1)\n",
                    "  @minLength(2)\n",
                    1,
                ),
                encoding="utf-8",
            )
            errors = MODULE.audit(root)
            self.assertTrue(
                any("pathTemplate decorators" in error for error in errors),
                errors,
            )

    def test_missing_typespec_header_key_constraint_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http/request-surface.tsp"
            source = path.read_text(encoding="utf-8")
            path.write_text(
                source.replace(
                    '  @extension("propertyNames", '
                    '#{ pattern: "^[!#$%&\'*+.^_`|~0-9a-z-]+$" })\n',
                    "",
                ),
                encoding="utf-8",
            )
            errors = MODULE.audit(root)
            self.assertTrue(
                any("headers decorators" in error for error in errors),
                errors,
            )

    def test_typespec_routing_metadata_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http/request-surface.tsp"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    '#["method", "pathTemplate"]',
                    '#["method", "pathTemplate", "query"]',
                ),
                encoding="utf-8",
            )
            errors = MODULE.audit(root)
            self.assertTrue(
                any("RequestSurface decorators" in error for error in errors),
                errors,
            )

    def test_json_validation_only_metadata_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path, document = self.request_schema(root)
            document["$defs"]["RequestSurface"]["x-ores-validation-only"] = [
                "path",
                "query",
                "body",
            ]
            path.write_text(json.dumps(document), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(
                any("validation-only" in error for error in errors),
                errors,
            )

    def test_json_header_name_constraint_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path, document = self.request_schema(root)
            document["$defs"]["RequestSurface"]["properties"]["headers"][
                "propertyNames"
            ]["pattern"] = ".*"
            path.write_text(json.dumps(document), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(
                any("headers" in error and "shape" in error for error in errors),
                errors,
            )

    def test_json_body_shape_drift_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path, document = self.request_schema(root)
            document["$defs"]["RequestSurface"]["properties"]["body"] = {
                "type": "object"
            }
            path.write_text(json.dumps(document), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(
                any("body" in error and "shape" in error for error in errors),
                errors,
            )

    def test_header_dispatch_extension_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path, document = self.request_schema(root)
            document["$defs"]["RequestSurface"]["properties"][
                "routeByHeader"
            ] = {"type": "string"}
            path.write_text(json.dumps(document), encoding="utf-8")
            self.assertTrue(MODULE.audit(root))

    def test_any_expected_delta_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/http-request-surface.expected-deltas.json"
            document = json.loads(path.read_text(encoding="utf-8"))
            document["deltas"] = [
                {
                    "id": "unexpected-waiver",
                    "kind": "constraint_absent",
                    "reason": "No HTTP request-surface waivers are permitted.",
                }
            ]
            path.write_text(json.dumps(document), encoding="utf-8")
            errors = MODULE.audit(root)
            self.assertTrue(
                any("zero active expected deltas" in error for error in errors),
                errors,
            )


if __name__ == "__main__":
    unittest.main()
