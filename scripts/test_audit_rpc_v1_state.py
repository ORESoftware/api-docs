#!/usr/bin/env python3
"""Adversarial tests for the RPC v1 receipt-state authority gate."""
from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_SPEC = importlib.util.spec_from_file_location(
    "audit_rpc_v1_state",
    ROOT / "scripts" / "audit-rpc-v1-state.py",
)
audit = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules["audit_rpc_v1_state"] = audit
_SPEC.loader.exec_module(audit)


class RpcV1ReceiptStateAudit(unittest.TestCase):
    def test_current_tree_is_green(self):
        report = audit.run(ROOT)
        self.assertTrue(report["ok"], report["vetoes"])
        self.assertEqual(report["vetoes"], [])

    def test_typespec_union_drift_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "typespec" / "v1.tsp"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "RpcSuccessReceipt | RpcErrorReceipt",
                    "RpcSuccessReceipt | RpcSuccessReceipt",
                    1,
                ),
                encoding="utf-8",
            )
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("exact success/error union" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_json_schema_success_status_drift_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "json-schema" / "rpc-receipt.schema.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            doc["allOf"][0]["then"]["properties"]["status"]["maximum"] = 599
            path.write_text(json.dumps(doc), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertIn("rpc-receipt success branch drift", report["vetoes"])

    def test_protobuf_field_reuse_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "protobuf" / "ores" / "rpc" / "v1" / "rpc.proto"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "optional bytes error = 9;",
                    "optional bytes error = 8;",
                    1,
                ),
                encoding="utf-8",
            )
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("Protobuf RpcReceipt fields" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_unreviewed_delta_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "rpc-v1-receipt-state-deltas.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            doc["deltas"].append(
                {
                    "id": "silent-extra",
                    "kind": "shape_projection",
                    "reason": "A deliberately long unreviewed exception that must still fail the exact ledger check.",
                }
            )
            path.write_text(json.dumps(doc), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertIn(
            "receipt-state delta ledger must be the exact reviewed set",
            report["vetoes"],
        )

    def test_required_negative_corpus_case_cannot_disappear(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "examples" / "rpc-v1" / "conformance.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            doc["invalid"] = [
                item
                for item in doc["invalid"]
                if item.get("name") != "failure-without-error"
            ]
            path.write_text(json.dumps(doc), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("missing negative cases" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def _copy_tree(self, tmp: Path) -> Path:
        root = tmp / "repo"
        shutil.copytree(ROOT / "idl", root / "idl")
        shutil.copytree(ROOT / "json-schema", root / "json-schema")
        (root / "examples").mkdir(parents=True)
        shutil.copytree(ROOT / "examples" / "rpc-v1", root / "examples" / "rpc-v1")
        (root / "runtime").mkdir(parents=True)
        shutil.copy(
            ROOT / "runtime" / "v1-conformance.json",
            root / "runtime" / "v1-conformance.json",
        )
        return root


if __name__ == "__main__":
    unittest.main()
