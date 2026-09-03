#!/usr/bin/env python3
"""Audit durable guidance for the peer-authority invariant and emit a receipt."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

FORMAT = "ores.schema-audit-receipt/v1"
SCRIPT_VERSION = "2.0.0"
ACTIVE_POLICY_FILES = (
    "README.md",
    "AGENTS.md",
    "idl/README.md",
    "docs/rpc-contract-coupling.md",
)
ADR_FILE = "docs/adr/0001-peer-schema-authority.md"
SCANNED_FILES = (*ACTIVE_POLICY_FILES, ADR_FILE)

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

REQUIRED_COMBINED_TERMS = (
    "typespec",
    "json schema",
    "peer",
    "human-authored",
    "sql",
    "protobuf",
    "grpc",
    "wire clients",
    "interfaces",
    "write clients",
    "diesel",
    "seaorm",
)

ADR_REQUIREMENTS = (
    "**Status:** Accepted",
    "co-equal",
    "STOPPED_FOR_EVALUATION",
    "ores.schema-audit-receipt/v1",
    "A sweep without a receipt is incomplete",
)


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def line_number(text: str, needle: str) -> int:
    offset = text.find(needle)
    return 1 if offset < 0 else text.count("\n", 0, offset) + 1


def fingerprint(path: str, line: int, rule: str, text: str) -> str:
    value = f"{path}\0{line}\0{rule}\0{text}".encode()
    return "schema-doc:" + hashlib.sha256(value).hexdigest()[:24]


def finding(path: str, line: int, rule: str, message: str, text: str) -> dict[str, Any]:
    return {
        "fingerprint": fingerprint(path, line, rule, text),
        "severity": "error",
        "path": path,
        "line": max(1, line),
        "message": message,
        "status": "unexplained",
    }


def git_output(root: Path, *args: str) -> str | None:
    try:
        completed = subprocess.run(
            ["git", *args],
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


def append_unique(values: list[str], *candidates: Any) -> None:
    for candidate in candidates:
        if isinstance(candidate, str):
            value = candidate.strip()
            if value and value not in values:
                values.append(value)


def event_scope() -> tuple[list[str], list[str]]:
    branches: list[str] = []
    commits: list[str] = []
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        return branches, commits
    try:
        event = json.loads(Path(event_path).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return branches, commits

    pull_request = event.get("pull_request")
    if isinstance(pull_request, dict):
        for side_name in ("head", "base"):
            side = pull_request.get(side_name)
            if isinstance(side, dict):
                append_unique(branches, side.get("ref"))
                append_unique(commits, side.get("sha"))

    event_ref = event.get("ref")
    if isinstance(event_ref, str):
        for prefix in ("refs/heads/", "refs/tags/"):
            if event_ref.startswith(prefix):
                event_ref = event_ref[len(prefix):]
                break
        append_unique(branches, event_ref)
    append_unique(commits, event.get("before"), event.get("after"))
    return branches, commits


def audit(root: Path) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    findings: list[dict[str, Any]] = []
    inputs: list[dict[str, str]] = []
    content: dict[str, str] = {}

    for relative in SCANNED_FILES:
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
            {"kind": "policy-document", "name": relative, "digest": sha256_bytes(raw)}
        )
        try:
            content[relative] = raw.decode("utf-8")
        except UnicodeDecodeError:
            findings.append(
                finding(relative, 1, "utf8", "Policy document is not UTF-8.", relative)
            )

    for relative in ACTIVE_POLICY_FILES:
        text = content.get(relative, "")
        for prohibited in PROHIBITED_ACTIVE_TEXT:
            if prohibited in text:
                findings.append(
                    finding(
                        relative,
                        line_number(text, prohibited),
                        "superseded-hierarchy",
                        f"Active guidance contains superseded hierarchy text: {prohibited!r}",
                        prohibited,
                    )
                )

    combined = "\n".join(content.get(path, "") for path in SCANNED_FILES).lower()
    for required in REQUIRED_COMBINED_TERMS:
        if required not in combined:
            findings.append(
                finding(
                    ADR_FILE,
                    1,
                    "required-topology",
                    f"Durable guidance is missing peer-authority concept {required!r}.",
                    required,
                )
            )

    if "halt and evaluate" not in combined and "stopped_for_evaluation" not in combined:
        findings.append(
            finding(
                ADR_FILE,
                1,
                "discrepancy-stop",
                "Durable guidance must define halt-and-evaluate/STOPPED_FOR_EVALUATION behavior.",
                "STOPPED_FOR_EVALUATION",
            )
        )

    adr = content.get(ADR_FILE, "")
    for required in ADR_REQUIREMENTS:
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

    return findings, inputs


def make_receipt(root: Path, receipt_path: Path) -> dict[str, Any]:
    started_at = utc_now()
    findings, inputs = audit(root)
    event_branches, event_commits = event_scope()

    branches: list[str] = []
    append_unique(
        branches,
        os.environ.get("GITHUB_HEAD_REF"),
        os.environ.get("GITHUB_BASE_REF"),
        *event_branches,
        os.environ.get("GITHUB_REF_NAME"),
        git_output(root, "branch", "--show-current"),
    )
    if not branches:
        branches.append("unknown")

    commits: list[str] = []
    append_unique(
        commits,
        git_output(root, "rev-parse", "HEAD"),
        os.environ.get("GITHUB_SHA"),
        *event_commits,
    )
    if not commits:
        commits.append("unknown")

    repository = os.environ.get("GITHUB_REPOSITORY", "ORESoftware/api-docs")
    run_id = os.environ.get("GITHUB_RUN_ID")
    artifacts = [{"kind": "schema-audit-receipt", "location": str(receipt_path)}]
    if run_id:
        artifacts.append(
            {
                "kind": "github-actions-run",
                "location": f"https://github.com/{repository}/actions/runs/{run_id}",
            }
        )

    return {
        "format": FORMAT,
        "status": "stopped_for_evaluation" if findings else "passed",
        "startedAt": started_at,
        "finishedAt": utc_now(),
        "actor": os.environ.get("GITHUB_ACTOR", os.environ.get("USER", "unknown")),
        "scope": {
            "organizations": [repository.split("/", 1)[0]],
            "repositories": [repository],
            "branches": branches,
            "commits": commits,
            "services": ["api-docs peer-authority policy"],
            "files": list(SCANNED_FILES),
            "linearRecords": ["DEN-3959", "DEN-3982", "DEN-3321"],
            "githubRecords": ["ORESoftware/api-docs#19", "ORESoftware/api-docs#20"],
            "paginationOrCaps": ["fixed explicit policy-file set; no pagination"],
            "exclusions": [
                "Semantic contract, SQL, client-type, Diesel, and SeaORM checks execute in their separate repository gates."
            ],
            "inaccessibleOrReadOnly": [],
        },
        "inputs": inputs,
        "tools": [
            {
                "name": "schema-authority-doc-sweep.py",
                "version": SCRIPT_VERSION,
                "options": {"root": str(root), "policyFiles": list(SCANNED_FILES)},
            },
            {"name": "python", "version": platform.python_version(), "options": {}},
        ],
        "checks": [
            {
                "name": "required-policy-files",
                "state": "executed",
                "details": "Checked the explicit durable-guidance set for existence and UTF-8.",
            },
            {
                "name": "superseded-hierarchy-text",
                "state": "executed",
                "details": "Rejected active P0/P1 and one-way authority claims while allowing marked history in ADR 0001.",
            },
            {
                "name": "peer-authority-topology",
                "state": "executed",
                "details": "Required both authored lanes, downstream SQL/client paths, Diesel/SeaORM comparison, and discrepancy-stop behavior.",
            },
            {
                "name": "receipt-contract",
                "state": "executed",
                "details": "Required ADR 0001 to define ores.schema-audit-receipt/v1 and receipt-before-coverage semantics.",
            },
        ],
        "artifacts": artifacts,
        "findings": findings,
        "discrepancyFingerprints": [item["fingerprint"] for item in findings],
        "zeroUnexplainedFindings": not findings,
    }


def write_receipt(path: Path, receipt: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    temporary.replace(path)


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for relative in SCANNED_FILES:
            (root / relative).parent.mkdir(parents=True, exist_ok=True)
            (root / relative).write_text("peer human-authored guidance\n", encoding="utf-8")
        (root / "README.md").write_text(
            "TypeSpec and JSON Schema/OpenAPI are peer human-authored sources. "
            "TypeSpec emits SQL, Protobuf, gRPC, and wire clients. "
            "JSON Schema emits interfaces, SQL, and write clients. "
            "Diesel and SeaORM halt and evaluate differences.\n",
            encoding="utf-8",
        )
        (root / ADR_FILE).write_text(
            "**Status:** Accepted\nco-equal\nSTOPPED_FOR_EVALUATION\n"
            "ores.schema-audit-receipt/v1\nA sweep without a receipt is incomplete\n",
            encoding="utf-8",
        )
        findings, _ = audit(root)
        if findings:
            raise AssertionError(findings)

        (root / "README.md").write_text(
            "TypeSpec is the semantic and wire authority\n", encoding="utf-8"
        )
        findings, _ = audit(root)
        if not any(item["message"].startswith("Active guidance") for item in findings):
            raise AssertionError("legacy hierarchy was not rejected")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        print("schema-authority-doc-sweep self-test: ok")
        return 0
    if args.receipt is None:
        raise SystemExit("--receipt is required unless --self-test is used")
    root = args.root.resolve()
    receipt_path = args.receipt.resolve()
    receipt = make_receipt(root, receipt_path)
    write_receipt(receipt_path, receipt)
    print(json.dumps(receipt, indent=2, sort_keys=True))
    return 2 if receipt["findings"] else 0


if __name__ == "__main__":
    sys.exit(main())
