#!/usr/bin/env python3
"""Audit the RPC v1 receipt state across peer authorities and projections."""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_DELTA_IDS = {
    "v1-receipt-typespec-alias-vs-json-schema-conditional",
    "v1-receipt-protobuf-flattened",
}
EXPECTED_PROTO_FIELDS = {
    "v": 1,
    "op": 2,
    "id": 3,
    "key": 4,
    "transport": 5,
    "ok": 6,
    "status": 7,
    "body": 8,
    "error": 9,
    "traceId": 10,
    "spanId": 11,
}
EXPECTED_INVALID_CASES = {
    "success-with-error",
    "success-error-status",
    "failure-without-error",
    "failure-with-body",
    "failure-success-status",
    "error-not-object",
    "ridl-v2-presented-as-v1-receipt",
}


def _read(root: Path, relative: str) -> str:
    return (root / relative).read_text(encoding="utf-8")


def _load(root: Path, relative: str) -> dict[str, Any]:
    value = json.loads(_read(root, relative))
    if not isinstance(value, dict):
        raise ValueError(f"{relative} must contain an object")
    return value


def _alias_body(text: str, name: str) -> str | None:
    match = re.search(
        rf"(?s)\balias\s+{re.escape(name)}\s*=\s*\{{(.*?)\}}\s*;",
        text,
    )
    return None if match is None else match.group(1)


def _field_names(body: str) -> set[str]:
    return {
        match.group(1).strip("`")
        for match in re.finditer(
            r"(?m)^\s*(`[^`]+`|[A-Za-z_][A-Za-z0-9_]*)(?:\?)?\s*:",
            body,
        )
    }


def _check_alias(
    vetoes: list[str],
    text: str,
    *,
    name: str,
    ok_literal: str,
    status_min: int,
    status_max: int,
    expected_fields: set[str],
) -> None:
    body = _alias_body(text, name)
    if body is None:
        vetoes.append(f"TypeSpec missing {name} model-expression alias")
        return
    actual_fields = _field_names(body)
    if actual_fields != expected_fields:
        vetoes.append(
            f"TypeSpec {name} fields {sorted(actual_fields)} != "
            f"{sorted(expected_fields)}"
        )
    if re.search(rf"(?m)^\s*ok:\s*{ok_literal};\s*$", body) is None:
        vetoes.append(f"TypeSpec {name}.ok must be {ok_literal}")
    status_pattern = (
        rf"(?s)@minValue\({status_min}\)\s*"
        rf"@maxValue\({status_max}\)\s*status\?:\s*int32;"
    )
    if re.search(status_pattern, body) is None:
        vetoes.append(
            f"TypeSpec {name}.status must be optional {status_min}..{status_max}"
        )
    for field, maximum in (("id", 128), ("traceId", 64), ("spanId", 32)):
        if field not in expected_fields:
            continue
        pattern = rf"(?s)@minLength\(1\)\s*@maxLength\({maximum}\)\s*{field}\??:\s*string;"
        if re.search(pattern, body) is None:
            vetoes.append(f"TypeSpec {name}.{field} length constraint drift")
    if re.search(
        r'(?s)@minLength\(1\)\s*@pattern\("\^\[A-Za-z\]\[A-Za-z0-9_\]\*\$"\)\s*key:\s*string;',
        body,
    ) is None:
        vetoes.append(f"TypeSpec {name}.key constraint drift")
    if re.search(
        r"(?m)^\s*transport\?:\s*Ores\.Rpc\.V1\.Transport;\s*$", body
    ) is None:
        vetoes.append(f"TypeSpec {name}.transport reference drift")


def _proto_receipt_fields(text: str) -> dict[str, int]:
    message = re.search(r"(?s)\bmessage\s+RpcReceipt\s*\{(.*?)\}", text)
    if message is None:
        return {}
    fields: dict[str, int] = {}
    for raw_name, number, options in re.findall(
        r"(?m)^\s*(?:optional\s+|repeated\s+)?[^=;]+?\s+"
        r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(\d+)\s*"
        r"(?:\[([^\]]+)\])?\s*;",
        message.group(1),
    ):
        name = raw_name
        json_name = re.search(r'json_name\s*=\s*"([^"]+)"', options or "")
        if json_name is not None:
            name = json_name.group(1)
        fields[name] = int(number)
    return fields


def run(root: Path | None = None) -> dict[str, Any]:
    root = root or ROOT
    vetoes: list[str] = []

    try:
        typespec = _read(root, "idl/typespec/v1.tsp")
        schema = _load(root, "json-schema/rpc-receipt.schema.json")
        proto = _read(root, "idl/protobuf/ores/rpc/v1/rpc.proto")
        lock = _load(root, "idl/protobuf.lock.json")
        deltas = _load(root, "idl/rpc-v1-receipt-state-deltas.json")
        manifest = _load(root, "runtime/v1-conformance.json")
        corpus = _load(root, "examples/rpc-v1/conformance.json")
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return {"ok": False, "vetoes": [str(error)]}

    if "Structural comparison witness" not in typespec:
        vetoes.append("TypeSpec RpcReceipt structural-witness boundary missing")
    if re.search(
        r"(?m)^\s*alias\s+RpcReceiptState\s*=\s*"
        r"RpcSuccessReceipt\s*\|\s*RpcErrorReceipt\s*;\s*$",
        typespec,
    ) is None:
        vetoes.append("TypeSpec RpcReceiptState must be the exact success/error union")

    common = {"v", "op", "id", "key", "transport", "ok", "status", "traceId", "spanId"}
    _check_alias(
        vetoes,
        typespec,
        name="RpcSuccessReceipt",
        ok_literal="true",
        status_min=200,
        status_max=399,
        expected_fields=common | {"body"},
    )
    _check_alias(
        vetoes,
        typespec,
        name="RpcErrorReceipt",
        ok_literal="false",
        status_min=400,
        status_max=599,
        expected_fields=common | {"error"},
    )
    error_body = _alias_body(typespec, "RpcErrorReceipt") or ""
    if re.search(r"(?m)^\s*error:\s*Record<unknown>;\s*$", error_body) is None:
        vetoes.append("TypeSpec RpcErrorReceipt.error must be required object data")

    if schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
        vetoes.append("rpc-receipt must remain Draft 2020-12")
    if schema.get("additionalProperties") is not False:
        vetoes.append("rpc-receipt must remain closed")
    required = schema.get("required")
    if required != ["v", "op", "id", "key", "ok"]:
        vetoes.append("rpc-receipt structural required-field set drift")
    properties = schema.get("properties") if isinstance(schema.get("properties"), dict) else {}
    status = properties.get("status") if isinstance(properties.get("status"), dict) else {}
    if status.get("minimum") != 100 or status.get("maximum") != 599:
        vetoes.append("rpc-receipt structural status range must remain 100..599")
    if "status" in set(required or []):
        vetoes.append("rpc-receipt status must remain optional-compatible")
    if properties.get("error") != {"type": "object"}:
        vetoes.append("rpc-receipt error must remain a JSON object")

    all_of = schema.get("allOf")
    if not isinstance(all_of, list) or len(all_of) != 1 or not isinstance(all_of[0], dict):
        vetoes.append("rpc-receipt must contain one reviewed if/then/else state rule")
    else:
        rule = all_of[0]
        if rule.get("if") != {
            "properties": {"ok": {"const": True}},
            "required": ["ok"],
        }:
            vetoes.append("rpc-receipt success discriminator drift")
        if rule.get("then") != {
            "properties": {"status": {"minimum": 200, "maximum": 399}},
            "not": {"required": ["error"]},
        }:
            vetoes.append("rpc-receipt success branch drift")
        if rule.get("else") != {
            "required": ["error"],
            "properties": {"status": {"minimum": 400, "maximum": 599}},
            "not": {"required": ["body"]},
        }:
            vetoes.append("rpc-receipt error branch drift")

    proto_fields = _proto_receipt_fields(proto)
    if proto_fields != EXPECTED_PROTO_FIELDS:
        vetoes.append(
            f"Protobuf RpcReceipt fields {proto_fields} != {EXPECTED_PROTO_FIELDS}"
        )
    locked = (lock.get("messages") or {}).get("ores.rpc.v1.RpcReceipt")
    locked_fields = locked.get("fields") if isinstance(locked, dict) else None
    if locked_fields != EXPECTED_PROTO_FIELDS:
        vetoes.append("Protobuf RpcReceipt lock drift")
    for marker in (
        "Flattened transport projection",
        "RpcReceiptState",
        "ok=true",
        "ok=false",
        "shared semantic validator",
    ):
        if marker not in proto:
            vetoes.append(f"Protobuf RpcReceipt projection marker missing: {marker}")

    items = deltas.get("deltas")
    if not isinstance(items, list):
        vetoes.append("receipt-state deltas must be an array")
    else:
        ids = [item.get("id") for item in items if isinstance(item, dict)]
        if set(ids) != EXPECTED_DELTA_IDS or len(ids) != len(EXPECTED_DELTA_IDS):
            vetoes.append("receipt-state delta ledger must be the exact reviewed set")
        for item in items:
            if not isinstance(item, dict):
                vetoes.append("receipt-state delta entry must be an object")
                continue
            reason = item.get("reason")
            if not isinstance(reason, str) or len(reason.strip()) < 48:
                vetoes.append(f"receipt-state delta {item.get('id')} needs a reviewable reason")
    if deltas.get("scope") != "ores.rpc.v1.receipt-state":
        vetoes.append("receipt-state delta ledger scope drift")

    if manifest.get("statusPresence") != "optional-compatible":
        vetoes.append("runtime manifest status-presence drift")
    invalid = corpus.get("invalid")
    invalid_names = {
        item.get("name") for item in invalid or [] if isinstance(item, dict)
    }
    missing_cases = EXPECTED_INVALID_CASES - invalid_names
    if missing_cases:
        vetoes.append(f"receipt corpus missing negative cases {sorted(missing_cases)}")

    unique_vetoes = sorted(set(vetoes))
    return {"ok": not unique_vetoes, "vetoes": unique_vetoes}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    report = run(args.root)
    print(json.dumps(report, indent=2, sort_keys=True))
    if report["vetoes"]:
        print("RPC v1 receipt-state admission veto", file=sys.stderr)
        for veto in report["vetoes"]:
            print(f"  {veto}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
