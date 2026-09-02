#!/usr/bin/env python3
"""Unit tests for the RPC dual-primary cross-check."""
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "cross_check_rpc_idl",
    Path(__file__).with_name("cross-check-rpc-idl.py"),
)
xc = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules["cross_check_rpc_idl"] = xc
_SPEC.loader.exec_module(xc)


ROOT = Path(__file__).resolve().parents[1]


class ParsePrimaries(unittest.TestCase):
    def test_typespec_rpc_call_fields(self):
        shapes = xc.load_all_typespec(ROOT)
        call = shapes["Ores.Rpc.V1.RpcCall"]
        self.assertEqual(
            set(call.fields),
            {"v", "op", "id", "key", "transport", "path", "query", "body", "traceId", "spanId"},
        )
        self.assertEqual(call.fields["v"].const, 1)
        self.assertEqual(call.fields["op"].const, "call")
        self.assertTrue(call.fields["id"].required)
        self.assertFalse(call.fields["transport"].required)
        self.assertEqual(call.fields["id"].max_length, 128)
        self.assertEqual(call.fields["key"].pattern, "^[A-Za-z][A-Za-z0-9_]*$")

    def test_json_schema_frame_collects_if_then_arms(self):
        shapes = xc.load_json_shapes(ROOT)
        frame = shapes["rpc-frame"]
        self.assertIn("key", frame.fields)
        self.assertIn("code", frame.fields)
        self.assertIn("meta", frame.fields)
        self.assertTrue(frame.fields["v"].required)
        self.assertTrue(frame.fields["key"].required)

    def test_proto_json_name_and_lock(self):
        proto = xc.load_all_proto(ROOT)
        call = proto["ores.rpc.v1.RpcCall"]
        self.assertEqual(call.fields["traceId"].proto_number, 9)
        self.assertEqual(call.fields["spanId"].proto_number, 10)
        lock = json.loads((ROOT / "idl" / "protobuf.lock.json").read_text())
        self.assertEqual(xc.check_protobuf_lock(proto, lock), [])


class DualPrimaryGate(unittest.TestCase):
    def test_current_tree_is_green(self):
        report = xc.run(ROOT)
        self.assertEqual(report["vetoes"], [], msg=report["vetoes"])
        self.assertTrue(report["ok"])

    def test_dropping_a_typespec_field_vetoes(self):
        text = (ROOT / "idl" / "typespec" / "v1.tsp").read_text()
        broken = text.replace(
            "  @minLength(1)\n  @maxLength(32)\n  spanId?: string;\n",
            "",
            1,
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._copy_idl(root)
            (root / "idl" / "typespec" / "v1.tsp").write_text(broken, encoding="utf-8")
            report = xc.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(any("spanId" in item for item in report["vetoes"]))

    def test_reusing_a_proto_field_number_vetoes(self):
        proto = (ROOT / "idl" / "protobuf" / "ores" / "rpc" / "v1" / "rpc.proto").read_text()
        broken = proto.replace("optional string span_id = 10", "optional string span_id = 9")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._copy_idl(root)
            (root / "idl" / "protobuf" / "ores" / "rpc" / "v1" / "rpc.proto").write_text(
                broken, encoding="utf-8"
            )
            report = xc.run(root)
        self.assertFalse(report["ok"])
        self.assertTrue(any("field numbers" in item for item in report["vetoes"]))

    def _copy_idl(self, dest: Path) -> None:
        import shutil

        shutil.copytree(ROOT / "idl", dest / "idl")
        shutil.copytree(ROOT / "json-schema", dest / "json-schema")


if __name__ == "__main__":
    unittest.main()
