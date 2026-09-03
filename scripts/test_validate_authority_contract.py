#!/usr/bin/env python3
"""Unit tests for the peer-authority governance contract."""
from __future__ import annotations

import copy
import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "validate_authority_contract",
    Path(__file__).with_name("validate-authority-contract.py"),
)
validate = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules["validate_authority_contract"] = validate
_SPEC.loader.exec_module(validate)

ROOT = Path(__file__).resolve().parents[1]


class PeerAuthorityContract(unittest.TestCase):
    def setUp(self) -> None:
        self.document = json.loads(
            (ROOT / "idl" / "authority-contract.json").read_text(encoding="utf-8")
        )

    def test_current_tree_is_green(self):
        self.assertEqual(validate.validate_contract(self.document, ROOT), [])

    def test_authority_order_is_forbidden(self):
        broken = copy.deepcopy(self.document)
        broken["policy"]["authorityOrder"] = ["typespec", "json-schema-openapi"]
        errors = validate.validate_contract(broken, ROOT)
        self.assertTrue(any("authorityOrder" in error for error in errors))

    def test_missing_sql_output_is_a_veto(self):
        broken = copy.deepcopy(self.document)
        broken["authorities"][1]["requiredOutputs"].remove("sql")
        errors = validate.validate_contract(broken, ROOT)
        self.assertTrue(any("requiredOutputs" in error for error in errors))

    def test_hierarchy_marker_is_a_veto(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in validate.GOVERNANCE_FILES:
                source = ROOT / relative
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            for relative in ("idl/typespec", "json-schema", "examples"):
                target = root / relative
                target.mkdir(parents=True, exist_ok=True)
            agents = root / "AGENTS.md"
            agents.write_text(
                agents.read_text(encoding="utf-8") + "\nTypeSpec is P0.\n",
                encoding="utf-8",
            )
            errors = validate.validate_contract(self.document, root)
        self.assertTrue(any("obsolete hierarchy marker" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
