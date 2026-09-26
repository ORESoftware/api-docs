#!/usr/bin/env python3
"""Cross-field admission rules for ORES server compatibility receipts."""

from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
from typing import Any

EXPECTED_STACK_REPOSITORY = "ores-stack/ores-stack-cli"
EXPECTED_COMPOSE_REPOSITORY = "ORESoftware/ores-compose"


def policy_errors(receipt: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    toolchain = receipt.get("toolchain", {})
    if toolchain.get("oresStack", {}).get("repository") != EXPECTED_STACK_REPOSITORY:
        errors.append("toolchain.oresStack.repository must be ores-stack/ores-stack-cli")
    if toolchain.get("oresCompose", {}).get("repository") != EXPECTED_COMPOSE_REPOSITORY:
        errors.append("toolchain.oresCompose.repository must be ORESoftware/ores-compose")

    checks = receipt.get("checks", [])
    if not isinstance(checks, list):
        return errors + ["checks must be an array"]

    executed = [check for check in checks if isinstance(check, dict) and check.get("executed") is True]
    declared_count = receipt.get("executedChecks")
    if declared_count != len(executed):
        errors.append("executedChecks must equal the number of checks with executed=true")

    for check in checks:
        if not isinstance(check, dict):
            continue
        if check.get("executed") is False and check.get("outcome") in {"passed", "failed"}:
            errors.append(f"unexecuted check {check.get('id', '<unknown>')} cannot be passed or failed")
        if check.get("executed") is True and check.get("outcome") == "not_run":
            errors.append(f"executed check {check.get('id', '<unknown>')} cannot be not_run")

    state = receipt.get("evidenceState")
    if state == "passed":
        if not executed:
            errors.append("passed receipts must contain at least one executed check")
        if any(check.get("outcome") != "passed" for check in checks if isinstance(check, dict)):
            errors.append("passed receipts may contain only passed checks")
    elif state == "not_run" and executed:
        errors.append("not_run receipts cannot contain executed checks")

    return errors


def self_test(valid_path: Path) -> list[str]:
    receipt = json.loads(valid_path.read_text(encoding="utf-8"))
    errors: list[str] = []

    if policy_errors(receipt):
        errors.append("checked-in valid receipt violates policy")

    zero_step = copy.deepcopy(receipt)
    zero_step["executedChecks"] = 0
    for check in zero_step["checks"]:
        check["executed"] = False
        check["outcome"] = "not_run"
    if "passed receipts must contain at least one executed check" not in policy_errors(zero_step):
        errors.append("zero-step passed receipt was not rejected")

    old_cli = copy.deepcopy(receipt)
    old_cli["toolchain"]["oresStack"]["repository"] = "ORESoftware/ores-stack"
    if not any("ores-stack/ores-stack-cli" in item for item in policy_errors(old_cli)):
        errors.append("legacy ores-stack repository was not rejected")

    count_mismatch = copy.deepcopy(receipt)
    count_mismatch["executedChecks"] = 1
    if not any("executedChecks" in item for item in policy_errors(count_mismatch)):
        errors.append("executed-check count mismatch was not rejected")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--receipt",
        type=Path,
        default=Path("contracts/server-compatibility-receipt/instances/ServerCompatibilityReceipt/valid/standalone.json"),
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        errors = self_test(args.receipt)
    else:
        errors = policy_errors(json.loads(args.receipt.read_text(encoding="utf-8")))

    if errors:
        for error in errors:
            print(f"ERROR {error}")
        return 1

    print("server compatibility receipt policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
