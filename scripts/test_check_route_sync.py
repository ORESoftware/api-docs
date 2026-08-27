#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_route_sync", ROOT / "scripts" / "check-route-sync.py"
)
mod = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(mod)


class RouteSync(unittest.TestCase):
    def test_example_map_matches_schema(self):
        schema = json.loads((ROOT / "json-schema" / "route-map.schema.json").read_text())
        instance = json.loads((ROOT / "examples" / "pmap-api.route-map.json").read_text())
        errs = mod.jsonschema_validate(instance, schema, "example")
        self.assertEqual(errs, [], errs)

    def test_scan_and_compare_single_line_axum(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "main.rs"
            src.write_text(
                """
                .route("/healthz", get(|| async { "ok" }))
                .route("/v1/matters", post(create_matter))
                .merge(docs::router());
                """,
                encoding="utf-8",
            )
            scanned, docs = mod.scan_rust_routes([Path(tmp)])
            self.assertTrue(docs)
            self.assertEqual(scanned["/healthz"], {"GET"})
            self.assertEqual(scanned["/v1/matters"], {"POST"})
            mapping = {
                "schema_version": "1.0.0",
                "service": "x",
                "map": {"healthz": "/healthz", "create_matter": "/v1/matters"},
            }
            self.assertEqual(
                mod.compare(
                    mapping, scanned, allow_docs_merge=True, docs_merged=docs, label="t"
                ),
                [],
            )

    def test_drift_is_reported(self):
        mapping = {
            "schema_version": "1.0.0",
            "service": "x",
            "map": {"healthz": "/healthz"},
        }
        errs = mod.compare(
            mapping,
            {"/v1/secret": {"GET"}},
            allow_docs_merge=False,
            docs_merged=False,
            label="t",
        )
        self.assertTrue(any("not registered" in e for e in errs))
        self.assertTrue(any("not in the map" in e for e in errs))

    def test_pascal_case_is_post(self):
        self.assertEqual(mod.infer_methods("CheckFieldSanity"), ["POST"])
        self.assertEqual(mod.infer_methods("healthz"), ["GET"])


if __name__ == "__main__":
    unittest.main()
