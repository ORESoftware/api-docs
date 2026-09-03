#!/usr/bin/env python3
"""Audit policy-bearing docs for the peer-authority invariant and emit a receipt.

This sweep is deliberately scoped to durable documentation. The existing RPC
IDL cross-check and strict audit remain the semantic gates for contract data.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

FORMAT = "ores.schema-audit-receipt/v1"
SCRIPT_VERSION = "1.0.0"

ACTIVE_POLICY_FILES = (
    "README.md",
    "AGENTS.md",
    "idl/README.md",
    "docs/rpc-contract-coupling.md",
)
ADR_FILE = "docs/adr/0001-peer-schema-authority.md"

# These exact strings were active hierarchy claims before ADR 0001. Historical
# quotations in the ADR are intentionally excluded from this exact-string scan.
PROHIBITED_ACTIVE_TEXT = (
    "P0 semantic and wire authority",
    "P1 runtime-admission projection/profile",
    "P0 authoritative TypeSpec",
    "TypeSpec authority with checked JSON Schema and Protobuf projections",
    "TypeSpec is the semantic and wire authority",
    "There is one authority for shared RPC envelope semantics",
    "Change the shared semantic fact in TypeSpec first",
    "A shared-envelope change begins in TypeSpec",
)

REQUIRED_IN_EACH_ACTIVE_FILE = (
    "co-equal",
    "STOPPED_FOR_EVALUATION",
    "receipt",
)

REQUIRED_TOPOLOGY_TERMS = (
    "SQL_T",
    "SQL_J",
    "Protobuf/proto3",
    "gRPC",
    "HTTP/write clients",
)


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def command_output(root: Path, *args: str) -> str | None:
    try:
        completed = subprocess.run(
            args,
            cwd=root,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    value = completed.stdout.strip()
    return value or None


def fingerprint(path: str, line: int, rule: str, text: str) -> str:
    payload = f"{path}\0{line}\0{rule}\0{text}".encode("utf-8")
    return "schema-doc:" + hashlib.sha256(payload).hexdigest()[:24]


def finding(path: str, line: int, rule: str, message: str, text: str) -> dict[str, Any]:
    return {
        "fingerprint": fingerprint(path, line, rule, text),
        "severity": "error",
        "path": path,
        "line": max(1, line),
        "message": message,
        "status": "unexplained",
    }


def line_number(text: str, needle: str) -> int:
    offset = text.find(needle)
    if offset < 0:
        return 1
    return text.count("\n", 0, offset) + 1


def audit(root: Path) -> tuple[list[dict[str, Any]], list[dict[str, str]], list[str]]:
    findings: list[dict[str, Any]] = []
    inputs: list[dict[str, str]] = []
    scanned_files = [*ACTIVE_POLICY_FILES, ADR_FILE]
    content: dict[str, str] = {}

    for relative in scanned_files:
        path = root / relative
        if not path.is_file():
            findings.append(
                finding(
                    relative,
                    1,
                    "required-file",
                    f"Required policy file is missing: {relative}",
                    relative,
                )
            )
            continue
        raw = path.read_bytes()
        inputs.append(
            {
                "kind": "policy-document",
                "name": relative,
                "digest": sha256_bytes(raw),
            }
        )
        try:
            content[relative] = raw.decode("utf-8")
        except UnicodeDecodeError:
            findings.append(
                finding(
                    relative,
                    1,
                    "utf8",
                    "Policy document is not valid UTF-8.",
                    relative,
                )
            )

    for relative in ACTIVE_POLICY_FILES:
        text = content.get(relative)
        if text is None:
            continue
        for prohibited in PROHIBITED_ACTIVE_TEXT:
            if prohibited in text:
                findings.append(
                    finding(
                        relative,
                        line_number(text, prohibited),
                        "superseded-hierarchy",
                        f"Active guidance still contains superseded hierarchy text: {prohibited!r}",
                        prohibited,
                    )
                )
        for required in REQUIRED_IN_EACH_ACTIVE_FILE:
            if required not in text:
                findings.append(
                    finding(
                        relative,
                        1,
                        "required-invariant",
                        f"Active guidance must contain {required!r}.",
                        required,
                    )
                )

    combined_active = "\n".join(content.get(path, "") for path in ACTIVE_POLICY_FILES)
    for required in REQUIRED_TOPOLOGY_TERMS:
        if required not in combined_active:
            findings.append(
                finding(
                    "README.md",
                    1,
                    "required-topology",
                    f"The peer-lane topology is missing required term {required!r}.",
                    required,
                )
            )

    adr = content.get(ADR_FILE, "")
    adr_requirements = (
        "Status:** Accepted",
        "co-equal",
        "STOPPED_FOR_EVALUATION",
        "ores.schema-audit-receipt/v1",
        "A sweep without a receipt is incomplete",
    )
    for required in adr_requirements:
        if required not in adr:
            findings.append(
                finding(
                    ADR_FILE,
                    1,
                    "adr-contract",
                    f"ADR 0001 is missing required decision text {required!r}.",
                    required,
                )
            )

    return findings, inputs, scanned_files


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Repository root (default: inferred from this script).",
    )
    parser.add_argument(
        "--receipt",
        type=Path,
        required=True,
        help="Path to the machine-readable JSON receipt.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    started_at = utc_now()
    findings, inputs, scanned_files = audit(root)

    commit = command_output(root, "git", "rev-parse", "HEAD") or "unknown"
    branch = (
        os.environ.get("GITHUB_HEAD_REF")
        or os.environ.get("GITHUB_REF_NAME")
        or command_output(root, "git", "branch", "--show-current")
        or "unknown"
    )
    repository = os.environ.get("GITHUB_REPOSITORY", "ORESoftware/api-docs")
    actor = os.environ.get("GITHUB_ACTOR", os.environ.get("USER", "unknown"))
    run_id = os.environ.get("GITHUB_RUN_ID")
    run_url = (
        f"https://github.com/{repository}/actions/runs/{run_id}"
        if run_id
        else None
    )

    status = "stopped_for_evaluation" if findings else "passed"
    receipt_path = args.receipt.resolve()
    artifacts: list[dict[str, str]] = [
        {
            "kind": "schema-audit-receipt",
            "location": str(receipt_path),
        }
    ]
    if run_url:
        artifacts.append(
            {
                "kind": "github-actions-run",
                "location": run_url,
            }
        )

    receipt: dict[str, Any] = {
        "format": FORMAT,
        "status": status,
        "startedAt": started_at,
        "finishedAt": utc_now(),
        "actor": actor,
        "scope": {
            "organizations": [repository.split("/", 1)[0]],
            "repositories": [repository],
            "branches": [branch],
            "commits": [commit],
            "services": ["api-docs RPC contract policy"],
            "files": scanned_files,
            "linearRecords": ["DEN-3959", "DEN-3982"],
            "githubRecords": ["ORESoftware/my-ai#61"],
            "paginationOrCaps": [
                "fixed explicit policy-file set; no pagination"
            ],
            "exclusions": [
                "Repository code outside policy-bearing documentation was not scanned by this documentation sweep; semantic RPC IDL gates run separately."
            ],
            "inaccessibleOrReadOnly": [],
        },
        "inputs": inputs,
        "tools": [
            {
                "name": "schema-authority-doc-sweep.py",
                "version": SCRIPT_VERSION,
                "options": {
                    "root": str(root),
                    "policyFiles": scanned_files,
                },
            },
            {
                "name": "python",
                "version": platform.python_version(),
                "options": {},
            },
        ],
        "checks": [
            {
                "name": "required-policy-files",
                "state": "executed",
                "details": "Checked all explicitly declared policy-bearing files for existence and UTF-8 content.",
            },
            {
                "name": "superseded-hierarchy-text",
                "state": "executed",
                "details": "Checked active guidance for exact legacy TypeSpec-P0/JSON-Schema-P1 hierarchy claims.",
            },
            {
                "name": "peer-authority-topology",
                "state": "executed",
                "details": "Checked for co-equal authority, SQL_T/SQL_J, Protobuf/gRPC, HTTP/write-client, discrepancy-stop, and receipt requirements.",
            },
            {
                "name": "adr-contract",
                "state": "executed",
                "details": "Checked ADR 0001 acceptance, stop state, and receipt contract anchors.",
            },
        ],
        "artifacts": artifacts,
        "findings": findings,
        "discrepancyFingerprints": [item["fingerprint"] for item in findings],
        "zeroUnexplainedFindings": not findings,
    }

    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = receipt_path.with_suffix(receipt_path.suffix + ".tmp")
    temporary.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(receipt_path)
    print(json.dumps(receipt, indent=2, sort_keys=True))
    return 2 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
