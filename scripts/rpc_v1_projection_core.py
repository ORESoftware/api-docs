#!/usr/bin/env python3
"""Generate parity-gated RPC v1 SQL, Protobuf, and gRPC artifacts."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DELTA_IDS = {
    "protobuf-json-containers",
    "rpc-call-headers-property-names",
    "rpc-receipt-conditional-state",
}
PROTO_MAX = 536_870_911
FIELD_RE = re.compile(r"^(`[^`]+`|[A-Za-z_][A-Za-z0-9_]*)(\?)?\s*:\s*(.+);$")
DECORATOR_RE = re.compile(r"^@([A-Za-z_][A-Za-z0-9_]*)(?:\((.*)\))?$")
SQL_NAME_RE = re.compile(r"^[a-z][a-z0-9_]*$")


class ProjectionError(RuntimeError):
    pass


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ProjectionError(f"cannot load {path}: {error}") from error
    if not isinstance(value, dict):
        raise ProjectionError(f"{path} must contain an object")
    return value


def strip_comment(line: str) -> str:
    return line.split("//", 1)[0].strip()


def braced_body(text: str, kind: str, name: str) -> list[str]:
    lines = text.splitlines()
    start = next(
        (
            index
            for index, line in enumerate(lines)
            if re.match(rf"^\s*{kind}\s+{re.escape(name)}\s*\{{", strip_comment(line))
        ),
        None,
    )
    if start is None:
        raise ProjectionError(f"TypeSpec {kind} missing: {name}")
    depth = lines[start].count("{") - lines[start].count("}")
    body: list[str] = []
    for line in lines[start + 1 :]:
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            break
        body.append(line)
    if depth != 0:
        raise ProjectionError(f"unclosed TypeSpec {kind}: {name}")
    return body


def parse_decorator(raw: str | None) -> Any:
    if raw is None:
        return True
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw


def tsp_kind(raw: str) -> tuple[str, Any, str | None]:
    raw = raw.strip()
    if raw.startswith('"') and raw.endswith('"'):
        return "string", json.loads(raw), None
    if re.fullmatch(r"-?\d+", raw):
        return "integer", int(raw), None
    if raw in {"int32", "int64", "uint32", "uint64", "integer"}:
        return "integer", None, None
    if raw in {"float32", "float64", "decimal", "numeric"}:
        return "number", None, None
    if raw in {"string", "boolean"}:
        return raw, None, None
    if raw == "unknown":
        return "any", None, None
    if raw.startswith("Record<"):
        return "object", None, None
    return "ref", None, raw.split(".")[-1]


def parse_tsp_model(text: str, name: str) -> dict[str, dict[str, Any]]:
    output: dict[str, dict[str, Any]] = {}
    decorators: dict[str, Any] = {}
    for raw in braced_body(text, "model", name):
        line = strip_comment(raw)
        if not line:
            continue
        decorator = DECORATOR_RE.fullmatch(line)
        if decorator:
            decorators[decorator.group(1)] = parse_decorator(decorator.group(2))
            continue
        field = FIELD_RE.fullmatch(line)
        if not field:
            decorators.clear()
            continue
        field_name = field.group(1).strip("`")
        kind, const, reference = tsp_kind(field.group(3))
        output[field_name] = {
            "required": field.group(2) != "?",
            "kind": kind,
            "const": const,
            "reference": reference,
            "minLength": decorators.get("minLength"),
            "maxLength": decorators.get("maxLength"),
            "pattern": decorators.get("pattern"),
            "minimum": decorators.get("minValue"),
            "maximum": decorators.get("maxValue"),
        }
        decorators.clear()
    return output


def parse_tsp_enum(text: str, name: str) -> list[str]:
    values = []
    for raw in braced_body(text, "enum", name):
        line = strip_comment(raw).rstrip(",")
        if line:
            values.append(line.split(":", 1)[0].strip())
    return values


def schema_kind(node: Any) -> tuple[str, Any]:
    if node is True:
        return "any", None
    if not isinstance(node, dict):
        raise ProjectionError("JSON Schema properties must be objects or true")
    if "const" in node:
        value = node["const"]
        if isinstance(value, bool):
            return "boolean", value
        if isinstance(value, int) and not isinstance(value, bool):
            return "integer", value
        if isinstance(value, float):
            return "number", value
        if isinstance(value, str):
            return "string", value
        return "any", value
    kind = node.get("type")
    if kind in {"string", "integer", "number", "boolean", "object"}:
        return kind, None
    raise ProjectionError(f"unsupported JSON Schema type: {kind!r}")


def parse_schema(document: dict[str, Any]) -> dict[str, dict[str, Any]]:
    if document.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
        raise ProjectionError("JSON Schema must declare Draft 2020-12")
    if document.get("type") != "object" or document.get("additionalProperties") is not False:
        raise ProjectionError("JSON Schema authority must remain a closed object")
    properties = document.get("properties")
    required = document.get("required", [])
    if not isinstance(properties, dict) or not isinstance(required, list):
        raise ProjectionError("JSON Schema properties/required shape is invalid")
    if len(required) != len(set(required)) or not set(required).issubset(properties):
        raise ProjectionError("JSON Schema required inventory is invalid")
    output = {}
    for name, node in properties.items():
        kind, const = schema_kind(node)
        output[name] = {
            "required": name in required,
            "kind": kind,
            "const": const,
            "enum": tuple(node.get("enum", ())) if isinstance(node, dict) else (),
            "minLength": node.get("minLength") if isinstance(node, dict) else None,
            "maxLength": node.get("maxLength") if isinstance(node, dict) else None,
            "pattern": node.get("pattern") if isinstance(node, dict) else None,
            "minimum": node.get("minimum") if isinstance(node, dict) else None,
            "maximum": node.get("maximum") if isinstance(node, dict) else None,
        }
    return output


def compare_authorities(
    model: str,
    tsp_fields: dict[str, dict[str, Any]],
    schema_fields: dict[str, dict[str, Any]],
    enums: dict[str, list[str]],
) -> None:
    if set(tsp_fields) != set(schema_fields):
        raise ProjectionError(
            f"{model} authority field inventory drift: "
            f"missing-from-json={sorted(set(tsp_fields) - set(schema_fields))}, "
            f"extra-in-json={sorted(set(schema_fields) - set(tsp_fields))}"
        )
    for name in sorted(tsp_fields):
        left, right = tsp_fields[name], schema_fields[name]
        if left["required"] != right["required"]:
            raise ProjectionError(f"{model}.{name} authority requiredness drift")
        if left["kind"] == "ref":
            values = tuple(enums.get(left["reference"], ()))
            if right["kind"] != "string" or values != right["enum"]:
                raise ProjectionError(f"{model}.{name} authority enum/reference drift")
        elif left["kind"] != right["kind"]:
            raise ProjectionError(f"{model}.{name} authority kind drift")
        for key in (
            "const",
            "minLength",
            "maxLength",
            "pattern",
            "minimum",
            "maximum",
        ):
            if left[key] != right[key]:
                raise ProjectionError(
                    f"{model}.{name} authority {key} drift: "
                    f"TypeSpec={left[key]!r}, JSON Schema={right[key]!r}"
                )


def snake(name: str) -> str:
    result = re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()
    if not SQL_NAME_RE.fullmatch(result):
        raise ProjectionError(f"unsafe generated identifier: {result!r}")
    return result


def valid_proto_number(value: Any) -> bool:
    return (
        isinstance(value, int)
        and not isinstance(value, bool)
        and 0 < value <= PROTO_MAX
        and not 19_000 <= value <= 19_999
    )


def validate(
    config: dict[str, Any],
    lock: dict[str, Any],
    tsp_text: str,
    schemas: dict[str, dict[str, Any]],
) -> tuple[dict[str, dict[str, dict[str, Any]]], dict[str, list[str]]]:
    if config.get("formatVersion") != 1 or config.get("package") != "ores.rpc.v1":
        raise ProjectionError("projection identity must remain ores.rpc.v1 format v1")
    delta_items = config.get("expectedDeltas")
    delta_ids = {
        item.get("id")
        for item in delta_items or ()
        if isinstance(item, dict) and isinstance(item.get("message"), str)
        and len(item["message"]) >= 48
    }
    if delta_ids != DELTA_IDS or len(delta_items or ()) != len(DELTA_IDS):
        raise ProjectionError("reviewed representation-delta ledger drift")

    namespace = re.search(r"\bnamespace\s+([A-Za-z_][A-Za-z0-9_.]*);", tsp_text)
    if namespace is None or namespace.group(1) != "Ores.Rpc.V1":
        raise ProjectionError("TypeSpec namespace must remain Ores.Rpc.V1")
    enums = {"Transport": parse_tsp_enum(tsp_text, "Transport")}
    if enums["Transport"] != ["http", "tcp", "websocket", "nats"]:
        raise ProjectionError("TypeSpec Transport enum drift")
    if lock.get("enums", {}).get("ores.rpc.v1.Transport") != {
        "TRANSPORT_UNSPECIFIED": 0,
        "TRANSPORT_HTTP": 1,
        "TRANSPORT_TCP": 2,
        "TRANSPORT_WEBSOCKET": 3,
        "TRANSPORT_NATS": 4,
    }:
        raise ProjectionError("Protobuf Transport enum ledger drift")

    output = {}
    messages = config.get("messages")
    if not isinstance(messages, dict) or set(messages) != {"RpcCall", "RpcReceipt"}:
        raise ProjectionError("projection must define exactly RpcCall and RpcReceipt")
    for model in ("RpcCall", "RpcReceipt"):
        specification = messages[model]
        schema_path = specification.get("jsonSchema")
        if specification.get("typespec") != f"Ores.Rpc.V1.{model}":
            raise ProjectionError(f"{model} TypeSpec identity drift")
        if schema_path not in schemas:
            raise ProjectionError(f"{model} JSON Schema identity drift")
        tsp_fields = parse_tsp_model(tsp_text, model)
        schema_fields = parse_schema(schemas[schema_path])
        compare_authorities(model, tsp_fields, schema_fields, enums)
        output[model] = schema_fields

        proto_types = specification.get("protoTypes")
        if not isinstance(proto_types, dict) or set(proto_types) != set(schema_fields):
            raise ProjectionError(f"{model} Protobuf mapping inventory drift")
        locked = lock.get("messages", {}).get(f"ores.rpc.v1.{model}")
        if not isinstance(locked, dict) or set(locked.get("fields", {})) != set(schema_fields):
            raise ProjectionError(f"{model} Protobuf field ledger drift")
        numbers = list(locked["fields"].values())
        if not all(valid_proto_number(number) for number in numbers):
            raise ProjectionError(f"{model} Protobuf field number invalid")
        if len(numbers) != len(set(numbers)):
            raise ProjectionError(f"{model} Protobuf field ledger reuses a field number")
        if locked.get("reserved", []) != []:
            raise ProjectionError(f"{model} reserved ledger requires generator extension")

    headers = schemas["json-schema/rpc-call.schema.json"]["properties"]["headers"]
    if not isinstance(headers, dict) or not isinstance(headers.get("propertyNames"), dict):
        raise ProjectionError("header-name expected-delta evidence missing")
    if len(schemas["json-schema/rpc-receipt.schema.json"].get("allOf", [])) != 1:
        raise ProjectionError("receipt-state expected-delta evidence missing")
    for marker in (
        "alias RpcSuccessReceipt",
        "alias RpcErrorReceipt",
        "alias RpcReceiptState = RpcSuccessReceipt | RpcErrorReceipt;",
    ):
        if marker not in tsp_text:
            raise ProjectionError(f"TypeSpec receipt-state marker missing: {marker}")

    service = config.get("service")
    expected_service = {
        "name": "RpcGateway",
        "methods": [
            {
                "name": "Call",
                "request": "RpcCall",
                "response": "RpcReceipt",
                "clientStreaming": False,
                "serverStreaming": False,
            }
        ],
    }
    if service != expected_service:
        raise ProjectionError("gRPC service must remain unary RpcGateway.Call")
    return output, enums


def stable_digest(value: Any) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def render_proto(
    config: dict[str, Any],
    lock: dict[str, Any],
    fields: dict[str, dict[str, dict[str, Any]]],
    digest: str,
) -> str:
    lines = [
        "// GENERATED by scripts/generate-rpc-v1-projections.py; DO NOT EDIT.",
        f"// projection_sha256: {digest}",
        "// TypeSpec and JSON Schema/OpenAPI remain independent top-level authorities.",
        "// Field numbers are append-only release identity in idl/protobuf.lock.json.",
        "// Protobuf decoding is not admission; generated adapters run shared semantic validation.",
        "",
        'syntax = "proto3";',
        "",
        "package ores.rpc.v1;",
        "",
        "enum Transport {",
    ]
    for name, number in sorted(
        lock["enums"]["ores.rpc.v1.Transport"].items(), key=lambda item: item[1]
    ):
        lines.append(f"  {name} = {number};")
    lines.extend(
        [
            "}",
            "",
            "// Transport projection of the parity-approved RPC call shape.",
            "// JSON objects and unknown bodies use canonical UTF-8 JSON bytes.",
        ]
    )
    for model in ("RpcCall", "RpcReceipt"):
        if model == "RpcReceipt":
            lines.extend(
                [
                    "// Flattened transport projection of the TypeSpec RpcReceiptState alias and the",
                    "// JSON Schema if/then/else state machine. Protobuf alone cannot express that:",
                    "//   ok=true  => error absent; optional status is 200..399; body may be present",
                    "//   ok=false => error present; body absent; optional status is 400..599",
                    "// Generated Protobuf adapters must run the shared semantic validator.",
                ]
            )
        lines.append(f"message {model} {{")
        locked_fields = lock["messages"][f"ores.rpc.v1.{model}"]["fields"]
        for source_name, number in sorted(locked_fields.items(), key=lambda item: item[1]):
            field = fields[model][source_name]
            proto_type = config["messages"][model]["protoTypes"][source_name]
            proto_name = snake(source_name)
            optional = "" if field["required"] else "optional "
            option = (
                f' [json_name = "{source_name}"]'
                if proto_name != source_name
                else ""
            )
            lines.append(
                f"  {optional}{proto_type} {proto_name} = {number}{option};"
            )
        lines.extend(["}", ""])
    lines.extend(
        [
            "service RpcGateway {",
            "  rpc Call(RpcCall) returns (RpcReceipt);",
            "}",
            "",
        ]
    )
    return "\n".join(lines)


def sql_literal(value: Any) -> str:
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, int) and not isinstance(value, bool):
        return str(value)
    if isinstance(value, str):
        return "'" + value.replace("'", "''") + "'"
    raise ProjectionError(f"unsupported SQL literal: {value!r}")


def sql_type(field: dict[str, Any]) -> str:
    return {
        "string": "text",
        "integer": "integer",
        "number": "double precision",
        "boolean": "boolean",
        "object": "jsonb",
        "any": "jsonb",
    }[field["kind"]]


def field_checks(
    column: str, field: dict[str, Any], enums: dict[str, list[str]]
) -> list[str]:
    checks = []
    if field["const"] is not None:
        checks.append(f"{column} = {sql_literal(field['const'])}")
    if field["minLength"] is not None:
        checks.append(f"char_length({column}) >= {field['minLength']}")
    if field["maxLength"] is not None:
        checks.append(f"char_length({column}) <= {field['maxLength']}")
    if field["pattern"] is not None:
        if field["pattern"] != "^[A-Za-z][A-Za-z0-9_]*$":
            raise ProjectionError("unreviewed JSON Schema-to-PostgreSQL regex")
        checks.append(f"{column} ~ {sql_literal(field['pattern'])}")
    if field["minimum"] is not None:
        checks.append(f"{column} >= {field['minimum']}")
    if field["maximum"] is not None:
        checks.append(f"{column} <= {field['maximum']}")
    enum_values = field.get("enum", ())
    if enum_values:
        checks.append(
            f"{column} IN ({', '.join(sql_literal(value) for value in enum_values)})"
        )
    if field["kind"] == "object":
        checks.append(f"jsonb_typeof({column}) = 'object'")
    return checks


def render_sql(
    config: dict[str, Any],
    fields: dict[str, dict[str, dict[str, Any]]],
    enums: dict[str, list[str]],
    digest: str,
) -> str:
    schema = config["sql"]["schema"]
    if not SQL_NAME_RE.fullmatch(schema):
        raise ProjectionError("unsafe SQL schema name")
    output = [
        "-- GENERATED by scripts/generate-rpc-v1-projections.py; DO NOT EDIT.",
        f"-- projection_sha256: {digest}",
        "-- TypeSpec and JSON Schema/OpenAPI remain peer authorities. This DDL contains",
        "-- only their common structural semantics plus the reviewed receipt state rule.",
        "-- Application authorization, route ownership, and transaction policy remain external.",
        "",
        f"CREATE SCHEMA IF NOT EXISTS {schema};",
        "",
    ]
    for model in ("RpcCall", "RpcReceipt"):
        table = config["messages"][model]["sql"]["table"]
        if not SQL_NAME_RE.fullmatch(table):
            raise ProjectionError("unsafe SQL table name")
        columns, constraints = [], []
        for source_name, field in fields[model].items():
            column = snake(source_name)
            columns.append(
                f"  {column} {sql_type(field)}"
                + (" NOT NULL" if field["required"] else "")
            )
            for index, expression in enumerate(field_checks(column, field, enums), 1):
                if not field["required"]:
                    expression = f"{column} IS NULL OR ({expression})"
                constraints.append(
                    f"  CONSTRAINT {table}_{column}_check_{index} CHECK ({expression})"
                )
        if model == "RpcReceipt":
            constraints.append(
                "  CONSTRAINT receipts_v1_state_check CHECK ("
                "(ok = TRUE AND error IS NULL "
                "AND (status IS NULL OR status BETWEEN 200 AND 399)) "
                "OR (ok = FALSE AND error IS NOT NULL AND body IS NULL "
                "AND (status IS NULL OR status BETWEEN 400 AND 599)))"
            )
        primary = [snake(name) for name in config["messages"][model]["sql"]["primaryKey"]]
        constraints.append(
            f"  CONSTRAINT {table}_pkey PRIMARY KEY ({', '.join(primary)})"
        )
        output.extend(
            [
                f"CREATE TABLE IF NOT EXISTS {schema}.{table} (",
                ",\n".join([*columns, *constraints]),
                ");",
                "",
                f"REVOKE ALL ON TABLE {schema}.{table} FROM PUBLIC;",
                "",
            ]
        )
        for index, names in enumerate(config["messages"][model]["sql"]["indexes"], 1):
            output.extend(
                [
                    f"CREATE INDEX IF NOT EXISTS {table}_lookup_{index}",
                    f"  ON {schema}.{table} ({', '.join(snake(name) for name in names)});",
                    "",
                ]
            )
    return "\n".join(output)


def render_manifest(config: dict[str, Any], digest: str) -> str:
    service = config["service"]
    package = config["package"]
    methods = [
        {
            "clientStreaming": method["clientStreaming"],
            "fullName": f"{package}.{service['name']}.{method['name']}",
            "name": method["name"],
            "request": f"{package}.{method['request']}",
            "response": f"{package}.{method['response']}",
            "serverStreaming": method["serverStreaming"],
        }
        for method in service["methods"]
    ]
    value = {
        "expectedDeltaIds": sorted(DELTA_IDS),
        "formatVersion": 1,
        "package": package,
        "projectionSha256": digest,
        "proto": "../../idl/protobuf/ores/rpc/v1/rpc.proto",
        "semanticValidator": "../../json-schema/rpc-receipt.schema.json",
        "service": {
            "fullName": f"{package}.{service['name']}",
            "methods": methods,
            "name": service["name"],
        },
        "sql": "rpc-storage.sql",
    }
    return json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"


def write_or_check(path: Path, content: str, check: bool) -> None:
    if check:
        if not path.is_file() or path.read_text(encoding="utf-8") != content:
            raise ProjectionError(
                f"generated output drift: {path}; run generate-rpc-v1-projections.py"
            )
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, prefix=f".{path.name}.", delete=False
    ) as handle:
        handle.write(content)
        temporary = Path(handle.name)
    os.replace(temporary, path)
    path.chmod(0o444)


def generate(root: Path, check: bool) -> str:
    config = load_json(root / "idl/rpc-v1.projection.json")
    lock = load_json(root / "idl/protobuf.lock.json")
    tsp_text = (root / "idl/typespec/v1.tsp").read_text(encoding="utf-8")
    schemas = {
        "json-schema/rpc-call.schema.json": load_json(
            root / "json-schema/rpc-call.schema.json"
        ),
        "json-schema/rpc-receipt.schema.json": load_json(
            root / "json-schema/rpc-receipt.schema.json"
        ),
    }
    fields, enums = validate(config, lock, tsp_text, schemas)
    digest = stable_digest(
        {
            "formatVersion": 1,
            "projection": config,
            "typespec": tsp_text,
            "jsonSchemas": schemas,
            "protobufLock": lock,
        }
    )
    outputs = {
        root / "idl/protobuf/ores/rpc/v1/rpc.proto": render_proto(
            config, lock, fields, digest
        ),
        root / "generated/rpc-v1/rpc-storage.sql": render_sql(
            config, fields, enums, digest
        ),
        root / "generated/rpc-v1/grpc.json": render_manifest(config, digest),
    }
    for path, content in outputs.items():
        write_or_check(path, content, check)
    return digest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        digest = generate(args.root.resolve(), args.check)
    except (OSError, KeyError, TypeError, ProjectionError) as error:
        print(f"RPC v1 projection admission veto: {error}", file=sys.stderr)
        return 1
    print(
        f"{'verified' if args.check else 'generated'} "
        f"RPC v1 SQL/Protobuf/gRPC projection {digest}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
