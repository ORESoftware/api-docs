#!/usr/bin/env python3
"""Cross-check independently authored RPC peer lanes.

TypeSpec and JSON Schema/OpenAPI are co-equal top-level authorities. Protobuf is
the reviewed binary/streaming artifact of the TypeSpec lane. This gate extracts
structural fingerprints and diffs them; known representation losses live in
idl/expected-deltas.json. Anything else is a release veto and enters
STOPPED_FOR_EVALUATION. No lane wins automatically.

Does not compile TypeSpec, open sockets, or depend on opto-sync / ores-otel.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

DECORATOR_RE = re.compile(r"^@(\w+)(?:\((.*)\))?\s*$")
PROP_RE = re.compile(r"^(`[^`]+`|[A-Za-z_][\w.]*)(\?)?:\s*(.+);$")
MODEL_RE = re.compile(r"^model\s+(\w+)\s*\{")
ENUM_RE = re.compile(r"^enum\s+(\w+)\s*\{")
UNION_RE = re.compile(r"^union\s+(\w+)\s*\{")
NS_RE = re.compile(r"^namespace\s+([\w.]+);")
PROTO_MSG_RE = re.compile(r"^message\s+(\w+)\s*\{")
PROTO_ENUM_RE = re.compile(r"^enum\s+(\w+)\s*\{")
PROTO_FIELD_RE = re.compile(
    r"^(optional|repeated)?\s*([\w.]+)\s+(\w+)\s*=\s*(\d+)\s*(?:\[([^\]]+)\])?;"
)
PROTO_ENUM_FIELD_RE = re.compile(r"^(\w+)\s*=\s*(\d+);")
PROTO_PACKAGE_RE = re.compile(r"^package\s+([\w.]+);")
JSON_NAME_RE = re.compile(r'json_name\s*=\s*"([^"]+)"')


@dataclass
class Field:
    name: str
    required: bool
    kind: str
    const: Any = None
    enum: list[str] | None = None
    min_length: int | None = None
    max_length: int | None = None
    pattern: str | None = None
    minimum: int | None = None
    maximum: int | None = None
    proto_number: int | None = None

    def fingerprint(self) -> dict[str, Any]:
        out: dict[str, Any] = {"required": self.required, "kind": self.kind}
        if self.const is not None:
            out["const"] = self.const
        if self.enum:
            out["enum"] = list(self.enum)
        if self.min_length is not None:
            out["minLength"] = self.min_length
        if self.max_length is not None:
            out["maxLength"] = self.max_length
        if self.pattern is not None:
            out["pattern"] = self.pattern
        if self.minimum is not None:
            out["minimum"] = self.minimum
        if self.maximum is not None:
            out["maximum"] = self.maximum
        if self.proto_number is not None:
            out["proto"] = self.proto_number
        return out


@dataclass
class Shape:
    name: str
    source: str
    fields: dict[str, Field] = field(default_factory=dict)
    enum_values: list[str] | None = None
    union_members: list[str] | None = None


def _strip_comment(line: str) -> str:
    if "//" in line:
        line = line[: line.index("//")]
    return line.strip()


def _unquote_ident(name: str) -> str:
    if name.startswith("`") and name.endswith("`"):
        return name[1:-1]
    return name


def _parse_decorators(raw: str | None) -> dict[str, Any]:
    extras: dict[str, Any] = {}
    if raw is None:
        return extras
    raw = raw.strip()
    if raw.startswith('"') and raw.endswith('"'):
        extras["pattern"] = json.loads(raw)
        return extras
    if re.fullmatch(r"-?\d+", raw):
        extras["int"] = int(raw)
        return extras
    return extras


def parse_typespec(text: str, source: str, namespace: str = "") -> dict[str, Shape]:
    shapes: dict[str, Shape] = {}
    ns = namespace
    i = 0
    lines = text.splitlines()
    pending: list[tuple[str, str | None]] = []

    def flush_pending() -> dict[str, Any]:
        acc: dict[str, Any] = {}
        for name, arg in pending:
            parsed = _parse_decorators(arg)
            if name == "minLength":
                acc["min_length"] = parsed.get("int")
            elif name == "maxLength":
                acc["max_length"] = parsed.get("int")
            elif name == "minValue":
                acc["minimum"] = parsed.get("int")
            elif name == "maxValue":
                acc["maximum"] = parsed.get("int")
            elif name == "pattern":
                acc["pattern"] = parsed.get("pattern")
        pending.clear()
        return acc

    def parse_type(type_src: str, extras: dict[str, Any], required: bool, name: str) -> Field:
        type_src = type_src.strip().rstrip(",")
        kind = "unknown"
        const: Any = None
        enum: list[str] | None = None
        if type_src.startswith('"') and "|" in type_src:
            enum = [json.loads(part.strip()) for part in type_src.split("|")]
            kind = "enum"
        elif type_src.startswith('"') and type_src.endswith('"'):
            const = json.loads(type_src)
            kind = "const"
        elif re.fullmatch(r"-?\d+", type_src):
            const = int(type_src)
            kind = "const"
        elif type_src == "string":
            kind = "string"
        elif type_src in {"int32", "int64", "uint32", "integer"}:
            kind = "integer"
        elif type_src == "boolean":
            kind = "boolean"
        elif type_src == "unknown":
            kind = "any"
        elif type_src.startswith("Record<") or type_src.endswith("[]"):
            kind = "object" if type_src.startswith("Record<") else "array"
        else:
            kind = "ref"
        return Field(
            name=name,
            required=required,
            kind=kind,
            const=const,
            enum=enum,
            min_length=extras.get("min_length"),
            max_length=extras.get("max_length"),
            pattern=extras.get("pattern"),
            minimum=extras.get("minimum"),
            maximum=extras.get("maximum"),
        )

    while i < len(lines):
        raw = lines[i]
        line = _strip_comment(raw)
        i += 1
        if not line:
            continue
        ns_m = NS_RE.match(line)
        if ns_m:
            ns = ns_m.group(1)
            continue
        dec = DECORATOR_RE.match(line)
        if dec:
            pending.append((dec.group(1), dec.group(2)))
            continue
        opened = MODEL_RE.match(line) or ENUM_RE.match(line) or UNION_RE.match(line)
        if not opened:
            pending.clear()
            continue
        kind = (
            "model"
            if line.startswith("model")
            else "enum"
            if line.startswith("enum")
            else "union"
        )
        name = f"{ns}.{opened.group(1)}" if ns else opened.group(1)
        body: list[str] = []
        depth = line.count("{") - line.count("}")
        while depth > 0 and i < len(lines):
            nxt = lines[i]
            i += 1
            depth += nxt.count("{") - nxt.count("}")
            if depth > 0:
                body.append(nxt)
        shape = Shape(name=name, source=source)
        if kind == "enum":
            values = []
            for item in body:
                token = _strip_comment(item).rstrip(",")
                if token:
                    values.append(token.split(":")[0].strip())
            shape.enum_values = values
        elif kind == "union":
            members = []
            for item in body:
                token = _strip_comment(item).rstrip(",")
                if ":" in token:
                    members.append(token.split(":", 1)[0].strip())
            shape.union_members = members
        else:
            inner_pending: list[tuple[str, str | None]] = []
            for item in body:
                s = _strip_comment(item)
                if not s:
                    continue
                d = DECORATOR_RE.match(s)
                if d:
                    inner_pending.append((d.group(1), d.group(2)))
                    continue
                p = PROP_RE.match(s)
                if not p:
                    inner_pending.clear()
                    continue
                extras = {}
                # reuse decorator flush against inner_pending
                saved = list(pending)
                pending[:] = inner_pending
                extras = flush_pending()
                pending[:] = saved
                inner_pending.clear()
                fname = _unquote_ident(p.group(1))
                required = p.group(2) != "?"
                shape.fields[fname] = parse_type(p.group(3), extras, required, fname)
        shapes[name] = shape
        pending.clear()
    return shapes


def _schema_field(name: str, spec: Any, required: bool) -> Field:
    if spec is True:
        return Field(name=name, required=required, kind="any")
    if not isinstance(spec, dict):
        return Field(name=name, required=required, kind="unknown")
    kind = spec.get("type") or ("const" if "const" in spec else "enum" if "enum" in spec else "unknown")
    if spec.get("const") is not None and kind == "unknown":
        kind = "const"
    if spec.get("enum") and kind == "unknown":
        kind = "enum"
    enum = spec.get("enum")
    if enum is not None:
        enum = [str(v) for v in enum]
    return Field(
        name=name,
        required=required,
        kind=str(kind),
        const=spec.get("const"),
        enum=enum,
        min_length=spec.get("minLength"),
        max_length=spec.get("maxLength"),
        pattern=spec.get("pattern"),
        minimum=spec.get("minimum"),
        maximum=spec.get("maximum"),
    )


def parse_json_schema(doc: dict[str, Any], name: str, source: str) -> Shape:
    required = set(doc.get("required") or [])
    shape = Shape(name=name, source=source)
    for fname, spec in (doc.get("properties") or {}).items():
        shape.fields[fname] = _schema_field(fname, spec, fname in required)
    for block in doc.get("allOf") or []:
        then = block.get("then") or {}
        then_req = set(then.get("required") or [])
        for fname, spec in (then.get("properties") or {}).items():
            if fname not in shape.fields:
                shape.fields[fname] = _schema_field(fname, spec, fname in then_req)
    return shape


def parse_proto(text: str, source: str) -> dict[str, Shape]:
    shapes: dict[str, Shape] = {}
    package = ""
    i = 0
    lines = text.splitlines()
    while i < len(lines):
        line = _strip_comment(lines[i])
        i += 1
        pkg = PROTO_PACKAGE_RE.match(line)
        if pkg:
            package = pkg.group(1)
            continue
        opened = PROTO_MSG_RE.match(line) or PROTO_ENUM_RE.match(line)
        if not opened:
            continue
        kind = "message" if line.startswith("message") else "enum"
        qname = f"{package}.{opened.group(1)}" if package else opened.group(1)
        body: list[str] = []
        depth = line.count("{") - line.count("}")
        while depth > 0 and i < len(lines):
            nxt = lines[i]
            i += 1
            depth += nxt.count("{") - nxt.count("}")
            if depth > 0:
                body.append(nxt)
        shape = Shape(name=qname, source=source)
        if kind == "enum":
            values = []
            for item in body:
                m = PROTO_ENUM_FIELD_RE.match(_strip_comment(item))
                if m:
                    values.append(m.group(1))
            shape.enum_values = values
        else:
            for item in body:
                m = PROTO_FIELD_RE.match(_strip_comment(item))
                if not m:
                    continue
                raw_name = m.group(3)
                json_name = raw_name
                opts = m.group(5) or ""
                jn = JSON_NAME_RE.search(opts)
                if jn:
                    json_name = jn.group(1)
                optional = m.group(1) == "optional"
                ptype = m.group(2)
                kind_map = {
                    "string": "string",
                    "bool": "boolean",
                    "uint32": "integer",
                    "int32": "integer",
                    "bytes": "any",
                }
                shape.fields[json_name] = Field(
                    name=json_name,
                    required=not optional and ptype != "bytes",
                    kind=kind_map.get(ptype, "ref"),
                    proto_number=int(m.group(4)),
                )
        shapes[qname] = shape
    return shapes


def _tree(root: Path) -> tuple[Path, Path, Path]:
    return root / "idl" / "typespec", root / "idl" / "protobuf", root / "json-schema"


def load_all_typespec(root: Path) -> dict[str, Shape]:
    tsp_dir, _, _ = _tree(root)
    out: dict[str, Shape] = {}
    for path in sorted(tsp_dir.glob("*.tsp")):
        if path.name == "main.tsp":
            continue
        out.update(parse_typespec(path.read_text(encoding="utf-8"), str(path.relative_to(root))))
    return out


def load_all_proto(root: Path) -> dict[str, Shape]:
    _, proto_dir, _ = _tree(root)
    out: dict[str, Shape] = {}
    for path in sorted(proto_dir.rglob("*.proto")):
        out.update(parse_proto(path.read_text(encoding="utf-8"), str(path.relative_to(root))))
    return out


def load_json_shapes(root: Path) -> dict[str, Shape]:
    _, _, schema_dir = _tree(root)
    out = {}
    for stem in ("rpc-call", "rpc-receipt", "rpc-frame", "telemetry-attributes"):
        path = schema_dir / f"{stem}.schema.json"
        doc = json.loads(path.read_text(encoding="utf-8"))
        out[stem] = parse_json_schema(doc, stem, str(path.relative_to(root)))
    return out


def field_names(shape: Shape) -> set[str]:
    return set(shape.fields)


def compare_names(left: Shape, right: Shape) -> list[str]:
    missing = sorted(field_names(left) - field_names(right))
    extra = sorted(field_names(right) - field_names(left))
    diffs = []
    if missing:
        diffs.append(f"{right.name} missing fields present in {left.name}: {missing}")
    if extra:
        diffs.append(f"{right.name} has extra fields vs {left.name}: {extra}")
    return diffs


def compare_constraints(left: Shape, right: Shape, *, skip_kinds: bool) -> list[str]:
    diffs = []
    for name in sorted(field_names(left) & field_names(right)):
        a, b = left.fields[name], right.fields[name]
        for attr in ("const", "min_length", "max_length", "pattern", "minimum", "maximum"):
            av, bv = getattr(a, attr), getattr(b, attr)
            if av is not None and bv is not None and av != bv:
                diffs.append(f"{left.name}.{name}.{attr}={av} vs {right.name}.{name}.{attr}={bv}")
        if a.enum and b.enum and sorted(a.enum) != sorted(b.enum):
            diffs.append(f"{left.name}.{name}.enum {a.enum} vs {right.name}.{name}.enum {b.enum}")
        if a.required != b.required and not skip_kinds:
            diffs.append(
                f"{left.name}.{name} required={a.required} vs {right.name}.{name} required={b.required}"
            )
    return diffs


def proto_enum_json_values(values: list[str]) -> list[str]:
    out = []
    for item in values:
        if item.endswith("_UNSPECIFIED"):
            continue
        lowered = item.lower()
        if lowered.startswith("transport_"):
            out.append(lowered[len("transport_") :])
        else:
            out.append(lowered)
    return out


def check_protobuf_lock(proto_shapes: dict[str, Shape], lock: dict[str, Any]) -> list[str]:
    diffs = []
    messages = lock.get("messages") or {}
    for qname, expected in messages.items():
        shape = proto_shapes.get(qname)
        if shape is None:
            diffs.append(f"protobuf.lock missing message in sources: {qname}")
            continue
        got = {f.name: f.proto_number for f in shape.fields.values()}
        want = expected.get("fields") or {}
        if got != want:
            diffs.append(f"{qname} field numbers {got} != lock {want}")
        reserved = set(expected.get("reserved") or [])
        overlap = reserved & set(got.values())
        if overlap:
            diffs.append(f"{qname} reuses reserved field numbers {sorted(overlap)}")
    return diffs


def run(root: Path | None = None) -> dict[str, Any]:
    root = root or ROOT
    idl = root / "idl"
    schemas = load_json_shapes(root)
    tsp = load_all_typespec(root)
    proto = load_all_proto(root)
    lock = json.loads((idl / "protobuf.lock.json").read_text(encoding="utf-8"))
    expected_doc = json.loads((idl / "expected-deltas.json").read_text(encoding="utf-8"))
    expected_ids = {d["id"] for d in expected_doc.get("deltas") or []}

    vetoes: list[str] = []
    notes: list[str] = []

    pairs = [
        (schemas["rpc-call"], tsp["Ores.Rpc.V1.RpcCall"], proto["ores.rpc.v1.RpcCall"]),
        (schemas["rpc-receipt"], tsp["Ores.Rpc.V1.RpcReceipt"], proto["ores.rpc.v1.RpcReceipt"]),
        (schemas["telemetry-attributes"], tsp["Ores.Rpc.Telemetry.TelemetryAttributes"], None),
    ]
    for schema, model, message in pairs:
        vetoes.extend(compare_names(schema, model))
        vetoes.extend(compare_constraints(schema, model, skip_kinds=False))
        if message is not None:
            vetoes.extend(compare_names(schema, message))
            notes.append(f"expected-delta proto-json-bytes applies to {message.name}")

    frame_schema = schemas["rpc-frame"]
    frame_union = tsp["Ores.Rpc.V2.RpcFrame"]
    if not frame_union.union_members:
        vetoes.append("TypeSpec RpcFrame must be a union of call/data/end/error/cancel")
    else:
        want = {"call", "data", "end", "error", "cancel"}
        got = set(frame_union.union_members)
        if got != want:
            vetoes.append(f"TypeSpec RpcFrame arms {sorted(got)} != {sorted(want)}")

    tsp_frame_fields: set[str] = set()
    for arm in ("RpcCallFrame", "RpcDataFrame", "RpcEndFrame", "RpcErrorFrame", "RpcCancelFrame"):
        shape = tsp.get(f"Ores.Rpc.V2.{arm}")
        if shape is None:
            vetoes.append(f"missing TypeSpec model Ores.Rpc.V2.{arm}")
            continue
        tsp_frame_fields.update(field_names(shape))
    schema_frame_fields = field_names(frame_schema)
    if schema_frame_fields - tsp_frame_fields:
        vetoes.append(
            f"JSON Schema rpc-frame fields missing from TypeSpec arms: "
            f"{sorted(schema_frame_fields - tsp_frame_fields)}"
        )
    notes.append("expected-delta v2-union-vs-if-then applies to rpc-frame")

    proto_frame = proto["ores.rpc.v2.RpcFrame"]
    if schema_frame_fields - field_names(proto_frame):
        vetoes.append(
            f"JSON Schema rpc-frame fields missing from proto: "
            f"{sorted(schema_frame_fields - field_names(proto_frame))}"
        )
    notes.append("expected-delta v2-proto-flattened applies to ores.rpc.v2.RpcFrame")

    transport = tsp.get("Ores.Rpc.V1.Transport")
    schema_transport = schemas["rpc-call"].fields["transport"].enum or []
    if transport and transport.enum_values and sorted(transport.enum_values) != sorted(schema_transport):
        vetoes.append(
            f"TypeSpec Transport {transport.enum_values} != JSON Schema {schema_transport}"
        )
    proto_transport = proto.get("ores.rpc.v1.Transport")
    if proto_transport and proto_transport.enum_values:
        mapped = proto_enum_json_values(proto_transport.enum_values)
        if sorted(mapped) != sorted(schema_transport):
            vetoes.append(f"protobuf Transport {mapped} != JSON Schema {schema_transport}")
        notes.append("expected-delta proto-transport-unspecified applies to ores.rpc.v1.Transport")

    vetoes.extend(check_protobuf_lock(proto, lock))

    # Known deltas must exist as documentation; they do not auto-clear name mismatches.
    missing_docs = {
        "proto-transport-unspecified",
        "proto-json-bytes",
        "v2-union-vs-if-then",
        "v2-unevaluated-properties",
        "additional-properties-closed",
        "v2-meta-map-vs-object",
        "v2-proto-flattened",
    } - expected_ids
    if missing_docs:
        vetoes.append(f"expected-deltas.json missing ids {sorted(missing_docs)}")

    report = {
        "ok": not vetoes,
        "vetoes": vetoes,
        "notes": notes,
        "shapes": {
            "json-schema": sorted(schemas),
            "typespec": sorted(tsp),
            "protobuf": sorted(proto),
        },
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--write-report", type=Path)
    args = parser.parse_args(argv)
    report = run(args.root)
    text = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.write_report:
        args.write_report.parent.mkdir(parents=True, exist_ok=True)
        args.write_report.write_text(text, encoding="utf-8")
    sys.stdout.write(text)
    if report["vetoes"]:
        sys.stderr.write("rpc idl peer-authority veto\n")
        for item in report["vetoes"]:
            sys.stderr.write(f"  {item}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
