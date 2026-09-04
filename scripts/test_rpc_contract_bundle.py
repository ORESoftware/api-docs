#!/usr/bin/env python3
"""Tests for digest-bound RPC docs/language bundle generation."""
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

_SPEC = importlib.util.spec_from_file_location(
    "rpc_contract_bundle",
    ROOT / "scripts" / "rpc-contract-bundle.py",
)
bundle = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules["rpc_contract_bundle"] = bundle
_SPEC.loader.exec_module(bundle)


class RpcContractBundle(unittest.TestCase):
    def test_all_v1_maps_generate_and_verify(self):
        maps = bundle.default_maps()
        self.assertGreaterEqual(len(maps), 8)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            for path in maps:
                target = bundle.generate_one(path, out)
                bundle.verify_bundle(target)
            bundle.verify_ridl_emitters()

    def test_docs_and_all_language_surfaces_share_one_digest(self):
        path = ROOT / "examples" / "rpc-transports.route-map.json"
        with tempfile.TemporaryDirectory() as tmp:
            target = bundle.generate_one(path, Path(tmp))
            contract = json.loads((target / "contract.json").read_text())
            digest = contract["contractSha256"]
            bundle.verify_bundle(target)
            for name in ("openapi", "openrpc", "connect", "hyper-schema"):
                doc = json.loads((target / "docs" / f"{name}.json").read_text())
                self.assertEqual(doc["x-ores-rpc-contract-sha256"], digest)
            for relative in (
                "typescript/routes.ts",
                "rust/routes.rs",
                "dart/routes.dart",
                "gleam/routes.gleam",
                "go/routes.go",
            ):
                self.assertIn(digest, (target / relative).read_text())

    def test_formatting_and_object_key_order_do_not_change_digest(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        original = json.loads(source.read_text())
        with tempfile.TemporaryDirectory() as tmp:
            tmp_root = Path(tmp)
            first = tmp_root / "a.route-map.json"
            second = tmp_root / "b.route-map.json"
            first.write_text(json.dumps(original, indent=2), encoding="utf-8")
            second.write_text(
                json.dumps(original, separators=(",", ":"), sort_keys=True),
                encoding="utf-8",
            )
            self.assertEqual(
                bundle.build_contract(first)["contractSha256"],
                bundle.build_contract(second)["contractSha256"],
            )

    def test_semantic_change_changes_digest(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        original = json.loads(source.read_text())
        changed = json.loads(source.read_text())
        changed["map"]["get_item"]["methods"] = ["POST"]
        with tempfile.TemporaryDirectory() as tmp:
            tmp_root = Path(tmp)
            first = tmp_root / "a.route-map.json"
            second = tmp_root / "b.route-map.json"
            first.write_text(json.dumps(original), encoding="utf-8")
            second.write_text(json.dumps(changed), encoding="utf-8")
            self.assertNotEqual(
                bundle.build_contract(first)["contractSha256"],
                bundle.build_contract(second)["contractSha256"],
            )

    def test_invalid_nested_json_schema_is_a_veto(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        broken = json.loads(source.read_text())
        broken["map"]["get_item"]["response_schema"] = {
            "type": "definitely-not-a-json-schema-type"
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.route-map.json"
            path.write_text(json.dumps(broken), encoding="utf-8")
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.build_contract(path)
        self.assertIn("invalid JSON Schema 2020-12", str(ctx.exception))

    def test_headers_are_digest_bound_and_projected_everywhere(self):
        path = ROOT / "examples" / "rpc-transports.route-map.json"
        contract = bundle.build_contract(path)
        operation = next(op for op in contract["operations"] if op["key"] == "get_item")
        self.assertIn("headerSchema", operation)

        openapi = bundle.project_openapi(contract)
        parameters = openapi["paths"]["/v1/items/{id}"]["get"]["parameters"]
        header = next(p for p in parameters if p["in"] == "header")
        self.assertEqual(header["name"], "x-request-id")
        self.assertTrue(header["required"])

        openrpc = bundle.project_openrpc(contract)
        method = next(m for m in openrpc["methods"] if m["name"] == "get_item")
        self.assertTrue(any(p["x-ores-location"] == "header" for p in method["params"]))

        hyper = bundle.project_hyper_schema(contract)
        link = next(link for link in hyper["links"] if link["rel"] == "get_item")
        self.assertIn("headerSchema", link)

    def test_path_variables_must_be_required(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        broken = json.loads(source.read_text())
        broken["map"]["get_item"]["path_params"]["required"] = []
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.route-map.json"
            path.write_text(json.dumps(broken), encoding="utf-8")
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.build_contract(path)
        self.assertIn("every path variable must be required", str(ctx.exception))

    def test_alias_cycles_are_rejected(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        broken = json.loads(source.read_text())
        broken["map"]["a"] = {"path": "/a", "alias_of": "b"}
        broken["map"]["b"] = {"path": "/b", "alias_of": "a"}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.route-map.json"
            path.write_text(json.dumps(broken), encoding="utf-8")
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.build_contract(path)
        self.assertIn("alias cycle", str(ctx.exception))

    def test_connect_method_name_must_match_route_key(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        broken = json.loads(source.read_text())
        broken["map"]["CreateThing"] = {
            "path": "/example.v1.Service/WrongThing",
            "methods": ["POST"],
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.route-map.json"
            path.write_text(json.dumps(broken), encoding="utf-8")
            contract = bundle.build_contract(path)
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.project_connect(contract)
        self.assertIn("must equal the route-map key", str(ctx.exception))

    def test_hyper_schema_emits_every_declared_method(self):
        source = ROOT / "examples" / "rpc-transports.route-map.json"
        changed = json.loads(source.read_text())
        changed["map"]["healthz"] = {
            "path": "/healthz",
            "methods": ["GET", "HEAD"],
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "multi.route-map.json"
            path.write_text(json.dumps(changed), encoding="utf-8")
            contract = bundle.build_contract(path)
            links = bundle.project_hyper_schema(contract)["links"]
        health_methods = {
            link["method"] for link in links if link["rel"] == "healthz"
        }
        self.assertEqual(health_methods, {"GET", "HEAD"})

    def test_language_surfaces_include_full_mechanism_manifest(self):
        path = ROOT / "examples" / "rpc-transports.route-map.json"
        with tempfile.TemporaryDirectory() as tmp:
            target = bundle.generate_one(path, Path(tmp))
            for relative in (
                "typescript/routes.ts",
                "rust/routes.rs",
                "dart/routes.dart",
                "gleam/routes.gleam",
                "go/routes.go",
            ):
                text = (target / relative).read_text()
                for token in ("get_item", "ndjson", "direct", "websocket", "nats"):
                    self.assertIn(token, text, relative)
            self.assertIn(
                "RPCMechanismsJSON",
                (target / "go" / "routes.go").read_text(),
            )

    def test_document_mechanism_drift_is_a_veto_even_with_matching_digest(self):
        path = ROOT / "examples" / "rpc-transports.route-map.json"
        with tempfile.TemporaryDirectory() as tmp:
            target = bundle.generate_one(path, Path(tmp))
            openapi_path = target / "docs" / "openapi.json"
            document = json.loads(openapi_path.read_text())
            changed = False
            for path_item in document["paths"].values():
                for operation in path_item.values():
                    if isinstance(operation, dict) and "x-ores-rpc" in operation:
                        extension = operation["x-ores-rpc"]
                        extension["key"] = f"{extension['key']}_drift"
                        changed = True
                        break
                if changed:
                    break
            self.assertTrue(changed)
            openapi_path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.verify_bundle(target)
        self.assertIn("openapi RPC mechanisms differ", str(ctx.exception))

    def test_language_mechanism_drift_is_a_veto_not_a_string_search(self):
        path = ROOT / "examples" / "rpc-transports.route-map.json"
        with tempfile.TemporaryDirectory() as tmp:
            target = bundle.generate_one(path, Path(tmp))
            typescript_path = target / "typescript" / "routes.ts"
            text = typescript_path.read_text()
            changed = text.replace(
                '"delivery": "direct"',
                '"delivery": "opto_sync_queued"',
                1,
            )
            self.assertNotEqual(text, changed)
            typescript_path.write_text(changed, encoding="utf-8")
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.verify_bundle(target)
        self.assertIn("typescript RPC mechanisms differ", str(ctx.exception))

    def test_missing_machine_readable_language_manifest_is_a_veto(self):
        path = ROOT / "examples" / "rpc-transports.route-map.json"
        with tempfile.TemporaryDirectory() as tmp:
            target = bundle.generate_one(path, Path(tmp))
            go_path = target / "go" / "routes.go"
            lines = [
                line
                for line in go_path.read_text().splitlines()
                if not line.startswith("const RPCMechanismsJSON = ")
            ]
            go_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
            with self.assertRaises(bundle.ContractError) as ctx:
                bundle.verify_bundle(target)
        self.assertIn("go missing machine-readable", str(ctx.exception))

    def test_go_identifiers_are_unique(self):
        contract = {
            "service": "x",
            "contractSha256": "a" * 64,
            "operations": [
                {
                    "key": "foo_bar",
                    "path": "/a",
                    "methods": ["GET"],
                    "transports": ["http"],
                    "delivery": "direct",
                },
                {
                    "key": "foo__bar",
                    "path": "/b",
                    "methods": ["GET"],
                    "transports": ["http"],
                    "delivery": "direct",
                },
            ],
        }
        with self.assertRaises(bundle.ContractError):
            bundle.gen_go(contract)


if __name__ == "__main__":
    unittest.main()
