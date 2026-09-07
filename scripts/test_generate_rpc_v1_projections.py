#!/usr/bin/env python3
"""Tests for deterministic RPC v1 SQL/Protobuf/gRPC projection."""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "generate-rpc-v1-projections.py"
PROTO = ROOT / "idl" / "protobuf" / "ores" / "rpc" / "v1" / "rpc.proto"
SQL = ROOT / "generated" / "rpc-v1" / "rpc-storage.sql"
GRPC = ROOT / "generated" / "rpc-v1" / "grpc.json"
LOCK = ROOT / "idl" / "protobuf.lock.json"

COPY_INPUTS = (
    "scripts/generate-rpc-v1-projections.py",
    "scripts/rpc_v1_projection_core.py",
    "idl/rpc-v1.projection.json",
    "idl/typespec/v1.tsp",
    "idl/protobuf.lock.json",
    "json-schema/rpc-call.schema.json",
    "json-schema/rpc-receipt.schema.json",
)


class DerivedProjectionTest(unittest.TestCase):
    maxDiff = None

    def test_committed_outputs_are_byte_identical_to_regeneration(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(GENERATOR), "--root", str(ROOT), "--check"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr or completed.stdout)
        self.assertIn("verified RPC v1 SQL/Protobuf/gRPC projection", completed.stdout)

    def test_proto_preserves_payload_identity_and_uses_buf_compliant_wrappers(self) -> None:
        text = PROTO.read_text(encoding="utf-8")
        lock = json.loads(LOCK.read_text(encoding="utf-8"))
        self.assertIn("service RpcService {", text)
        self.assertIn("rpc Call(CallRequest) returns (CallResponse);", text)
        self.assertIn("message CallRequest {\n  RpcCall call = 1;\n}", text)
        self.assertIn("message CallResponse {\n  RpcReceipt receipt = 1;\n}", text)
        self.assertNotIn("message RpcGatewayCallRequest", text)
        self.assertIn(
            "Generated Protobuf adapters must run the shared semantic validator.",
            text,
        )

        expected_messages = {
            "ores.rpc.v1.RpcCall",
            "ores.rpc.v1.RpcReceipt",
            "ores.rpc.v1.CallRequest",
            "ores.rpc.v1.CallResponse",
            "ores.rpc.v2.RpcFrame",
        }
        self.assertEqual(set(lock["messages"]), expected_messages)

        for message_name in ("RpcCall", "RpcReceipt"):
            body_match = re.search(
                rf"message {message_name} \{{(?P<body>.*?)\n\}}",
                text,
                re.DOTALL,
            )
            self.assertIsNotNone(body_match)
            actual: dict[str, int] = {}
            for match in re.finditer(
                r"^\s*(?:optional\s+)?[A-Za-z_][A-Za-z0-9_.]*\s+"
                r"([a-z_][a-z0-9_]*)\s*=\s*(\d+)",
                body_match.group("body"),
                re.MULTILINE,
            ):
                source_name = {
                    "trace_id": "traceId",
                    "span_id": "spanId",
                }.get(match.group(1), match.group(1))
                actual[source_name] = int(match.group(2))
            self.assertEqual(
                actual,
                lock["messages"][f"ores.rpc.v1.{message_name}"]["fields"],
            )
        self.assertEqual(
            lock["messages"]["ores.rpc.v1.CallRequest"]["fields"],
            {"call": 1},
        )
        self.assertEqual(
            lock["messages"]["ores.rpc.v1.CallResponse"]["fields"],
            {"receipt": 1},
        )

    def test_sql_materializes_shared_shape_and_receipt_state(self) -> None:
        text = SQL.read_text(encoding="utf-8")
        self.assertIn("CREATE TABLE IF NOT EXISTS ores_rpc.calls_v1", text)
        self.assertIn("CREATE TABLE IF NOT EXISTS ores_rpc.receipts_v1", text)
        self.assertIn("ok = TRUE AND error IS NULL", text)
        self.assertIn("ok = FALSE AND error IS NOT NULL AND body IS NULL", text)
        self.assertIn("REVOKE ALL ON TABLE ores_rpc.calls_v1 FROM PUBLIC", text)
        self.assertIn("REVOKE ALL ON TABLE ores_rpc.receipts_v1 FROM PUBLIC", text)
        self.assertNotRegex(
            text,
            r"\b(?:password|access_token|refresh_token|api_key|client_secret)\b",
        )

    def test_every_projection_has_the_same_source_digest(self) -> None:
        manifest = json.loads(GRPC.read_text(encoding="utf-8"))
        digest = manifest["projectionSha256"]
        self.assertRegex(digest, r"^[0-9a-f]{64}$")
        self.assertIn(f"projection_sha256: {digest}", PROTO.read_text(encoding="utf-8"))
        self.assertIn(f"projection_sha256: {digest}", SQL.read_text(encoding="utf-8"))
        self.assertEqual(manifest["service"]["fullName"], "ores.rpc.v1.RpcService")
        self.assertEqual(
            manifest["service"]["methods"],
            [
                {
                    "clientStreaming": False,
                    "fullName": "ores.rpc.v1.RpcService.Call",
                    "name": "Call",
                    "request": "ores.rpc.v1.CallRequest",
                    "requestPayload": "ores.rpc.v1.RpcCall",
                    "response": "ores.rpc.v1.CallResponse",
                    "responsePayload": "ores.rpc.v1.RpcReceipt",
                    "serverStreaming": False,
                }
            ],
        )
        self.assertEqual(
            manifest["wireCompatibility"],
            {
                "bufCompliantWrappers": ["CallRequest", "CallResponse"],
                "requestPayload": "RpcCall",
                "responsePayload": "RpcReceipt",
                "stablePayloadMessagesRenamed": False,
            },
        )

    def copy_fixture(self, target: Path) -> None:
        for relative in COPY_INPUTS:
            source = ROOT / relative
            destination = target / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)

    def test_authority_inventory_drift_stops_before_emission(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)
            self.copy_fixture(target)
            call_path = target / "json-schema" / "rpc-call.schema.json"
            call = json.loads(call_path.read_text(encoding="utf-8"))
            del call["properties"]["headers"]
            call_path.write_text(json.dumps(call, indent=2) + "\n", encoding="utf-8")

            completed = subprocess.run(
                [
                    sys.executable,
                    str(target / "scripts/generate-rpc-v1-projections.py"),
                    "--root",
                    str(target),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("authority field inventory drift", completed.stderr)
            self.assertFalse(
                (target / "generated/rpc-v1/rpc-storage.sql").exists()
            )

    def test_unreviewed_payload_field_number_drift_stops_before_emission(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)
            self.copy_fixture(target)
            lock_path = target / "idl/protobuf.lock.json"
            lock = json.loads(lock_path.read_text(encoding="utf-8"))
            lock["messages"]["ores.rpc.v1.RpcCall"]["fields"]["headers"] = 10
            lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")

            completed = subprocess.run(
                [
                    sys.executable,
                    str(target / "scripts/generate-rpc-v1-projections.py"),
                    "--root",
                    str(target),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("reuses a field number", completed.stderr)

    def test_missing_or_reused_wrapper_identity_stops_before_emission(self) -> None:
        for mutation, expected in (
            ("missing", "CallRequest Protobuf field ledger drift"),
            ("reserved", "CallRequest reuses a reserved field number"),
        ):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                target = Path(temporary)
                self.copy_fixture(target)
                lock_path = target / "idl/protobuf.lock.json"
                lock = json.loads(lock_path.read_text(encoding="utf-8"))
                request = lock["messages"]["ores.rpc.v1.CallRequest"]
                if mutation == "missing":
                    request["fields"] = {}
                else:
                    request["reserved"] = [1]
                lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")

                completed = subprocess.run(
                    [
                        sys.executable,
                        str(target / "scripts/generate-rpc-v1-projections.py"),
                        "--root",
                        str(target),
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertNotEqual(completed.returncode, 0)
                self.assertIn(expected, completed.stderr)


if __name__ == "__main__":
    unittest.main()
