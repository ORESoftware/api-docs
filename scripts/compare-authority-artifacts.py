#!/usr/bin/env python3
"""Compare generated SQL/type/ORM manifests and halt on any discrepancy."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

REQUIRED_ARTIFACT_KEYS = {"sql", "clientTypes"}


def _walk(left: Any, right: Any, pointer: str, differences: list[dict[str, Any]]) -> None:
    if type(left) is not type(right):
        differences.append(
            {
                "path": pointer or "/",
                "kind": "type_mismatch",
                "left": type(left).__name__,
                "right": type(right).__name__,
            }
        )
        return
    if isinstance(left, dict):
        left_keys = set(left)
        right_keys = set(right)
        for key in sorted(left_keys - right_keys):
            differences.append(
                {"path": f"{pointer}/{key}", "kind": "missing_right", "left": left[key]}
            )
        for key in sorted(right_keys - left_keys):
            differences.append(
                {"path": f"{pointer}/{key}", "kind": "missing_left", "right": right[key]}
            )
        for key in sorted(left_keys & right_keys):
            _walk(left[key], right[key], f"{pointer}/{key}", differences)
        return
    if isinstance(left, list):
        if len(left) != len(right):
            differences.append(
                {
                    "path": pointer or "/",
                    "kind": "length_mismatch",
                    "left": len(left),
                    "right": len(right),
                }
            )
        for index, (left_item, right_item) in enumerate(zip(left, right, strict=False)):
            _walk(left_item, right_item, f"{pointer}/{index}", differences)
        return
    if left != right:
        differences.append(
            {"path": pointer or "/", "kind": "value_mismatch", "left": left, "right": right}
        )


def compare_manifests(
    left: Any,
    right: Any,
    *,
    left_label: str,
    right_label: str,
) -> dict[str, Any]:
    errors: list[str] = []
    for label, document in ((left_label, left), (right_label, right)):
        if not isinstance(document, dict):
            errors.append(f"{label}: manifest must be an object")
            continue
        if document.get("schemaVersion") != 1:
            errors.append(f"{label}: schemaVersion must equal 1")
        if document.get("authority") != label:
            errors.append(f"{label}: authority field must equal the supplied label")
        artifacts = document.get("artifacts")
        if not isinstance(artifacts, dict):
            errors.append(f"{label}: artifacts must be an object")
        elif not REQUIRED_ARTIFACT_KEYS <= set(artifacts):
            errors.append(
                f"{label}: artifacts must include {sorted(REQUIRED_ARTIFACT_KEYS)}"
            )

    differences: list[dict[str, Any]] = []
    if not errors:
        _walk(left["artifacts"], right["artifacts"], "/artifacts", differences)
    ok = not errors and not differences
    return {
        "schemaVersion": 1,
        "ok": ok,
        "decision": "continue" if ok else "halt_and_evaluate",
        "left": left_label,
        "right": right_label,
        "errors": errors,
        "differences": differences,
    }


def _load(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--left", required=True, type=Path)
    parser.add_argument("--right", required=True, type=Path)
    parser.add_argument("--left-label", required=True)
    parser.add_argument("--right-label", required=True)
    parser.add_argument("--write-report", type=Path)
    args = parser.parse_args(argv)
    try:
        left = _load(args.left)
        right = _load(args.right)
    except (OSError, json.JSONDecodeError) as error:
        sys.stderr.write(f"unable to load generated authority artifacts: {error}\n")
        return 66

    report = compare_manifests(
        left,
        right,
        left_label=args.left_label,
        right_label=args.right_label,
    )
    text = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.write_report:
        args.write_report.parent.mkdir(parents=True, exist_ok=True)
        args.write_report.write_text(text, encoding="utf-8")
    sys.stdout.write(text)
    if not report["ok"]:
        sys.stderr.write(
            f"{args.left_label} vs {args.right_label}: discrepancy detected; halt and evaluate\n"
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
