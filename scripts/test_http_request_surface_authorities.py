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
            path = root / "idl/typespec/http-request-surface.tsp"
            path.write_text(path.read_text().replace("  headers?: Record<unknown>;\n", ""))
            self.assertTrue(MODULE.audit(root))

    def test_header_dispatch_extension_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "json-schema/http-request-surface.schema.json"
            schema = json.loads(path.read_text())
            schema["properties"]["routeByHeader"] = {"type": "string"}
            path.write_text(json.dumps(schema))
            self.assertTrue(MODULE.audit(root))


if __name__ == "__main__":
    unittest.main()
