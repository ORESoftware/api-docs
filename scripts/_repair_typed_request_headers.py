#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def replace_exact(text: str, old: str, new: str, label: str, expected: int = 1) -> str:
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{label}: expected {expected} occurrence(s), found {count}")
    return text.replace(old, new)


def pre() -> None:
    source = Path("scripts/_apply_typed_request_headers.py")
    lines = source.read_text().splitlines()
    repaired = 0
    marker = 'default = "" if any(p.required for p in route.query_params) else '
    for index, line in enumerate(lines):
        if marker in line and line.endswith("else \\"):
            lines[index] = line + "\\"
            repaired += 1
    if repaired != 2:
        raise SystemExit(f"expected two Python emitter patch literals, repaired {repaired}")
    source.write_text("\n".join(lines) + "\n")


def post() -> None:
    test = Path("scripts/test_typed_request_headers.py")
    test_lines = test.read_text().splitlines()
    if "import sys" not in test_lines:
        test_lines.insert(test_lines.index("import unittest"), "import sys")
        validator_index = test_lines.index("from jsonschema import Draft202012Validator")
        test_lines[validator_index:validator_index] = [
            "",
            "ROOT = Path(__file__).resolve().parents[1]",
            "sys.path.insert(0, str(ROOT))",
            "",
        ]
        test.write_text("\n".join(test_lines) + "\n")

    emitter = Path("ridl/emit/python.py")
    lines = emitter.read_text().splitlines()
    function_start = lines.index(
        "def _emit_call_fn(rmap: RouteMap, route: Route, w: Writer) -> None:"
    )
    query_start = lines.index("    if route.query_params:", function_start)
    header_start = lines.index("    if route.header_params:", query_start)
    body_start = lines.index("    if route.request is not None:", header_start)
    result_start = next(
        index
        for index in range(body_start + 1, len(lines))
        if lines[index].startswith("    ret = ")
    )
    if not (function_start < query_start < header_start < body_start < result_start):
        raise SystemExit("generated Python call arguments do not have the expected shape")
    query_block = lines[query_start:header_start]
    header_block = lines[header_start:body_start]
    body_block = lines[body_start:result_start]
    lines[query_start:result_start] = body_block + header_block + query_block
    emitter.write_text("\n".join(lines) + "\n")

    dart = Path("ridl/emit/dart.py")
    dart_text = dart.read_text()
    old = '''                    else:
                        with w.block(f"if ({ident} != null)"):
                            if is_list:
                                with w.block(f"for (final item in {ident})"):
                                    w.line(
                                        f"pairs.add(MapEntry({json.dumps(param.wire)}, "
                                        f"_queryValue({inner})));"
                                    )
                            else:
                                w.line(
                                    f"pairs.add(MapEntry({json.dumps(param.wire)}, "
                                    f"_queryValue({inner})));"
                                )
'''
    new = '''                    else:
                        if is_list:
                            w.line(f"final {ident}Values = {ident};")
                            with w.block(f"if ({ident}Values != null)"):
                                with w.block(f"for (final item in {ident}Values)"):
                                    w.line(
                                        f"pairs.add(MapEntry({json.dumps(param.wire)}, "
                                        f"_queryValue({inner})));"
                                    )
                        else:
                            with w.block(f"if ({ident} != null)"):
                                w.line(
                                    f"pairs.add(MapEntry({json.dumps(param.wire)}, "
                                    f"_queryValue({inner})));"
                                )
'''
    count = dart_text.count(old)
    if count != 2:
        raise SystemExit(f"expected two nullable Dart list emitter blocks, found {count}")
    dart.write_text(dart_text.replace(old, new))

    gleam = Path("ridl/emit/gleam.py")
    gleam_text = gleam.read_text()
    helper_old = '''def _param_ident(p: Param) -> str:
    return naming.escape(naming.snake(p.wire), LANG)


def _emit_operations'''
    helper_new = '''def _param_ident(p: Param) -> str:
    return naming.escape(naming.snake(p.wire), LANG)


def _wire_value(rmap: RouteMap, expr: TypeExpr, src: str) -> str:
    """Canonical HTTP field/query serialization without debug-string quoting."""
    target = rmap.underlying(expr)
    if isinstance(target, Named):
        if target.name in {"String", "Uuid", "DateTime", "Decimal"}:
            return src
        if target.name in {"Bool", "I32", "I64", "F64"}:
            return f"json.to_string({encoder(rmap, target, src)})"
        defn = rmap.types.get(target.name)
        if isinstance(defn, ScalarDef):
            if defn.base in {"String", "Uuid", "DateTime", "Decimal"}:
                return src
            if defn.base in {"Bool", "I32", "I64", "F64"}:
                return f"json.to_string({encoder(rmap, target, src)})"
        if isinstance(defn, EnumDef):
            return f"{naming.snake(target.name)}_to_wire({src})"
    return f"string.inspect({src})"


def _emit_operations'''
    gleam_text = replace_exact(
        gleam_text, helper_old, helper_new, "Gleam wire-value helper"
    )

    query_old = '''        if route.query_params:
            w.line("let query =")
            w.indent()
            w.line("[")
            w.indent()
            for param in route.query_params:
                ident = _param_ident(param)
                key = json.dumps(param.wire)
                is_list = isinstance(rmap.underlying(param.type), ListOf)
                if is_list:
                    w.line(
                        f"..list.map({ident}, fn(item) {{ #({key}, string.inspect(item)) }})"
                    )
                elif param.required:
                    w.line(f"#({key}, string.inspect({ident})),")
                else:
                    w.line(
                        f"..case {ident} {{ option.Some(v) -> [#({key}, string.inspect(v))] "
                        f"option.None -> [] }}"
                    )
            w.dedent()
            w.line("]")
            w.dedent()
        else:
            w.line("let query = []")
'''
    query_new = '''        if route.query_params:
            w.line("let query =")
            w.indent()
            w.line("list.concat([")
            w.indent()
            for param in route.query_params:
                ident = _param_ident(param)
                key = json.dumps(param.wire)
                target = rmap.underlying(param.type)
                is_list = isinstance(target, ListOf)
                if is_list and param.required:
                    value = _wire_value(rmap, target.item, "item")
                    w.line(f"list.map({ident}, fn(item) {{ #({key}, {value}) }}),")
                elif is_list:
                    value = _wire_value(rmap, target.item, "item")
                    w.line(
                        f"case {ident} {{ option.Some(values) -> "
                        f"list.map(values, fn(item) {{ #({key}, {value}) }}) "
                        f"option.None -> [] }},"
                    )
                elif param.required:
                    value = _wire_value(rmap, param.type, ident)
                    w.line(f"[#({key}, {value})],")
                else:
                    value = _wire_value(rmap, param.type, "v")
                    w.line(
                        f"case {ident} {{ option.Some(v) -> [#({key}, {value})] "
                        f"option.None -> [] }},"
                    )
            w.dedent()
            w.line("])")
            w.dedent()
        else:
            w.line("let query = []")
'''
    gleam_text = replace_exact(
        gleam_text, query_old, query_new, "Gleam query pair generation"
    )

    headers_old = '''        if route.header_params:
            w.line("let headers =")
            w.indent()
            w.line("[")
            w.indent()
            for param in route.header_params:
                ident = _param_ident(param)
                key = json.dumps(param.wire)
                is_list = isinstance(rmap.underlying(param.type), ListOf)
                if is_list:
                    w.line(
                        f"..list.map({ident}, fn(item) {{ #({key}, string.inspect(item)) }})"
                    )
                elif param.required:
                    w.line(f"#({key}, string.inspect({ident})),")
                else:
                    w.line(
                        f"..case {ident} {{ option.Some(v) -> [#({key}, string.inspect(v))] "
                        f"option.None -> [] }}"
                    )
            w.dedent()
            w.line("]")
            w.dedent()
        else:
            w.line("let headers = []")
'''
    headers_new = '''        if route.header_params:
            w.line("let headers =")
            w.indent()
            w.line("list.concat([")
            w.indent()
            for param in route.header_params:
                ident = _param_ident(param)
                key = json.dumps(param.wire)
                target = rmap.underlying(param.type)
                is_list = isinstance(target, ListOf)
                if is_list and param.required:
                    value = _wire_value(rmap, target.item, "item")
                    w.line(f"list.map({ident}, fn(item) {{ #({key}, {value}) }}),")
                elif is_list:
                    value = _wire_value(rmap, target.item, "item")
                    w.line(
                        f"case {ident} {{ option.Some(values) -> "
                        f"list.map(values, fn(item) {{ #({key}, {value}) }}) "
                        f"option.None -> [] }},"
                    )
                elif param.required:
                    value = _wire_value(rmap, param.type, ident)
                    w.line(f"[#({key}, {value})],")
                else:
                    value = _wire_value(rmap, param.type, "v")
                    w.line(
                        f"case {ident} {{ option.Some(v) -> [#({key}, {value})] "
                        f"option.None -> [] }},"
                    )
            w.dedent()
            w.line("])")
            w.dedent()
        else:
            w.line("let headers = []")
'''
    gleam_text = replace_exact(
        gleam_text, headers_old, headers_new, "Gleam header pair generation"
    )
    gleam.write_text(gleam_text)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=("pre", "post"))
    args = parser.parse_args()
    pre() if args.phase == "pre" else post()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
