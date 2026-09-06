#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from jsonschema import Draft202012Validator

from ridl.emit import dart, gleam, go, json_schema, kotlin, python, rust, swift, typescript
from ridl.model import parse_route_map
from ridl.validate import validate

ROOT = Path(__file__).resolve().parents[1]


class TypedRequestHeaderTests(unittest.TestCase):
    def route_map(self):
        return parse_route_map(json.loads((ROOT / "examples/demo.route-map.json").read_text()))

    def test_header_contract_is_typed_in_every_emitter(self) -> None:
        rmap = self.route_map()
        self.assertEqual([], validate(rmap))
        emitters = (dart, gleam, go, kotlin, python, rust, swift, typescript)
        for emitter in emitters:
            generated = "\n".join(item.text for item in emitter.emit(rmap))
            self.assertIn("x-client-version", generated, emitter.__name__)
            self.assertIn("headers", generated.lower(), emitter.__name__)

    def test_header_names_and_ownership_are_linted(self) -> None:
        base = json.loads((ROOT / "examples/demo.route-map.json").read_text())
        for name in ("X-Client-Version", "authorization", "content-length"):
            case = json.loads(json.dumps(base))
            case["map"]["get_matter"]["header_params"] = {name: "String"}
            errors = validate(parse_route_map(case))
            self.assertTrue(any("header" in error for error in errors), (name, errors))

    def test_header_contract_is_http_only(self) -> None:
        case = json.loads((ROOT / "examples/demo.route-map.json").read_text())
        case["map"]["get_matter"]["transports"] = ["http", "websocket"]
        errors = validate(parse_route_map(case))
        self.assertTrue(any("headers are HTTP-only" in error for error in errors), errors)

    def test_generated_runtime_schema_checks_every_request_surface(self) -> None:
        rmap = self.route_map()
        generated = {item.path: json.loads(item.text) for item in json_schema.emit(rmap)}
        schema = generated["json-schema/operations/get_matter.request.schema.json"]
        validator = Draft202012Validator(schema)
        valid = {
            "method": "GET",
            "pathTemplate": "/v1/matters/{id}",
            "path": {"id": "4f867eb4-27d4-47b9-83ce-3379c13f24ec"},
            "query": {"include_facts": True},
            "headers": {"x-client-version": "2026.09", "if-none-match": '"etag"'},
        }
        self.assertEqual([], list(validator.iter_errors(valid)))
        for mutation in (
            {**valid, "headers": {}},
            {**valid, "method": "POST"},
            {**valid, "query": {"include_facts": "yes"}},
            {**valid, "headers": {"x-client-version": 3}},
            {**valid, "routeByHeader": "x-client-version"},
        ):
            self.assertTrue(list(validator.iter_errors(mutation)), mutation)
        self.assertEqual(["method", "pathTemplate"], schema["x-ores-routing-identity"])
        self.assertEqual(["path", "query", "headers", "body"], schema["x-ores-validation-only"])


if __name__ == "__main__":
    unittest.main()
