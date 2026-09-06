#!/usr/bin/env python3
"""Tests for strict RPC IDL admission."""
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
    "audit_rpc_idl",
    ROOT / "scripts" / "audit-rpc-idl.py",
)
audit = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules["audit_rpc_idl"] = audit
_SPEC.loader.exec_module(audit)


class StrictRpcIdlAdmission(unittest.TestCase):
    def test_current_tree_is_green(self):
        report = audit.run(ROOT)
        self.assertEqual(report["vetoes"], [], report["vetoes"])
        self.assertTrue(report["ok"])

    def test_missing_typespec_constraint_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "typespec" / "v1.tsp"
            text = path.read_text(encoding="utf-8")
            text = text.replace(
                "  @maxLength(128)\n  id: string;",
                "  id: string;",
                1,
            )
            path.write_text(text, encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("RpcCall.id.max_length" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_unreviewed_expected_delta_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "expected-deltas.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            doc["deltas"].append(
                {
                    "id": "silent-new-exception",
                    "kind": "constraint_absent",
                    "reason": "This should never be accepted without changing the exact allow-list.",
                }
            )
            path.write_text(json.dumps(doc), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("exact allow-list" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_duplicate_proto_number_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "protobuf" / "ores" / "rpc" / "v1" / "rpc.proto"
            text = path.read_text(encoding="utf-8")
            text = text.replace(
                "optional string span_id = 10",
                "optional string span_id = 9",
                1,
            )
            path.write_text(text, encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("duplicate field numbers" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_unreviewed_typespec_reference_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "typespec" / "v1.tsp"
            text = path.read_text(encoding="utf-8").replace(
                "transport?: Transport;",
                "transport?: UnreviewedTransport;",
                1,
            )
            path.write_text(text, encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("two optional Transport fields" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_unparsed_proto_field_shape_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "protobuf" / "ores" / "rpc" / "v2" / "frame.proto"
            text = path.read_text(encoding="utf-8").replace(
                "optional bytes body = 8;",
                "map<string, string> body = 8;",
                1,
            )
            path.write_text(text, encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("protobuf parser coverage" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_proto_enum_ledger_drift_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "protobuf.lock.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            doc["enums"]["ores.rpc.v1.Transport"]["TRANSPORT_HTTP"] = 9
            path.write_text(json.dumps(doc), encoding="utf-8")
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("ledger enums" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def test_unreviewed_typespec_declaration_is_a_veto(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._copy_tree(Path(tmp))
            path = root / "idl" / "typespec" / "v1.tsp"
            path.write_text(
                path.read_text(encoding="utf-8")
                + "\nmodel UnreviewedEnvelope {\n  payload: unknown;\n}\n",
                encoding="utf-8",
            )
            report = audit.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(
            any("declaration set" in item for item in report["vetoes"]),
            report["vetoes"],
        )

    def _copy_tree(self, tmp: Path) -> Path:
        root = tmp / "repo"
        (root / "scripts").mkdir(parents=True)
        shutil.copy(
            ROOT / "scripts" / "cross-check-rpc-idl.py",
            root / "scripts" / "cross-check-rpc-idl.py",
        )
        shutil.copytree(ROOT / "idl", root / "idl")
        shutil.copytree(ROOT / "json-schema", root / "json-schema")
        return root


if __name__ == "__main__":
    unittest.main()
