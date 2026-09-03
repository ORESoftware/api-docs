#!/usr/bin/env python3
"""Unit tests for fail-closed generated authority artifact comparison."""
from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "compare_authority_artifacts",
    Path(__file__).with_name("compare-authority-artifacts.py"),
)
compare = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules["compare_authority_artifacts"] = compare
_SPEC.loader.exec_module(compare)


def manifest(authority: str) -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "authority": authority,
        "artifacts": {
            "sql": {
                "accounts": {
                    "columns": [
                        {"name": "id", "type": "uuid", "nullable": False},
                        {"name": "email", "type": "text", "nullable": False},
                    ],
                    "primaryKey": ["id"],
                }
            },
            "clientTypes": {
                "Account": {
                    "id": "string",
                    "email": "string",
                }
            },
        },
    }


class AuthorityArtifactComparison(unittest.TestCase):
    def test_peer_authorities_continue_only_on_exact_equivalence(self):
        report = compare.compare_manifests(
            manifest("typespec"),
            manifest("json-schema-openapi"),
            left_label="typespec",
            right_label="json-schema-openapi",
        )
        self.assertTrue(report["ok"])
        self.assertEqual(report["decision"], "continue")
        self.assertEqual(report["differences"], [])

    def test_sql_discrepancy_halts(self):
        left = manifest("typespec")
        right = manifest("json-schema-openapi")
        right["artifacts"]["sql"]["accounts"]["columns"][1]["nullable"] = True
        report = compare.compare_manifests(
            left,
            right,
            left_label="typespec",
            right_label="json-schema-openapi",
        )
        self.assertFalse(report["ok"])
        self.assertEqual(report["decision"], "halt_and_evaluate")
        self.assertTrue(
            any(
                difference["path"].endswith("/columns/1/nullable")
                for difference in report["differences"]
            )
        )

    def test_diesel_seaorm_relation_discrepancy_halts(self):
        diesel = manifest("diesel")
        seaorm = manifest("seaorm")
        diesel["artifacts"]["relations"] = {"account_sessions": "many"}
        seaorm["artifacts"]["relations"] = {"account_sessions": "one"}
        report = compare.compare_manifests(
            diesel,
            seaorm,
            left_label="diesel",
            right_label="seaorm",
        )
        self.assertFalse(report["ok"])
        self.assertEqual(report["decision"], "halt_and_evaluate")
        self.assertTrue(any("relations" in item["path"] for item in report["differences"]))

    def test_missing_required_artifact_halts(self):
        left = manifest("typespec")
        right = manifest("json-schema-openapi")
        del right["artifacts"]["sql"]
        report = compare.compare_manifests(
            left,
            right,
            left_label="typespec",
            right_label="json-schema-openapi",
        )
        self.assertFalse(report["ok"])
        self.assertEqual(report["decision"], "halt_and_evaluate")
        self.assertTrue(any("must include" in error for error in report["errors"]))


if __name__ == "__main__":
    unittest.main()
