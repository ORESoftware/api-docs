#!/usr/bin/env python3
"""Validate the peer-authority and convergence policy for RPC/API contracts."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CONTRACT_PATH = Path("idl/authority-contract.json")

EXPECTED_AUTHORITIES = {
    "typespec": {
        "kind": "human_authored_contract_authority",
        "roots": {"idl/typespec"},
        "requiredOutputs": {"sql", "protobuf", "grpc", "wire_clients"},
    },
    "json-schema-openapi": {
        "kind": "human_authored_contract_authority",
        "roots": {"json-schema", "examples"},
        "requiredOutputs": {
            "client_interfaces",
            "client_types",
            "sql",
            "write_clients",
        },
    },
}
EXPECTED_COMPARISONS = {
    "typespec-vs-json-schema-openapi": {
        "left": "typespec",
        "right": "json-schema-openapi",
        "artifacts": {"normalized_models", "sql", "client_types"},
    },
    "diesel-vs-seaorm": {
        "left": "diesel",
        "right": "seaorm",
        "artifacts": {"schema", "migrations", "constraints", "relations"},
    },
}
ALLOWED_MATERIALIZATION = {"implemented", "not_yet_materialized"}
REQUIRED_MATERIALIZATION = {
    "rpcModelCrossCheck",
    "digestBoundDocsAndClients",
    "typespecSqlEmitter",
    "jsonSchemaOpenApiSqlEmitter",
    "dieselSeaOrmCrossCheck",
}
GOVERNANCE_FILES = (
    "AGENTS.md",
    "README.md",
    "docs/rpc-contract-coupling.md",
    "idl/README.md",
    "idl/typespec/main.tsp",
    "idl/typespec/package.json",
    "scripts/cross-check-rpc-idl.py",
    "scripts/test_cross_check_rpc_idl.py",
)
FORBIDDEN_HIERARCHY_MARKERS = (
    "P0",
    "P1",
    "P2",
    "one authority for shared RPC",
    "cannot redefine the TypeSpec authority",
    "must not redefine P0",
    "begins in TypeSpec",
    "Change the shared semantic fact in TypeSpec first",
    "authoritative TypeSpec model",
)
REQUIRED_GOVERNANCE_MARKERS = {
    "AGENTS.md": (
        "peer, top-level, human-authored contract authorities",
        "halt and evaluate",
    ),
    "README.md": (
        "peer top-level contract authorities",
        "halt and evaluate",
    ),
    "docs/rpc-contract-coupling.md": (
        "peer top-level contract authorities",
        "halt and evaluate",
    ),
    "idl/README.md": (
        "peer top-level authorities",
        "halt and evaluate",
    ),
    "scripts/cross-check-rpc-idl.py": ("peer-authority veto",),
}


def _list(value: Any, label: str, errors: list[str]) -> list[Any]:
    if not isinstance(value, list):
        errors.append(f"{label} must be an array")
        return []
    return value


def _object(value: Any, label: str, errors: list[str]) -> dict[str, Any]:
    if not isinstance(value, dict):
        errors.append(f"{label} must be an object")
        return {}
    return value


def _relative_root(root: Path, raw: Any, label: str, errors: list[str]) -> None:
    if not isinstance(raw, str) or not raw:
        errors.append(f"{label} must be a non-empty relative path")
        return
    candidate = Path(raw)
    if candidate.is_absolute() or ".." in candidate.parts:
        errors.append(f"{label} must remain inside the repository")
        return
    resolved_root = root.resolve()
    resolved = (resolved_root / candidate).resolve()
    try:
        resolved.relative_to(resolved_root)
    except ValueError:
        errors.append(f"{label} escapes the repository")
        return
    if not resolved.exists():
        errors.append(f"{label} does not exist: {raw}")


def validate_contract(document: Any, root: Path) -> list[str]:
    errors: list[str] = []
    contract = _object(document, "contract", errors)
    if contract.get("schemaVersion") != 1:
        errors.append("schemaVersion must equal 1")

    policy = _object(contract.get("policy"), "policy", errors)
    if policy.get("authoritiesArePeers") is not True:
        errors.append("policy.authoritiesArePeers must be true")
    if policy.get("authorityOrder") != []:
        errors.append("policy.authorityOrder must be empty; peer authorities are unordered")
    if policy.get("automaticOverwriteAllowed") is not False:
        errors.append("policy.automaticOverwriteAllowed must be false")
    if policy.get("onUnexpectedDiscrepancy") != "halt_and_evaluate":
        errors.append("policy.onUnexpectedDiscrepancy must be halt_and_evaluate")
    if policy.get("productionPromotionRequiresAllMaterializedGates") is not True:
        errors.append("production promotion must require every materialized gate")

    authorities: dict[str, dict[str, Any]] = {}
    for index, raw in enumerate(_list(contract.get("authorities"), "authorities", errors)):
        authority = _object(raw, f"authorities[{index}]", errors)
        authority_id = authority.get("id")
        if not isinstance(authority_id, str) or not authority_id:
            errors.append(f"authorities[{index}].id is required")
            continue
        if authority_id in authorities:
            errors.append(f"duplicate authority: {authority_id}")
        authorities[authority_id] = authority

    if set(authorities) != set(EXPECTED_AUTHORITIES):
        errors.append(
            "authority set must be exact: "
            f"got={sorted(authorities)}, expected={sorted(EXPECTED_AUTHORITIES)}"
        )
    for authority_id, expected in EXPECTED_AUTHORITIES.items():
        authority = authorities.get(authority_id, {})
        if authority.get("kind") != expected["kind"]:
            errors.append(f"{authority_id}.kind must be {expected['kind']}")
        roots = _list(authority.get("roots"), f"{authority_id}.roots", errors)
        if set(roots) != expected["roots"] or len(roots) != len(set(roots)):
            errors.append(f"{authority_id}.roots must be exact and duplicate-free")
        for index, path in enumerate(roots):
            _relative_root(root, path, f"{authority_id}.roots[{index}]", errors)
        outputs = _list(
            authority.get("requiredOutputs"),
            f"{authority_id}.requiredOutputs",
            errors,
        )
        if set(outputs) != expected["requiredOutputs"] or len(outputs) != len(set(outputs)):
            errors.append(f"{authority_id}.requiredOutputs must be exact and duplicate-free")

    comparisons: dict[str, dict[str, Any]] = {}
    for index, raw in enumerate(_list(contract.get("comparisons"), "comparisons", errors)):
        comparison = _object(raw, f"comparisons[{index}]", errors)
        comparison_id = comparison.get("id")
        if not isinstance(comparison_id, str) or not comparison_id:
            errors.append(f"comparisons[{index}].id is required")
            continue
        if comparison_id in comparisons:
            errors.append(f"duplicate comparison: {comparison_id}")
        comparisons[comparison_id] = comparison

    if set(comparisons) != set(EXPECTED_COMPARISONS):
        errors.append(
            "comparison set must be exact: "
            f"got={sorted(comparisons)}, expected={sorted(EXPECTED_COMPARISONS)}"
        )
    for comparison_id, expected in EXPECTED_COMPARISONS.items():
        comparison = comparisons.get(comparison_id, {})
        for side in ("left", "right"):
            if comparison.get(side) != expected[side]:
                errors.append(f"{comparison_id}.{side} must equal {expected[side]}")
        artifacts = _list(
            comparison.get("artifacts"),
            f"{comparison_id}.artifacts",
            errors,
        )
        if set(artifacts) != expected["artifacts"] or len(artifacts) != len(set(artifacts)):
            errors.append(f"{comparison_id}.artifacts must be exact and duplicate-free")
        if comparison.get("onMismatch") != "halt_and_evaluate":
            errors.append(f"{comparison_id}.onMismatch must be halt_and_evaluate")

    materialization = _object(contract.get("materialization"), "materialization", errors)
    if set(materialization) != REQUIRED_MATERIALIZATION:
        errors.append("materialization keys must be exact")
    for name, status in materialization.items():
        if status not in ALLOWED_MATERIALIZATION:
            errors.append(f"materialization.{name} has invalid status {status!r}")
    if materialization.get("rpcModelCrossCheck") != "implemented":
        errors.append("rpcModelCrossCheck must remain implemented")
    if materialization.get("digestBoundDocsAndClients") != "implemented":
        errors.append("digestBoundDocsAndClients must remain implemented")

    for relative in GOVERNANCE_FILES:
        path = root / relative
        if not path.is_file():
            errors.append(f"missing governance file: {relative}")
            continue
        text = path.read_text(encoding="utf-8")
        for marker in FORBIDDEN_HIERARCHY_MARKERS:
            if marker in text:
                errors.append(f"{relative} retains obsolete hierarchy marker: {marker}")
        for marker in REQUIRED_GOVERNANCE_MARKERS.get(relative, ()):
            if marker not in text:
                errors.append(f"{relative} is missing peer-authority marker: {marker}")

    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--contract", type=Path)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    contract_path = args.contract or root / CONTRACT_PATH
    if not contract_path.is_absolute():
        contract_path = (root / contract_path).resolve()
    try:
        document = json.loads(contract_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        sys.stderr.write(f"unable to load authority contract: {error}\n")
        return 66

    errors = validate_contract(document, root)
    if errors:
        sys.stderr.write("peer-authority contract veto; halt and evaluate\n")
        for error in errors:
            sys.stderr.write(f"  {error}\n")
        return 1
    sys.stdout.write("peer-authority contract and convergence policy are valid\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
