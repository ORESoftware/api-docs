#!/usr/bin/env python3
from __future__ import annotations

import copy
import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check-authority-graph.py"
GRAPH = ROOT / "idl" / "authority-graph.json"

spec = importlib.util.spec_from_file_location("authority_graph_check", SCRIPT)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class AuthorityGraphTests(unittest.TestCase):
    def setUp(self) -> None:
        self.graph = json.loads(GRAPH.read_text(encoding="utf-8"))

    def assertRejected(self, graph: dict, contains: str) -> None:
        errors = module.validate(graph)
        self.assertTrue(
            any(contains in error for error in errors),
            f"expected error containing {contains!r}, got {errors!r}",
        )

    def test_canonical_graph_is_accepted(self) -> None:
        self.assertEqual(module.validate(self.graph), [])

    def test_both_authorities_must_remain_top_level(self) -> None:
        graph = copy.deepcopy(self.graph)
        graph["authorities"][1]["top_level"] = False
        self.assertRejected(graph, "top_level=true")

    def test_sql_parity_comparison_cannot_be_removed(self) -> None:
        graph = copy.deepcopy(self.graph)
        graph["comparisons"] = [
            comparison
            for comparison in graph["comparisons"]
            if set((comparison["left"], comparison["right"]))
            != {"typespec-sql", "jsonschema-sql"}
        ]
        self.assertRejected(graph, "missing required comparison")

    def test_discrepancies_cannot_continue_automatically(self) -> None:
        graph = copy.deepcopy(self.graph)
        graph["comparisons"][0]["on_discrepancy"] = "continue-with-typespec"
        self.assertRejected(graph, "on_discrepancy must be pause-and-evaluate")

    def test_downstream_client_cannot_become_an_authority(self) -> None:
        graph = copy.deepcopy(self.graph)
        graph["edges"].append(
            {
                "from": "jsonschema-write-clients",
                "to": "typespec",
                "kind": "projection",
            }
        )
        self.assertRejected(graph, "may not feed an authority")

    def test_diesel_and_seaorm_must_both_project_from_admitted_sql(self) -> None:
        graph = copy.deepcopy(self.graph)
        graph["edges"] = [
            edge
            for edge in graph["edges"]
            if not (
                edge["from"] == "normalized-sql"
                and edge["to"] == "seaorm-models"
            )
        ]
        self.assertRejected(graph, "normalized-sql -> seaorm-models")

    def test_generative_cycles_are_rejected(self) -> None:
        graph = copy.deepcopy(self.graph)
        graph["edges"].append(
            {
                "from": "diesel-models",
                "to": "normalized-sql",
                "kind": "projection",
            }
        )
        self.assertRejected(graph, "generative graph contains a cycle")


if __name__ == "__main__":
    unittest.main()
