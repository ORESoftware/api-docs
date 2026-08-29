#!/usr/bin/env python3
"""Generate compile-time route objects from a route-map JSON.

The JSON map is the shared source. This emits:

- TypeScript `Routes` const (keys → path literals + typed path/query/body)
- Rust `RouteKey` enum (exhaustive match on the backend)
- Dart `Routes` class with typed records
- Gleam `RouteKey` custom type (exhaustive `case` on the backend)

Frontend code uses keys instead of path strings. Backend `match` / `Handlers`
types fail to compile when a key is added without a handler.

Usage:
  python3 scripts/generate-routes.py --map examples/pmap-api.route-map.json --out generated
  python3 scripts/generate-routes.py --check
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]

_spec = importlib.util.spec_from_file_location(
    "check_route_sync", ROOT / "scripts" / "check-route-sync.py"
)
_mod = importlib.util.module_from_spec(_spec)
assert _spec.loader is not None
_spec.loader.exec_module(_mod)

infer_methods = _mod.infer_methods  # noqa: F401  — re-exported for tests
normalize_entry = _mod.normalize_entry
path_template_vars = _mod.path_template_vars


def pascal(key: str) -> str:
    if key and key[0].isupper() and "_" not in key:
        return key
    return "".join(part[:1].upper() + part[1:] for part in key.replace("-", "_").split("_") if part)


def rust_variant(key: str) -> str:
    return pascal(key)


def dart_ident(key: str) -> str:
    return f"rpc{key}" if key[:1].isupper() else key


def ts_type(schema: Any | None, fallback: str = "unknown") -> str:
    if not isinstance(schema, dict):
        return fallback
    if "enum" in schema and all(isinstance(x, str) for x in schema["enum"]):
        return " | ".join(json.dumps(x) for x in schema["enum"])
    types = schema.get("type")
    if isinstance(types, list):
        parts = [ts_type({**schema, "type": t}, fallback) for t in types if t != "null"]
        inner = " | ".join(parts) if parts else fallback
        return f"{inner} | null" if "null" in types else inner
    t = types
    if t == "string":
        return "string"
    if t in ("integer", "number"):
        return "number"
    if t == "boolean":
        return "boolean"
    if t == "array":
        return f"Array<{ts_type(schema.get('items'), 'unknown')}>"
    if t == "object" or "properties" in schema:
        props = schema.get("properties") or {}
        required = set(schema.get("required") or [])
        fields = []
        for name, sub in props.items():
            opt = "" if name in required else "?"
            fields.append(f"{json.dumps(name)}{opt}: {ts_type(sub)}")
        return "{ " + "; ".join(fields) + " }" if fields else "Record<string, unknown>"
    return fallback


def rust_type(schema: Any | None, fallback: str = "serde_json::Value") -> str:
    if not isinstance(schema, dict):
        return fallback
    types = schema.get("type")
    if isinstance(types, list):
        non_null = [t for t in types if t != "null"]
        inner = rust_type({**schema, "type": non_null[0]}, fallback) if non_null else fallback
        return f"Option<{inner}>" if "null" in types else inner
    t = types
    if t == "string":
        return "String"
    if t == "integer":
        return "i64"
    if t == "number":
        return "f64"
    if t == "boolean":
        return "bool"
    if t == "array":
        return f"Vec<{rust_type(schema.get('items'), 'serde_json::Value')}>"
    return fallback


RUST_KEYWORDS = {
    "as",
    "async",
    "await",
    "break",
    "const",
    "continue",
    "crate",
    "dyn",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "include",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "union",
    "unsafe",
    "use",
    "where",
    "while",
    "yield",
    "box",
}


def rust_field_name(fname: str) -> tuple[str, str]:
    rust_name = "".join("_" + c.lower() if c.isupper() else c for c in fname).lstrip("_")
    rust_name = rust_name.replace("-", "_")
    if rust_name in RUST_KEYWORDS:
        rust_name = f"{rust_name}_"
    rename = f'    #[serde(rename = "{fname}")]\n' if rust_name != fname else ""
    return rust_name, rename


def rust_struct(name: str, schema: dict[str, Any]) -> str:
    props = schema.get("properties") or {}
    required = set(schema.get("required") or [])
    fields = []
    for fname, sub in props.items():
        ty = rust_type(sub)
        if fname not in required and not ty.startswith("Option<"):
            ty = f"Option<{ty}>"
        rust_name, rename = rust_field_name(fname)
        fields.append(f"{rename}    pub {rust_name}: {ty},")
    body = "\n".join(fields) if fields else "    // no fields"
    return (
        f"#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]\n"
        f"pub struct {name} {{\n{body}\n}}\n"
    )


def gen_typescript(service: str, mapping: dict[str, Any]) -> str:
    companion: dict[str, tuple[str, str, str, str]] = {}
    lines = [
        "/** Generated from a route-map JSON. Do not edit by hand. */",
        "",
        "export type HttpMethod = \"GET\" | \"POST\" | \"PUT\" | \"PATCH\" | \"DELETE\" | \"HEAD\" | \"OPTIONS\";",
        'export type RpcTransport = "http" | "tcp" | "websocket" | "nats";',
        "",
        f"export const SERVICE = {json.dumps(service)} as const;",
        "",
        "export const Routes = {",
    ]
    for key, raw in mapping.items():
        entry = normalize_entry(key, raw)
        obj = raw if isinstance(raw, dict) else {}
        path_schema = obj.get("path_params")
        query_schema = obj.get("query_schema")
        req_schema = obj.get("request_schema")
        res_schema = obj.get("response_schema")
        path_t = ts_type(path_schema, "{ [k: string]: string }") if path_schema else "Record<string, never>"
        query_t = ts_type(query_schema, "Record<string, never>") if query_schema else "Record<string, never>"
        req_t = ts_type(req_schema, "unknown") if req_schema else "void"
        res_t = ts_type(res_schema, "unknown") if res_schema else "unknown"
        companion[key] = (path_t, query_t, req_t, res_t)
        vars_ = path_template_vars(entry["path"])
        build = "undefined as ((p: Record<string, never>) => string) | undefined"
        if vars_:
            build = (
                f"(p: {path_t}) => {json.dumps(entry['path'])}.replace(/\\{{([^}}]+)\\}}/g, "
                f"(_, n) => encodeURIComponent(String((p as Record<string, string>)[n])))"
            )
        methods_lit = ", ".join(json.dumps(m) for m in entry["methods"])
        transports_lit = ", ".join(json.dumps(t) for t in entry["transports"])
        lines.append(f"  {json.dumps(key)}: {{")
        lines.append(f"    key: {json.dumps(key)},")
        lines.append(f"    path: {json.dumps(entry['path'])} as const,")
        lines.append(f"    methods: [{methods_lit}] as const,")
        lines.append(f"    transports: [{transports_lit}] as const,")
        lines.append(f"    buildPath: {build},")
        lines.append("  },")
    lines.extend(
        [
            "} as const;",
            "",
            "export type RouteName = keyof typeof Routes;",
            "",
            "export interface RouteTypes {",
        ]
    )
    for key, (path_t, query_t, req_t, res_t) in companion.items():
        lines.append(
            f'  {json.dumps(key)}: {{ path: {path_t}; query: {query_t}; body: {req_t}; response: {res_t} }};'
        )
    lines.extend(
        [
            "}",
            "",
            "/** Adding a map key without a handler is a TypeScript error. */",
            "export type RouteHandlers<Ctx> = {",
            "  [K in RouteName]: (ctx: Ctx, args: {",
            '    path: RouteTypes[K]["path"];',
            '    query: RouteTypes[K]["query"];',
            '    body: RouteTypes[K]["body"];',
            '  }) => Promise<RouteTypes[K]["response"]> | RouteTypes[K]["response"];',
            "};",
            "",
            "export function lookup<K extends RouteName>(key: K): (typeof Routes)[K] {",
            "  return Routes[key];",
            "}",
            "",
        ]
    )
    return "\n".join(lines) + "\n"


def gen_dart(service: str, mapping: dict[str, Any]) -> str:
    lines = [
        "/// Generated from a route-map JSON. Do not edit by hand.",
        "library;",
        "",
        f"const String kService = {json.dumps(service)};",
        "",
        "class RouteMeta {",
        "  const RouteMeta({required this.key, required this.path, required this.methods, this.transports = const ['http']});",
        "  final String key;",
        "  final String path;",
        "  final List<String> methods;",
        "  final List<String> transports;",
        "  String expand(Map<String, String> params) {",
        "    var out = path;",
        "    params.forEach((k, v) {",
        "      out = out.replaceAll('{$k}', Uri.encodeComponent(v));",
        "    });",
        "    return out;",
        "  }",
        "}",
        "",
        "abstract final class Routes {",
    ]
    for key, raw in mapping.items():
        entry = normalize_entry(key, raw)
        ident = dart_ident(key)
        methods = ", ".join(json.dumps(m) for m in entry["methods"])
        transports = ", ".join(json.dumps(t) for t in entry["transports"])
        lines.append(
            f"  static const {ident} = RouteMeta(key: {json.dumps(key)}, "
            f"path: {json.dumps(entry['path'])}, methods: [{methods}], "
            f"transports: [{transports}]);"
        )
    lines.append("")
    lines.append("  static const Map<String, RouteMeta> byKey = {")
    for key in mapping:
        lines.append(f"    {json.dumps(key)}: {dart_ident(key)},")
    lines.extend(["  };", "}", ""])
    return "\n".join(lines) + "\n"


def gen_rust(service: str, mapping: dict[str, Any]) -> str:
    variants = []
    as_str = []
    from_str = []
    path_match = []
    methods_match = []
    transports_match = []
    structs: list[str] = []
    for key, raw in mapping.items():
        var = rust_variant(key)
        variants.append(f"    {var},")
        as_str.append(f'            Self::{var} => {json.dumps(key)},')
        from_str.append(f'            {json.dumps(key)} => Some(Self::{var}),')
        entry = normalize_entry(key, raw)
        path_match.append(f'            Self::{var} => {json.dumps(entry["path"])},')
        methods_lit = ", ".join(f'"{m}"' for m in entry["methods"])
        methods_match.append(f"            Self::{var} => &[{methods_lit}],")
        transports_lit = ", ".join(f'"{t}"' for t in entry["transports"])
        transports_match.append(f"            Self::{var} => &[{transports_lit}],")
        obj = raw if isinstance(raw, dict) else {}
        if isinstance(obj.get("path_params"), dict) and obj["path_params"].get("properties"):
            structs.append(rust_struct(f"{var}Path", obj["path_params"]))
        if isinstance(obj.get("query_schema"), dict) and obj["query_schema"].get("properties"):
            structs.append(rust_struct(f"{var}Query", obj["query_schema"]))
        if isinstance(obj.get("request_schema"), dict) and obj["request_schema"].get("properties"):
            structs.append(rust_struct(f"{var}Request", obj["request_schema"]))
        if isinstance(obj.get("response_schema"), dict) and obj["response_schema"].get("properties"):
            structs.append(rust_struct(f"{var}Response", obj["response_schema"]))
    all_vars = ", ".join(f"Self::{rust_variant(k)}" for k in mapping)
    struct_block = "\n".join(structs)
    return f'''//! Generated from a route-map JSON. Do not edit by hand.
//! Exhaustive `RouteKey` match is the backend compile check.
#![allow(dead_code)]

pub const SERVICE: &str = {json.dumps(service)};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RouteKey {{
{chr(10).join(variants)}
}}

impl RouteKey {{
    pub const ALL: &'static [Self] = &[{all_vars}];

    #[must_use]
    pub fn as_str(self) -> &'static str {{
        match self {{
{chr(10).join(as_str)}
        }}
    }}

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {{
        match key {{
{chr(10).join(from_str)}
            _ => None,
        }}
    }}

    #[must_use]
    pub fn path(self) -> &'static str {{
        match self {{
{chr(10).join(path_match)}
        }}
    }}

    #[must_use]
    pub fn methods(self) -> &'static [&'static str] {{
        match self {{
{chr(10).join(methods_match)}
        }}
    }}

    #[must_use]
    pub fn transports(self) -> &'static [&'static str] {{
        match self {{
{chr(10).join(transports_match)}
        }}
    }}
}}

{struct_block}
'''


def gleam_variant(key: str) -> str:
    """Gleam custom-type constructors are PascalCase public names."""
    return pascal(key)


def gen_gleam(service: str, mapping: dict[str, Any]) -> str:
    variants = [f"  {gleam_variant(key)}" for key in mapping]
    to_string = []
    parse = []
    path_match = []
    methods_match = []
    transports_match = []
    all_vars = []
    for key, raw in mapping.items():
        var = gleam_variant(key)
        entry = normalize_entry(key, raw)
        to_string.append(f"    {var} -> {json.dumps(key)}")
        parse.append(f"    {json.dumps(key)} -> Ok({var})")
        path_match.append(f"    {var} -> {json.dumps(entry['path'])}")
        methods_lit = ", ".join(json.dumps(m) for m in entry["methods"])
        methods_match.append(f"    {var} -> [{methods_lit}]")
        transports_lit = ", ".join(json.dumps(t) for t in entry["transports"])
        transports_match.append(f"    {var} -> [{transports_lit}]")
        all_vars.append(var)
    return f'''//// Generated from a route-map JSON. Do not edit by hand.
//// Exhaustive `RouteKey` case is the backend compile check.

pub const service: String = {json.dumps(service)}

pub type RouteKey {{
{chr(10).join(variants)}
}}

pub fn all() -> List(RouteKey) {{
  [{", ".join(all_vars)}]
}}

pub fn to_string(key: RouteKey) -> String {{
  case key {{
{chr(10).join(to_string)}
  }}
}}

pub fn parse(key: String) -> Result(RouteKey, Nil) {{
  case key {{
{chr(10).join(parse)}
    _ -> Error(Nil)
  }}
}}

pub fn path(key: RouteKey) -> String {{
  case key {{
{chr(10).join(path_match)}
  }}
}}

pub fn methods(key: RouteKey) -> List(String) {{
  case key {{
{chr(10).join(methods_match)}
  }}
}}

pub fn transports(key: RouteKey) -> List(String) {{
  case key {{
{chr(10).join(transports_match)}
  }}
}}
'''


def write_readonly(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.chmod(path.stat().st_mode | 0o200)
    # Every generated source ends in exactly one newline. Some renderers use
    # line arrays while others use templates, so normalize at the one shared
    # write boundary instead of relying on each renderer to trim itself.
    path.write_text(text.rstrip("\n") + "\n", encoding="utf-8")
    if path.name.lower() != "readme.md":
        path.chmod(path.stat().st_mode & ~0o222)


def write_outputs(map_path: Path, out_dir: Path) -> dict[str, Path]:
    doc = json.loads(map_path.read_text(encoding="utf-8"))
    service = doc["service"]
    mapping = doc["map"]
    stem = map_path.name.replace(".route-map.json", "").replace("-", "_")
    ts_dir = out_dir / "typescript"
    dart_dir = out_dir / "dart" / "lib"
    rust_dir = out_dir / "rust" / "src"
    gleam_dir = out_dir / "gleam" / "src"
    ts_dir.mkdir(parents=True, exist_ok=True)
    dart_dir.mkdir(parents=True, exist_ok=True)
    rust_dir.mkdir(parents=True, exist_ok=True)
    gleam_dir.mkdir(parents=True, exist_ok=True)
    files = {
        "ts": ts_dir / f"{stem}.ts",
        "dart": dart_dir / f"{stem}.dart",
        "rust": rust_dir / f"{stem}.rs",
        "gleam": gleam_dir / f"{stem}.gleam",
    }
    write_readonly(files["ts"], gen_typescript(service, mapping))
    write_readonly(files["dart"], gen_dart(service, mapping))
    write_readonly(files["rust"], gen_rust(service, mapping))
    write_readonly(files["gleam"], gen_gleam(service, mapping))
    return files


def schema_version_of(map_path: Path) -> str:
    doc = json.loads(map_path.read_text(encoding="utf-8"))
    if not isinstance(doc, dict):
        return ""
    return str(doc.get("schema_version") or "")


def is_v1_route_map(map_path: Path) -> bool:
    """RIDL v2 maps (`schema_version` 2.x) are generated by `ridl`, not this script."""
    version = schema_version_of(map_path)
    return not version.startswith("2.")


def default_maps() -> list[Path]:
    return [path for path in sorted((ROOT / "examples").glob("*.route-map.json")) if is_v1_route_map(path)]


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--map", action="append", dest="maps", default=[])
    parser.add_argument("--out", type=Path, default=ROOT / "generated")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    maps = [Path(p) for p in args.maps] if args.maps else default_maps()
    skipped_v2 = [
        path for path in maps if not is_v1_route_map(path if path.is_absolute() else ROOT / path)
    ]
    maps = [path for path in maps if is_v1_route_map(path if path.is_absolute() else ROOT / path)]
    for path in skipped_v2:
        print(
            f"note: skipping RIDL v2 map {path} (use python3 -m ridl.cli generate)",
            file=sys.stderr,
        )
    if not maps:
        parser.error("no maps")
    out = args.out
    if args.check:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            for item in maps:
                write_outputs(item if item.is_absolute() else ROOT / item, tmp_path)
            drift = []
            for produced in tmp_path.rglob("*"):
                if not produced.is_file():
                    continue
                rel = produced.relative_to(tmp_path)
                existing = out / rel
                if (
                    not existing.is_file()
                    or existing.read_text(encoding="utf-8") != produced.read_text(encoding="utf-8")
                ):
                    drift.append(str(rel))
            if drift:
                print("generated routes are stale:", file=sys.stderr)
                for item in drift:
                    print(f"  - {item}", file=sys.stderr)
                print("run: python3 scripts/generate-routes.py", file=sys.stderr)
                return 1
        print("generated routes ok")
        return 0
    for item in maps:
        files = write_outputs(item if item.is_absolute() else ROOT / item, out)
        for kind, path in files.items():
            print(f"wrote {kind}: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
