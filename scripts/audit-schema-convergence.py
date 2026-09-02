#!/usr/bin/env python3
"""Validate source-authority topology and compare generated schema artifacts.

TypeSpec and JSON Schema/OpenAPI are peer top-level authorities. They may each
originate facts in their own lane, but every overlapping SQL, language-type,
ORM, and RPC fact must converge. The comparison gate never chooses one source
because it ran first or because an emitter happens to be newer: an unexpected
difference is a release veto and an explicit pause-and-evaluate event.

The comparison input is deliberately producer-neutral. Each generator writes a
small JSON manifest with this shape:

    {
      "schemaVersion": "ores.schema-convergence.v1",
      "authority": "typespec" | "json-schema-openapi",
      "contract": { ... canonical generated facts ... }
    }

`contract` should contain normalized SQL, language type/interface, RPC, and ORM
facts. Maps are compared independent of key order; arrays are ordered and must
be canonicalized by their producer. Expected deltas are exact JSON Pointer
paths with an owner, reason, and expiry. Wildcards and unused exceptions fail.
"""
from __future__ import annotations

import argparse
import copy
import json
import sys
from dataclasses import dataclass
from datetime import date
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "idl" / "source-authorities.json"
POLICY_VERSION = "ores.source-authorities.v1"
MANIFEST_VERSION = "ores.schema-convergence.v1"
DELTA_VERSION = "ores.schema-expected-deltas.v1"
AUTHORITIES = {"typespec", "json-schema-openapi"}


class AuditError(ValueError):
    """Configuration or manifest error that must fail the release gate."""


@dataclass(frozen=True)
class Difference:
    path: str
    kind: str
    left: Any
    right: Any

    def as_dict(self) -> dict[str, Any]:
        return {
            "path": self.path,
            "kind": self.kind,
            "left": self.left,
            "right": self.right,
        }


@dataclass(frozen=True)
class ExpectedDelta:
    path: str
    reason: str
    owner: str
    expires: date

    def as_dict(self) -> dict[str, Any]:
        return {
            "path": self.path,
            "reason": self.reason,
            "owner": self.owner,
            "expires": self.expires.isoformat(),
        }


def load_json(path: Path) -> dict[str, Any]:
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as error:
        raise AuditError(f"cannot read {path}: {error}") from error
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        raise AuditError(f"invalid JSON in {path}: {error}") from error
    if not isinstance(value, dict):
        raise AuditError(f"{path} must contain a JSON object")
    return value


def _string_set(value: Any, *, field: str) -> set[str]:
    if not isinstance(value, list) or not value or not all(
        isinstance(item, str) and item.strip() for item in value
    ):
        raise AuditError(f"{field} must be a non-empty array of non-empty strings")
    if len(value) != len(set(value)):
        raise AuditError(f"{field} contains duplicate values")
    return set(value)


def _object(value: Any, *, field: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise AuditError(f"{field} must be an object")
    return value


def validate_policy(policy: dict[str, Any]) -> None:
    errors: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    expect(
        policy.get("schemaVersion") == POLICY_VERSION,
        f"schemaVersion must be {POLICY_VERSION!r}",
    )

    try:
        top_level = _string_set(
            policy.get("topLevelAuthorities"), field="topLevelAuthorities"
        )
    except AuditError as error:
        errors.append(str(error))
        top_level = set()
    expect(
        top_level == AUTHORITIES,
        "topLevelAuthorities must contain exactly TypeSpec and JSON Schema/OpenAPI",
    )

    pipelines = policy.get("authorityPipelines")
    if not isinstance(pipelines, dict):
        errors.append("authorityPipelines must be an object")
        pipelines = {}

    required_outputs = {
        "typespec": {"sql", "protobuf", "grpc", "wire-clients"},
        "json-schema-openapi": {
            "sql",
            "language-interfaces",
            "language-types",
            "write-clients",
        },
    }
    for authority, required in required_outputs.items():
        pipeline = pipelines.get(authority)
        if not isinstance(pipeline, dict):
            errors.append(f"authorityPipelines.{authority} must be an object")
            continue
        try:
            outputs = _string_set(
                pipeline.get("outputs"), field=f"authorityPipelines.{authority}.outputs"
            )
        except AuditError as error:
            errors.append(str(error))
            continue
        missing = sorted(required - outputs)
        expect(
            not missing,
            f"authorityPipelines.{authority}.outputs is missing {missing}",
        )

    projections = policy.get("projections")
    if not isinstance(projections, dict):
        errors.append("projections must be an object")
        projections = {}
    for projection in ("protobuf", "grpc"):
        config = projections.get(projection)
        if not isinstance(config, dict):
            errors.append(f"projections.{projection} must be an object")
            continue
        expect(
            config.get("producedFrom") == "typespec",
            f"projections.{projection}.producedFrom must be 'typespec'",
        )
        expect(
            config.get("releaseVeto") is True,
            f"projections.{projection}.releaseVeto must be true",
        )
        expect(
            config.get("maySilentlyOverrideSource") is False,
            f"projections.{projection}.maySilentlyOverrideSource must be false",
        )
    openapi = projections.get("openapi")
    if not isinstance(openapi, dict):
        errors.append("projections.openapi must be an object")
    else:
        expect(
            openapi.get("producedFrom") == "json-schema-openapi",
            "projections.openapi.producedFrom must be 'json-schema-openapi'",
        )
        expect(
            openapi.get("releaseVeto") is True,
            "projections.openapi.releaseVeto must be true",
        )
        expect(
            openapi.get("maySilentlyOverrideSource") is False,
            "projections.openapi.maySilentlyOverrideSource must be false",
        )

    convergence = policy.get("convergence")
    if not isinstance(convergence, dict):
        errors.append("convergence must be an object")
        convergence = {}
    expect(
        convergence.get("onUnexpectedDifference") == "pause-and-evaluate",
        "convergence.onUnexpectedDifference must be 'pause-and-evaluate'",
    )
    expect(
        convergence.get("automaticWinner") is None,
        "convergence.automaticWinner must be null",
    )
    try:
        overlapping = _string_set(
            convergence.get("overlappingFacts"),
            field="convergence.overlappingFacts",
        )
    except AuditError as error:
        errors.append(str(error))
        overlapping = set()
    required_overlap = {
        "field-names",
        "scalar-types",
        "nullability",
        "defaults",
        "enum-values",
        "table-names",
        "column-types",
        "primary-keys",
        "foreign-keys",
        "indexes",
        "rpc-operation-names",
        "request-types",
        "response-types",
        "error-types",
    }
    expect(
        required_overlap <= overlapping,
        f"convergence.overlappingFacts is missing {sorted(required_overlap - overlapping)}",
    )

    delta_policy = convergence.get("expectedDeltaPolicy")
    if not isinstance(delta_policy, dict):
        errors.append("convergence.expectedDeltaPolicy must be an object")
    else:
        for field in (
            "exactPathsOnly",
            "reasonRequired",
            "ownerRequired",
            "expiryRequired",
            "unusedDeltaIsError",
        ):
            expect(
                delta_policy.get(field) is True,
                f"convergence.expectedDeltaPolicy.{field} must be true",
            )

    orm = policy.get("ormCrossCheck")
    if not isinstance(orm, dict):
        errors.append("ormCrossCheck must be an object")
        orm = {}
    try:
        engines = _string_set(orm.get("engines"), field="ormCrossCheck.engines")
    except AuditError as error:
        errors.append(str(error))
        engines = set()
    expect(engines == {"diesel", "seaorm"}, "ORM engines must be Diesel and SeaORM")
    expect(
        orm.get("relationship") == "mutual-cross-check",
        "ormCrossCheck.relationship must be 'mutual-cross-check'",
    )
    expect(
        orm.get("onUnexpectedDifference") == "pause-and-evaluate",
        "ormCrossCheck.onUnexpectedDifference must be 'pause-and-evaluate'",
    )

    rpc_docs = policy.get("rpcDocs")
    if not isinstance(rpc_docs, dict):
        errors.append("rpcDocs must be an object")
        rpc_docs = {}
    expect(
        rpc_docs.get("couplingBoundary") == "scripts/rpc-contract-bundle.py",
        "rpcDocs.couplingBoundary must be scripts/rpc-contract-bundle.py",
    )
    expect(
        rpc_docs.get("requiredSharedDigest") == "sha256",
        "rpcDocs.requiredSharedDigest must be sha256",
    )
    expect(
        rpc_docs.get("docsOnlyBypassAllowed") is False,
        "rpcDocs.docsOnlyBypassAllowed must be false",
    )
    expect(
        rpc_docs.get("languageOnlyBypassAllowed") is False,
        "rpcDocs.languageOnlyBypassAllowed must be false",
    )
    try:
        languages = _string_set(
            rpc_docs.get("requiredLanguageSurfaces"),
            field="rpcDocs.requiredLanguageSurfaces",
        )
    except AuditError as error:
        errors.append(str(error))
        languages = set()
    expect(
        {"rust", "typescript", "dart", "gleam", "go"} <= languages,
        "RPC docs must be coupled to Rust, TypeScript, Dart, Gleam, and Go surfaces",
    )

    if errors:
        raise AuditError("source-authority policy errors:\n- " + "\n- ".join(errors))


def validate_manifest(manifest: dict[str, Any], *, label: str) -> None:
    if manifest.get("schemaVersion") != MANIFEST_VERSION:
        raise AuditError(f"{label}.schemaVersion must be {MANIFEST_VERSION!r}")
    authority = manifest.get("authority")
    if authority not in AUTHORITIES:
        raise AuditError(f"{label}.authority must be one of {sorted(AUTHORITIES)}")
    contract = manifest.get("contract")
    if not isinstance(contract, dict) or not contract:
        raise AuditError(f"{label}.contract must be a non-empty object")


def _escape_pointer(token: str) -> str:
    return token.replace("~", "~0").replace("/", "~1")


def _join_pointer(path: str, token: str) -> str:
    return f"{path}/{_escape_pointer(token)}" if path else f"/{_escape_pointer(token)}"


def compare_values(left: Any, right: Any, *, path: str = "") -> list[Difference]:
    if isinstance(left, dict) and isinstance(right, dict):
        differences: list[Difference] = []
        for key in sorted(set(left) | set(right)):
            child = _join_pointer(path, key)
            if key not in left:
                differences.append(Difference(child, "missing-left", None, right[key]))
            elif key not in right:
                differences.append(Difference(child, "missing-right", left[key], None))
            else:
                differences.extend(compare_values(left[key], right[key], path=child))
        return differences
    if type(left) is not type(right):
        return [Difference(path or "/", "type", left, right)]
    if isinstance(left, list):
        if left == right:
            return []
        return [Difference(path or "/", "ordered-list", left, right)]
    if left != right:
        return [Difference(path or "/", "value", left, right)]
    return []


def compare_manifests(left: dict[str, Any], right: dict[str, Any]) -> list[Difference]:
    validate_manifest(left, label="left")
    validate_manifest(right, label="right")
    if left["authority"] == right["authority"]:
        raise AuditError("comparison requires manifests from the two different authorities")
    return compare_values(left["contract"], right["contract"], path="/contract")


def parse_expected_deltas(document: dict[str, Any], *, today: date) -> list[ExpectedDelta]:
    if document.get("schemaVersion") != DELTA_VERSION:
        raise AuditError(f"expected-delta schemaVersion must be {DELTA_VERSION!r}")
    raw_deltas = document.get("deltas")
    if not isinstance(raw_deltas, list):
        raise AuditError("expected-delta document must contain a deltas array")
    parsed: list[ExpectedDelta] = []
    seen: set[str] = set()
    for index, raw in enumerate(raw_deltas):
        if not isinstance(raw, dict):
            raise AuditError(f"deltas[{index}] must be an object")
        path = raw.get("path")
        reason = raw.get("reason")
        owner = raw.get("owner")
        expires_raw = raw.get("expires")
        if not isinstance(path, str) or not path.startswith("/"):
            raise AuditError(f"deltas[{index}].path must be an exact JSON Pointer")
        if any(marker in path for marker in ("*", "?", "[", "]")):
            raise AuditError(f"deltas[{index}].path may not contain wildcard syntax")
        if path in seen:
            raise AuditError(f"duplicate expected delta for {path}")
        seen.add(path)
        if not isinstance(reason, str) or not reason.strip():
            raise AuditError(f"deltas[{index}].reason is required")
        if not isinstance(owner, str) or not owner.strip():
            raise AuditError(f"deltas[{index}].owner is required")
        if not isinstance(expires_raw, str):
            raise AuditError(f"deltas[{index}].expires is required")
        try:
            expires = date.fromisoformat(expires_raw)
        except ValueError as error:
            raise AuditError(f"deltas[{index}].expires must be YYYY-MM-DD") from error
        if expires < today:
            raise AuditError(f"expected delta {path} expired on {expires.isoformat()}")
        parsed.append(ExpectedDelta(path, reason.strip(), owner.strip(), expires))
    return parsed


def apply_expected_deltas(
    differences: Iterable[Difference], deltas: Iterable[ExpectedDelta]
) -> tuple[list[Difference], list[dict[str, Any]], list[ExpectedDelta]]:
    by_path = {delta.path: delta for delta in deltas}
    unexpected: list[Difference] = []
    matched: list[dict[str, Any]] = []
    used: set[str] = set()
    for difference in differences:
        delta = by_path.get(difference.path)
        if delta is None:
            unexpected.append(difference)
            continue
        used.add(delta.path)
        matched.append(
            {
                "difference": difference.as_dict(),
                "expectedDelta": delta.as_dict(),
            }
        )
    stale = [delta for path, delta in by_path.items() if path not in used]
    return unexpected, matched, stale


def audit_pair(
    left: dict[str, Any],
    right: dict[str, Any],
    *,
    expected_delta_document: dict[str, Any] | None = None,
    today: date | None = None,
) -> dict[str, Any]:
    today = today or date.today()
    differences = compare_manifests(left, right)
    deltas = (
        parse_expected_deltas(expected_delta_document, today=today)
        if expected_delta_document is not None
        else []
    )
    unexpected, matched, stale = apply_expected_deltas(differences, deltas)
    return {
        "schemaVersion": "ores.schema-convergence-report.v1",
        "leftAuthority": left["authority"],
        "rightAuthority": right["authority"],
        "status": "pass" if not unexpected and not stale else "pause-and-evaluate",
        "unexpectedDifferences": [item.as_dict() for item in unexpected],
        "expectedDifferences": matched,
        "unusedExpectedDeltas": [item.as_dict() for item in stale],
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--policy",
        type=Path,
        default=DEFAULT_POLICY,
        help="source-authority topology policy",
    )
    parser.add_argument("--left", type=Path, help="TypeSpec or JSON Schema manifest")
    parser.add_argument("--right", type=Path, help="the peer authority manifest")
    parser.add_argument(
        "--expected-deltas",
        type=Path,
        help="exact, owned, expiring discrepancy allow-list",
    )
    parser.add_argument("--report", type=Path, help="optional JSON report path")
    parser.add_argument(
        "--today",
        type=date.fromisoformat,
        help="override current date for deterministic testing",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        policy = load_json(args.policy)
        validate_policy(policy)
        if (args.left is None) != (args.right is None):
            raise AuditError("--left and --right must be supplied together")
        if args.left is None:
            print(
                json.dumps(
                    {
                        "schemaVersion": POLICY_VERSION,
                        "status": "pass",
                        "topLevelAuthorities": sorted(AUTHORITIES),
                        "onUnexpectedDifference": "pause-and-evaluate",
                    },
                    indent=2,
                    sort_keys=True,
                )
            )
            return 0

        left = load_json(args.left)
        right = load_json(args.right)
        expected = load_json(args.expected_deltas) if args.expected_deltas else None
        report = audit_pair(
            left,
            right,
            expected_delta_document=expected,
            today=args.today,
        )
        rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(rendered, encoding="utf-8")
        sys.stdout.write(rendered)
        return 0 if report["status"] == "pass" else 1
    except AuditError as error:
        print(f"schema convergence audit failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
