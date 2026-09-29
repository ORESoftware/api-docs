#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from ridl.emit import go
from ridl.model import parse_route_map
from ridl.validate import validate


class GoIdentifierCollisionTests(unittest.TestCase):
    def test_operation_name_is_disambiguated_from_authored_type(self) -> None:
        case = json.loads((ROOT / "examples/demo.route-map.json").read_text())
        case["types"]["WalkMatter"] = case["types"].pop("WalkBody")
        case["map"]["walk_matter"]["request"] = "WalkMatter"

        route_map = parse_route_map(case)
        self.assertEqual([], validate(route_map))
        generated = "\n".join(item.text for item in go.emit(route_map))

        self.assertIn("type WalkMatter struct", generated)
        self.assertIn("func WalkMatterCall(", generated)
        self.assertNotIn("func WalkMatter(", generated)


if __name__ == "__main__":
    unittest.main()
