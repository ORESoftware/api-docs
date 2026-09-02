#!/usr/bin/env python3
from __future__ import annotations

import copy
import importlib.util
import json
import sys
import unittest
from datetime import date
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "audit_schema_convergence",
    ROOT / "scripts" / "audit-schema-convergence.py",
)
audit = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = audit
SPEC.loader.exec_module(audit)


def manifest(authority: str, *, sql_type: str = "uuid") -> dict:
    return {
        "schemaVersion": audit.MANIFEST_VERSION,
        "authority": authority,
        "contract": {
            "models": {
                "Principal": {
                    "fields": {
                        "id": {"type": "string", "required": True},
                        "displayName": {"type": "string", "required": False},
                    }
                }
            },
            "sql": {
                "tables": {
                    "principals": {
                        "columns": {
                            "id": {"type": sql_type, "nullable": False},
                            "display_name": {"type": "text", "nullable": True},
                        },
                        "primaryKey": ["id"],
                    }
                }
            },
            "orm": {
                "diesel": {"models": ["Principal"]},
                "seaorm": {"models": ["Principal"]},
            },
            "rpc": {
                "operations": {
                    "principal.get": {
                        "request": "PrincipalGetRequest",
                        "response": "Principal",
                    }
                }
            },
        },
    }


class SourceAuthorityPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.policy = json.loads(
            (ROOT / "idl" / "source-authorities.json").read_text(encoding="utf-8")
        )

    def test_repository_policy_is_valid(self) -> None:
        audit.validate_policy(copy.deepcopy(self.policy))

    def test_typespec_cannot_be_made_parent_of_json_schema(self) -> None:
        bad = copy.deepcopy(self.policy)
        bad["topLevelAuthorities"] = ["typespec"]
        with self.assertRaisesRegex(audit.AuditError, "topLevelAuthorities"):
            audit.validate_policy(bad)

    def test_protobuf_must_remain_downstream_of_typespec(self) -> None:
        bad = copy.deepcopy(self.policy)
        bad["projections"]["protobuf"]["producedFrom"] = "json-schema-openapi"
        with self.assertRaisesRegex(audit.AuditError, "protobuf"):
            audit.validate_policy(bad)

    def test_rpc_docs_cannot_bypass_shared_bundle(self) -> None:
        bad = copy.deepcopy(self.policy)
        bad["rpcDocs"]["docsOnlyBypassAllowed"] = True
        with self.assertRaisesRegex(audit.AuditError, "docsOnlyBypassAllowed"):
            audit.validate_policy(bad)


class ManifestComparisonTests(unittest.TestCase):
    def test_equal_peer_manifests_pass(self) -> None:
        report = audit.audit_pair(
            manifest("typespec"),
            manifest("json-schema-openapi"),
            today=date(2026, 9, 2),
        )
        self.assertEqual(report["status"], "pass")
        self.assertEqual(report["unexpectedDifferences"], [])

    def test_sql_difference_pauses_release(self) -> None:
        report = audit.audit_pair(
            manifest("typespec", sql_type="uuid"),
            manifest("json-schema-openapi", sql_type="text"),
            today=date(2026, 9, 2),
        )
        self.assertEqual(report["status"], "pause-and-evaluate")
        self.assertEqual(
            report["unexpectedDifferences"][0]["path"],
            "/contract/sql/tables/principals/columns/id/type",
        )

    def test_exact_owned_unexpired_delta_can_explain_one_difference(self) -> None:
        expected = {
            "schemaVersion": audit.DELTA_VERSION,
            "deltas": [
                {
                    "path": "/contract/sql/tables/principals/columns/id/type",
                    "reason": "Temporary external database compatibility window",
                    "owner": "schema-platform",
                    "expires": "2026-09-30",
                }
            ],
        }
        report = audit.audit_pair(
            manifest("typespec", sql_type="uuid"),
            manifest("json-schema-openapi", sql_type="text"),
            expected_delta_document=expected,
            today=date(2026, 9, 2),
        )
        self.assertEqual(report["status"], "pass")
        self.assertEqual(len(report["expectedDifferences"]), 1)

    def test_unused_expected_delta_is_a_release_pause(self) -> None:
        expected = {
            "schemaVersion": audit.DELTA_VERSION,
            "deltas": [
                {
                    "path": "/contract/sql/tables/principals/columns/id/type",
                    "reason": "No longer needed",
                    "owner": "schema-platform",
                    "expires": "2026-09-30",
                }
            ],
        }
        report = audit.audit_pair(
            manifest("typespec"),
            manifest("json-schema-openapi"),
            expected_delta_document=expected,
            today=date(2026, 9, 2),
        )
        self.assertEqual(report["status"], "pause-and-evaluate")
        self.assertEqual(len(report["unusedExpectedDeltas"]), 1)

    def test_expired_or_wildcard_delta_is_rejected(self) -> None:
        expired = {
            "schemaVersion": audit.DELTA_VERSION,
            "deltas": [
                {
                    "path": "/contract/sql/tables/principals/columns/id/type",
                    "reason": "Expired",
                    "owner": "schema-platform",
                    "expires": "2026-09-01",
                }
            ],
        }
        with self.assertRaisesRegex(audit.AuditError, "expired"):
            audit.parse_expected_deltas(expired, today=date(2026, 9, 2))

        wildcard = copy.deepcopy(expired)
        wildcard["deltas"][0]["expires"] = "2026-09-30"
        wildcard["deltas"][0]["path"] = "/contract/sql/*"
        with self.assertRaisesRegex(audit.AuditError, "wildcard"):
            audit.parse_expected_deltas(wildcard, today=date(2026, 9, 2))

    def test_same_authority_comparison_is_rejected(self) -> None:
        with self.assertRaisesRegex(audit.AuditError, "different authorities"):
            audit.compare_manifests(
                manifest("typespec"),
                manifest("typespec"),
            )


if __name__ == "__main__":
    unittest.main()
