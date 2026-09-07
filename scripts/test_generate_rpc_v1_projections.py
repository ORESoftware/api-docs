#!/usr/bin/env python3
"""Tests for the deterministic RPC v1 SQL/Protobuf/gRPC projection."""
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

    def test_proto_contains_locked_messages_and_real_grpc_service(self) -> None:
        text = PROTO.read_text(encoding="utf-8")
        lock = json.loads(LOCK.read_text(encoding="utf-8"))
        self.assertIn("service RpcGateway {", text)
        self.assertIn("rpc Call(RpcCall) returns (RpcReceipt);", text)
        self.assertIn("Generated Protobuf adapters must run the shared semantic validator.", text)

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
        self.assertEqual(manifest["service"]["fullName"], "ores.rpc.v1.RpcGateway")
        self.assertEqual(
            manifest["service"]["methods"],
            [
                {
                    "clientStreaming": False,
                    "fullName": "ores.rpc.v1.RpcGateway.Call",
                    "name": "Call",
                    "request": "ores.rpc.v1.RpcCall",
                    "response": "ores.rpc.v1.RpcReceipt",
                    "serverStreaming": False,
                }
            ],
        )

    def test_authority_inventory_drift_stops_before_emission(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)
            for relative in (
                "scripts/generate-rpc-v1-projections.py",
                "idl/rpc-v1.projection.json",
                "idl/typespec/v1.tsp",
                "idl/protobuf.lock.json",
                "json-schema/rpc-call.schema.json",
                "json-schema/rpc-receipt.schema.json",
            ):
                source = ROOT / relative
                destination = target / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)

            call_path = target / "json-schema" / "rpc-call.schema.json"
            call = json.loads(call_path.read_text(encoding="utf-8"))
            del call["properties"]["headers"]
            call_path.write_text(json.dumps(call, indent=2) + "\n", encoding="utf-8")

            completed = subprocess.run(
                [
                    sys.executable,
                    str(target / "scripts" / "generate-rpc-v1-projections.py"),
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
                (target / "generated" / "rpc-v1" / "rpc-storage.sql").exists()
            )

    def test_unreviewed_field_number_drift_stops_before_emission(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)
            for relative in (
                "scripts/generate-rpc-v1-projections.py",
                "idl/rpc-v1.projection.json",
                "idl/typespec/v1.tsp",
                "idl/protobuf.lock.json",
                "json-schema/rpc-call.schema.json",
                "json-schema/rpc-receipt.schema.json",
            ):
                source = ROOT / relative
                destination = target / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)

            lock_path = target / "idl" / "protobuf.lock.json"
            lock = json.loads(lock_path.read_text(encoding="utf-8"))
            lock["messages"]["ores.rpc.v1.RpcCall"]["fields"]["headers"] = 10
            lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")

            completed = subprocess.run(
                [
                    sys.executable,
                    str(target / "scripts" / "generate-rpc-v1-projections.py"),
                    "--root",
                    str(target),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("reuses a field number", completed.stderr)


if __name__ == "__main__":
    unittest.main()
