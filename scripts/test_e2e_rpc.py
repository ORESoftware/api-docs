#!/usr/bin/env python3
"""Schema-level e2e for call/receipt/telemetry/envelope (no sockets)."""
from __future__ import annotations

import json
import re
import sys
import unittest
from pathlib import Path

import jsonschema

ROOT = Path(__file__).resolve().parents[1]


def load_json(rel: str) -> dict:
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


def validator(schema_name: str):
    schema = load_json(f"json-schema/{schema_name}")
    cls = getattr(jsonschema, "Draft202012Validator", jsonschema.Draft7Validator)
    return cls(schema)


class RpcE2E(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.map = load_json("examples/rpc-transports.route-map.json")
        cls.call_v = validator("rpc-call.schema.json")
        cls.receipt_v = validator("rpc-receipt.schema.json")
        cls.tel_v = validator("telemetry-attributes.schema.json")
        cls.env_v = validator("opto-sync-envelope.schema.json")
        cls.map_v = validator("route-map.schema.json")

    def test_example_map_and_get_item_on_three_transports(self):
        self.map_v.validate(self.map)
        entry = self.map["map"]["get_item"]
        self.assertEqual(entry["path"], "/v1/items/{id}")
        self.assertEqual(entry["transports"], ["http", "tcp", "websocket"])
        self.assertEqual(entry["tcp_framing"], "ndjson")
        self.assertEqual(self.map["map"]["tcp_ping"]["transports"], ["tcp"])
        self.assertEqual(self.map["map"]["nats_ping"]["transports"], ["nats"])
        self.assertEqual(self.map["map"]["websocket"]["transports"], ["websocket"])

    def test_same_call_json_valid_on_each_transport(self):
        bodies = []
        for transport, call_id in (
            ("http", "http-get-item"),
            ("tcp", "tcp-get-item"),
            ("websocket", "ws-get-item"),
        ):
            call = {
                "v": 1,
                "op": "call",
                "id": call_id,
                "key": "get_item",
                "transport": transport,
                "path": {"id": "item-42"},
                "traceId": "4bf92f3577b34da6a3ce929d0e0e4736",
                "spanId": "00f067aa0ba902b7",
            }
            self.call_v.validate(call)
            line = json.dumps(call, separators=(",", ":")) + "\n"
            self.assertTrue(line.endswith("\n"))
            back = json.loads(line)
            self.call_v.validate(back)
            receipt = {
                "v": 1,
                "op": "receipt",
                "id": call_id,
                "key": "get_item",
                "transport": transport,
                "ok": True,
                "status": 200,
                "body": {"id": "item-42", "name": "item-item-42"},
                "traceId": call["traceId"],
                "spanId": call["spanId"],
            }
            self.receipt_v.validate(receipt)
            bodies.append(receipt["body"])
            fields = {
                "rpc.system": "ores-api-docs",
                "rpc.service": self.map["service"],
                "rpc.method": "get_item",
                "rpc.transport": transport,
                "rpc.ok": True,
                "http.status_code": 200,
            }
            self.tel_v.validate(fields)
            self.assertNotIn("body", fields)
            log = {
                "fields": fields,
                "traceId": call["traceId"],
                "spanId": call["spanId"],
            }
            self.assertEqual(log["fields"]["rpc.system"], "ores-api-docs")
            self.assertRegex(log["traceId"], r"^[0-9a-f]{32}$")
        self.assertEqual(bodies[0], bodies[1])
        self.assertEqual(bodies[1], bodies[2])

    def test_opto_sync_envelope_is_the_map_not_a_call(self):
        env = {
            "id": self.map["service"],
            "scope": "ores.api-docs.route-map",
            "kind": "ores.api-docs.route-map",
            "record_id": self.map["service"],
            "updatedAt": "1689940800123456789",
            "payload": self.map,
        }
        self.env_v.validate(env)
        stuffed = dict(env)
        stuffed["payload"] = {
            "v": 1,
            "op": "call",
            "id": "nope",
            "key": "get_item",
        }
        with self.assertRaises(jsonschema.ValidationError):
            self.env_v.validate(stuffed)

    def test_rust_crate_does_not_depend_on_opto_sync_or_ores_otel(self):
        cargo = (ROOT / "rust" / "Cargo.toml").read_text(encoding="utf-8")
        deps = cargo.split("[dependencies]", 1)[1].split("[", 1)[0]
        self.assertNotIn("opto-sync", deps)
        self.assertNotIn("ores-otel", deps)
        zpkg = (ROOT / ".zpkg.toml").read_text(encoding="utf-8")
        dep_block = zpkg.split("[dependencies]", 1)[1].split("[", 1)[0]
        self.assertTrue(re.match(r"^\s*$", dep_block), dep_block)

    def test_golden_fixtures_and_omitted_transport(self):
        golden = load_json("tests/generated-contract/valid/rpc-call.json")
        self.call_v.validate(golden)
        self.assertEqual(golden["id"], "tcp-get-item")
        self.assertEqual(golden["transport"], "tcp")
        omitted = {"v": 1, "op": "call", "id": "inferred", "key": "get_item"}
        self.call_v.validate(omitted)
        self.assertNotIn("transport", json.dumps(omitted))
        receipt = load_json("tests/generated-contract/valid/rpc-receipt.json")
        self.receipt_v.validate(receipt)
        self.assertEqual(receipt["id"], golden["id"])

    def test_schema_rejects_illegal_call_and_receipt_shapes(self):
        invalid_fixture = load_json("tests/generated-contract/invalid/rpc-call.json")
        with self.assertRaises(jsonschema.ValidationError):
            self.call_v.validate(invalid_fixture)
        for bad in (
            {"v": 2, "op": "call", "id": "c", "key": "get_item"},
            {"v": 1, "op": "invoke", "id": "c", "key": "get_item"},
            {"v": 1, "op": "call", "id": "", "key": "get_item"},
            {"v": 1, "op": "call", "id": "c", "key": "get-item"},
            {"v": 1, "op": "call", "id": "c", "key": "get_item", "transport": "grpc"},
            {"v": 1, "op": "call", "id": "c", "key": "get_item", "extra": True},
        ):
            with self.subTest(bad=bad):
                with self.assertRaises(jsonschema.ValidationError):
                    self.call_v.validate(bad)
        with self.assertRaises(jsonschema.ValidationError):
            self.receipt_v.validate(
                {"v": 1, "op": "receipt", "id": "c", "key": "get_item"}
            )

    def test_receipt_copies_call_correlation_and_ndjson_is_one_object(self):
        call = {
            "v": 1,
            "op": "call",
            "id": "corr-1",
            "key": "get_item",
            "path": {"id": "item-42"},
        }
        self.call_v.validate(call)
        line = json.dumps(call, separators=(",", ":")) + "\n"
        self.assertEqual(line.count("\n"), 1)
        self.assertFalse(line.startswith("\n"))
        receipt = {
            "v": 1,
            "op": "receipt",
            "id": call["id"],
            "key": call["key"],
            "ok": True,
            "status": 200,
            "body": {"id": "item-42", "name": "item-item-42"},
        }
        self.receipt_v.validate(receipt)
        self.assertEqual(receipt["id"], call["id"])
        self.assertEqual(receipt["key"], call["key"])
        nats = {
            "v": 1,
            "op": "call",
            "id": "nats-1",
            "key": "get_item",
            "transport": "nats",
            "path": {"id": "item-42"},
        }
        self.call_v.validate(nats)

    def test_v1_call_is_not_a_ridl_frame(self):
        frame_v = validator("rpc-frame.schema.json")
        call = {"v": 1, "op": "call", "id": "c", "key": "get_item"}
        self.call_v.validate(call)
        with self.assertRaises(jsonschema.ValidationError):
            frame_v.validate(call)
        ridl_call = {
            "v": 1,
            "id": "c",
            "t": "call",
            "key": "healthz",
            "method": "GET",
            "path": "/healthz",
        }
        frame_v.validate(ridl_call)
        with self.assertRaises(jsonschema.ValidationError):
            self.call_v.validate(ridl_call)

    def test_runtime_crate_and_zed_bin_stay_decoupled(self):
        runtime = (ROOT / "runtime" / "rust" / "Cargo.toml").read_text(encoding="utf-8")
        runtime = runtime.split("[dependencies]", 1)[1].split("[", 1)[0]
        self.assertNotIn("opto-sync", runtime)
        self.assertNotIn("ores-otel", runtime)
        self.assertTrue((ROOT / "scripts" / "ridl").is_file())
        zpkg = (ROOT / ".zpkg.toml").read_text(encoding="utf-8")
        self.assertNotIn("opto-sync", zpkg.split("[dependencies]", 1)[1].split("[", 1)[0])
        self.assertNotIn('"opto-sync"', zpkg.split("keywords", 1)[1].split("\n", 1)[0])

    def test_ridl_refuses_v1_maps(self):
        if str(ROOT) not in sys.path:
            sys.path.insert(0, str(ROOT))
        from ridl.model import RidlError, load_route_map

        with self.assertRaises(RidlError) as ctx:
            load_route_map(ROOT / "examples" / "pmap-api.route-map.json")
        self.assertIn("v1 map", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
