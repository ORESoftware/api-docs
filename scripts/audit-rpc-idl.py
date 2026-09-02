#!/usr/bin/env python3
"""Strict semantic audit layered over the RPC TypeSpec/JSON Schema/Proto cross-check.

The existing cross-check intentionally has a small, dependency-free parser. This
gate closes the dangerous gaps: missing constraints are errors, expected deltas
are an exact allow-list, every Proto field number is unique, and the field-number
ledger may neither omit nor invent a message.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_DELTA_IDS = {
    "proto-transport-unspecified",
    "proto-json-bytes",
    "v2-union-vs-if-then",
    "v2-unevaluated-properties",
    "additional-properties-closed",
    "v2-meta-map-vs-object",
    "v2-proto-flattened",
}


def _load_cross_check(root: Path):
    path = root / "scripts" / "cross-check-rpc-idl.py"
    spec = importlib.util.spec_from_file_location("rpc_idl_cross_check", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["rpc_idl_cross_check"] = module
    spec.loader.exec_module(module)
    return module


def _same_constraint(vetoes: list[str], left: Any, right: Any, label: str) -> None:
    if left != right:
        vetoes.append(f"{label}: JSON Schema={left!r} TypeSpec={right!r}")


def _resolved_typespec_kind(field: Any, enums: dict[str, list[str]]) -> tuple[str, list[str] | None]:
    if field.kind != "ref":
        return field.kind, field.enum
    # Only the three reviewed transport properties may project a named TypeSpec
    # reference into the shared transport enum. Raw source checks below bind
    # those properties to the exact Transport declaration rather than accepting
    # any arbitrary reference as enum-equivalent.
    if field.name not in {"transport", "rpc.transport"}:
        return "ref", None
    values = enums.get("Ores.Rpc.V1.Transport")
    if values is not None:
        return "enum", values
    return "ref", None


def _strict_shape(
    vetoes: list[str],
    schema_shape: Any,
    type_shape: Any,
    enums: dict[str, list[str]],
) -> None:
    schema_names = set(schema_shape.fields)
    type_names = set(type_shape.fields)
    if schema_names != type_names:
        vetoes.append(
            f"{type_shape.name}: fields {sorted(type_names)} != "
            f"JSON Schema {sorted(schema_names)}"
        )
        return
    for name in sorted(schema_names):
        schema_field = schema_shape.fields[name]
        type_field = type_shape.fields[name]
        type_kind, type_enum = _resolved_typespec_kind(type_field, enums)
        if schema_field.kind != type_kind:
            vetoes.append(
                f"{type_shape.name}.{name}.kind: "
                f"JSON Schema={schema_field.kind!r} TypeSpec={type_kind!r}"
            )
        _same_constraint(
            vetoes,
            schema_field.required,
            type_field.required,
            f"{type_shape.name}.{name}.required",
        )
        for attr in (
            "const",
            "min_length",
            "max_length",
            "pattern",
            "minimum",
            "maximum",
        ):
            _same_constraint(
                vetoes,
                getattr(schema_field, attr),
                getattr(type_field, attr),
                f"{type_shape.name}.{name}.{attr}",
            )
        schema_enum = sorted(schema_field.enum or [])
        resolved_enum = sorted(type_enum or [])
        if schema_enum != resolved_enum:
            vetoes.append(
                f"{type_shape.name}.{name}.enum: "
                f"JSON Schema={schema_enum!r} TypeSpec={resolved_enum!r}"
            )


def _audit_delta_allowlist(root: Path, vetoes: list[str]) -> None:
    doc = json.loads((root / "idl" / "expected-deltas.json").read_text(encoding="utf-8"))
    deltas = doc.get("deltas")
    if not isinstance(deltas, list):
        vetoes.append("expected-deltas.json: deltas must be an array")
        return
    ids: list[str] = []
    for index, delta in enumerate(deltas):
        if not isinstance(delta, dict):
            vetoes.append(f"expected-deltas.json[{index}]: expected object")
            continue
        delta_id = delta.get("id")
        reason = delta.get("reason")
        if not isinstance(delta_id, str) or not delta_id:
            vetoes.append(f"expected-deltas.json[{index}]: id required")
            continue
        ids.append(delta_id)
        if not isinstance(reason, str) or len(reason.strip()) < 24:
            vetoes.append(
                f"expected-deltas.json[{index}] {delta_id}: "
                "reviewable reason of at least 24 characters required"
            )
    duplicates = sorted({item for item in ids if ids.count(item) > 1})
    if duplicates:
        vetoes.append(f"expected-deltas.json: duplicate ids {duplicates}")
    actual = set(ids)
    if actual != EXPECTED_DELTA_IDS:
        vetoes.append(
            "expected-deltas.json ids "
            f"{sorted(actual)} != exact allow-list {sorted(EXPECTED_DELTA_IDS)}"
        )


def _audit_proto_ledger(root: Path, cross: Any, proto: dict[str, Any], vetoes: list[str]) -> None:
    lock = json.loads((root / "idl" / "protobuf.lock.json").read_text(encoding="utf-8"))
    messages = lock.get("messages")
    if not isinstance(messages, dict):
        vetoes.append("protobuf.lock.json: messages must be an object")
        return
    source_messages = {name for name, shape in proto.items() if shape.fields}
    locked_messages = set(messages)
    if source_messages != locked_messages:
        vetoes.append(
            f"protobuf ledger messages {sorted(locked_messages)} != "
            f"source messages {sorted(source_messages)}"
        )
    vetoes.extend(cross.check_protobuf_lock(proto, lock))
    for name, shape in proto.items():
        numbers = [
            field.proto_number
            for field in shape.fields.values()
            if field.proto_number is not None
        ]
        duplicate_numbers = sorted(
            {number for number in numbers if numbers.count(number) > 1}
        )
        if duplicate_numbers:
            vetoes.append(f"{name}: duplicate field numbers {duplicate_numbers}")
        if numbers and min(numbers) < 1:
            vetoes.append(f"{name}: field numbers must be positive")


def _audit_typespec_references(root: Path, vetoes: list[str]) -> None:
    v1 = (root / "idl" / "typespec" / "v1.tsp").read_text(encoding="utf-8")
    telemetry = (root / "idl" / "typespec" / "telemetry.tsp").read_text(encoding="utf-8")
    reviewed = (
        (v1, r"(?m)^\s*transport\?:\s*Transport;\s*$", "RpcCall/RpcReceipt transport"),
        (
            telemetry,
            r"(?m)^\s*`rpc\.transport`:\s*Ores\.Rpc\.V1\.Transport;\s*$",
            "telemetry transport",
        ),
    )
    # v1 has exactly two optional transport fields, one per model.
    if len(re.findall(reviewed[0][1], v1)) != 2:
        vetoes.append("TypeSpec v1 must contain exactly two optional Transport fields")
    if len(re.findall(reviewed[1][1], telemetry)) != 1:
        vetoes.append("TypeSpec telemetry must bind rpc.transport to Ores.Rpc.V1.Transport")


def _proto_message_assignments(text: str, package: str) -> dict[str, dict[str, int]]:
    messages: dict[str, dict[str, int]] = {}
    current: str | None = None
    depth = 0
    for raw in text.splitlines():
        line = raw.split("//", 1)[0].strip()
        if current is None:
            match = re.match(r"^message\s+(\w+)\s*\{", line)
            if match:
                current = f"{package}.{match.group(1)}"
                messages[current] = {}
                depth = line.count("{") - line.count("}")
            continue
        depth += line.count("{") - line.count("}")
        match = re.match(
            r"^(?:optional\s+|repeated\s+)?[^=;]+?\s+(\w+)\s*=\s*(\d+)"
            r"\s*(?:\[([^\]]+)\])?\s*;",
            line,
        )
        if match:
            name, number, options = match.group(1), int(match.group(2)), match.group(3)
            if options:
                json_name = re.search(r'json_name\s*=\s*"([^"]+)"', options)
                if json_name:
                    name = json_name.group(1)
            messages[current][name] = number
        if depth <= 0:
            current = None
    return messages


def _proto_enum_assignments(text: str, package: str) -> dict[str, dict[str, int]]:
    enums: dict[str, dict[str, int]] = {}
    current: str | None = None
    depth = 0
    for raw in text.splitlines():
        line = raw.split("//", 1)[0].strip()
        if current is None:
            match = re.match(r"^enum\s+(\w+)\s*\{", line)
            if match:
                current = f"{package}.{match.group(1)}"
                enums[current] = {}
                depth = line.count("{") - line.count("}")
            continue
        depth += line.count("{") - line.count("}")
        match = re.match(r"^(\w+)\s*=\s*(\d+)\s*;", line)
        if match:
            enums[current][match.group(1)] = int(match.group(2))
        if depth <= 0:
            current = None
    return enums


def _audit_proto_source_coverage(root: Path, proto: dict[str, Any], vetoes: list[str]) -> None:
    source_messages: dict[str, dict[str, int]] = {}
    source_enums: dict[str, dict[str, int]] = {}
    for path in sorted((root / "idl" / "protobuf").rglob("*.proto")):
        text = path.read_text(encoding="utf-8")
        package_match = re.search(r"(?m)^package\s+([\w.]+);", text)
        package = package_match.group(1) if package_match else ""
        source_messages.update(_proto_message_assignments(text, package))
        source_enums.update(_proto_enum_assignments(text, package))
    for name, assignments in source_messages.items():
        parsed = proto.get(name)
        if parsed is None:
            vetoes.append(f"{name}: protobuf source message was not parsed")
            continue
        parsed_assignments = {
            field.name: field.proto_number
            for field in parsed.fields.values()
            if field.proto_number is not None
        }
        if assignments != parsed_assignments:
            vetoes.append(
                f"{name}: protobuf parser coverage {parsed_assignments} != source {assignments}"
            )

    lock = json.loads((root / "idl" / "protobuf.lock.json").read_text(encoding="utf-8"))
    locked_enums = lock.get("enums")
    if not isinstance(locked_enums, dict):
        vetoes.append("protobuf.lock.json: enums must be an object")
    elif source_enums != locked_enums:
        vetoes.append(
            f"protobuf ledger enums {locked_enums} != source enums {source_enums}"
        )


def run(root: Path | None = None) -> dict[str, Any]:
    root = root or ROOT
    cross = _load_cross_check(root)
    base_report = cross.run(root)
    vetoes = list(base_report["vetoes"])

    schemas = cross.load_json_shapes(root)
    typespec = cross.load_all_typespec(root)
    proto = cross.load_all_proto(root)
    enums = {
        name: list(shape.enum_values)
        for name, shape in typespec.items()
        if shape.enum_values is not None
    }

    strict_pairs = (
        ("rpc-call", "Ores.Rpc.V1.RpcCall"),
        ("rpc-receipt", "Ores.Rpc.V1.RpcReceipt"),
        ("telemetry-attributes", "Ores.Rpc.Telemetry.TelemetryAttributes"),
    )
    for schema_name, type_name in strict_pairs:
        schema_shape = schemas.get(schema_name)
        type_shape = typespec.get(type_name)
        if schema_shape is None or type_shape is None:
            vetoes.append(f"missing strict pair {schema_name} / {type_name}")
            continue
        _strict_shape(vetoes, schema_shape, type_shape, enums)

    _audit_delta_allowlist(root, vetoes)
    _audit_proto_ledger(root, cross, proto, vetoes)
    _audit_proto_source_coverage(root, proto, vetoes)
    _audit_typespec_references(root, vetoes)

    # Exact declaration set prevents a new frame model from bypassing review.
    expected_typespec = {
        "Ores.Rpc.Telemetry.TelemetryAttributes",
        "Ores.Rpc.V1.RpcCall",
        "Ores.Rpc.V1.RpcReceipt",
        "Ores.Rpc.V1.Transport",
        "Ores.Rpc.V2.RpcCallFrame",
        "Ores.Rpc.V2.RpcCancelFrame",
        "Ores.Rpc.V2.RpcDataFrame",
        "Ores.Rpc.V2.RpcEndFrame",
        "Ores.Rpc.V2.RpcErrorFrame",
        "Ores.Rpc.V2.RpcFrame",
    }
    actual_typespec = set(typespec)
    if actual_typespec != expected_typespec:
        vetoes.append(
            f"TypeSpec declaration set {sorted(actual_typespec)} != "
            f"reviewed set {sorted(expected_typespec)}"
        )

    unique_vetoes = sorted(set(vetoes))
    return {
        "ok": not unique_vetoes,
        "vetoes": unique_vetoes,
        "baseNotes": base_report["notes"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    report = run(args.root)
    print(json.dumps(report, indent=2, sort_keys=True))
    if report["vetoes"]:
        print("strict rpc idl admission veto", file=sys.stderr)
        for veto in report["vetoes"]:
            print(f"  {veto}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
