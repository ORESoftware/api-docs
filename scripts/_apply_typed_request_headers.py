#!/usr/bin/env python3
from __future__ import annotations

import copy
import json
import re
from pathlib import Path

ROOT = Path.cwd()


def path(rel: str) -> Path:
    return ROOT / rel


def read(rel: str) -> str:
    return path(rel).read_text(encoding="utf-8")


def write(rel: str, text: str) -> None:
    p = path(rel)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def replace_once(rel: str, old: str, new: str) -> None:
    text = read(rel)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{rel}: expected one occurrence, found {count}: {old[:100]!r}")
    write(rel, text.replace(old, new, 1))


def add_json_route_property(rel: str, query_name: str, header_name: str) -> None:
    data = json.loads(read(rel))
    changed = 0

    def visit(value):
        nonlocal changed
        if isinstance(value, dict):
            if query_name in value and header_name not in value and isinstance(value[query_name], dict):
                cloned = copy.deepcopy(value[query_name])
                cloned["description"] = (
                    "Typed request headers used only for validation and serialization. "
                    "They never participate in operation selection, which is method + path only."
                )
                value[header_name] = cloned
                changed += 1
            for child in list(value.values()):
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(data)
    if changed != 1:
        raise SystemExit(f"{rel}: expected one {query_name!r} contract location, changed {changed}")
    write(rel, json.dumps(data, indent=2, ensure_ascii=False) + "\n")


add_json_route_property(
    "json-schema/route-map.schema.json", "query_schema", "header_schema"
)
add_json_route_property(
    "json-schema/route-map-v2.schema.json", "query_params", "header_params"
)

replace_once(
    "ridl/model.py",
    "    query_params: list[Param] = field(default_factory=list)\n    request: TypeExpr | None = None",
    "    query_params: list[Param] = field(default_factory=list)\n"
    "    header_params: list[Param] = field(default_factory=list)\n"
    "    request: TypeExpr | None = None",
)
replace_once(
    "ridl/model.py",
    '    qp_raw = raw.get("query_params") if isinstance(raw.get("query_params"), dict) else {}\n\n    wildcards = wildcard_params_in(path)',
    '    qp_raw = raw.get("query_params") if isinstance(raw.get("query_params"), dict) else {}\n'
    '    hp_raw = raw.get("header_params") if isinstance(raw.get("header_params"), dict) else {}\n\n'
    '    wildcards = wildcard_params_in(path)',
)
replace_once(
    "ridl/model.py",
    '        query_params=[_parse_param(k, v, f"{where}.query_params.{k}") for k, v in qp_raw.items()],\n'
    '        request=_parse_body(raw.get("request"), f"{where}.request"),',
    '        query_params=[_parse_param(k, v, f"{where}.query_params.{k}") for k, v in qp_raw.items()],\n'
    '        header_params=[_parse_param(k, v, f"{where}.header_params.{k}") for k, v in hp_raw.items()],\n'
    '        request=_parse_body(raw.get("request"), f"{where}.request"),',
)

replace_once(
    "ridl/validate.py",
    "from __future__ import annotations\n\nfrom .model import (",
    "from __future__ import annotations\n\nimport re\n\nfrom .model import (",
)
replace_once(
    "ridl/validate.py",
    "OPTO_MAX_PAYLOAD_BYTES = 255 * 1024\n",
    '''OPTO_MAX_PAYLOAD_BYTES = 255 * 1024

HTTP_HEADER_NAME_RE = re.compile(r"^[!#$%&'*+.^_`|~0-9a-z-]+$")
RUNTIME_OWNED_REQUEST_HEADERS = frozenset({
    "authorization",
    "baggage",
    "connection",
    "content-encoding",
    "content-length",
    "content-type",
    "cookie",
    "forwarded",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "set-cookie",
    "te",
    "traceparent",
    "tracestate",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
})
''',
)
replace_once(
    "ridl/validate.py",
    "    errors += _validate_query_params(rmap, route, where)\n    errors += _validate_bodies(rmap, route, where)",
    "    errors += _validate_query_params(rmap, route, where)\n"
    "    errors += _validate_header_params(rmap, route, where)\n"
    "    errors += _validate_bodies(rmap, route, where)",
)
replace_once(
    "ridl/validate.py",
    '''    if route.query_params and set(route.transports) == {TRANSPORT_NATS}:
        errors.append(
            f"{where}: query parameters have no NATS encoding; add http or tcp, or "
            f"move them into the request body"
        )

    return errors
''',
    '''    if route.query_params and set(route.transports) == {TRANSPORT_NATS}:
        errors.append(
            f"{where}: query parameters have no NATS encoding; add http or tcp, or "
            f"move them into the request body"
        )

    if route.header_params and set(route.transports) != {TRANSPORT_HTTP}:
        errors.append(
            f"{where}: request headers are HTTP-only; use an HTTP-only operation or "
            f"move cross-transport metadata into the request body"
        )

    return errors
''',
)
query_validator_marker = '''def _validate_bodies(rmap: RouteMap, route: Route, where: str) -> list[str]:
'''
header_validator = '''def _validate_header_params(rmap: RouteMap, route: Route, where: str) -> list[str]:
    """Validate application-owned headers without turning them into dispatch keys.

    Header names are canonical lower-case HTTP tokens. Authentication, tracing,
    framing, proxy, and hop-by-hop headers remain owned by middleware/runtime
    adapters and cannot be declared by business route contracts.
    """
    errors: list[str] = []
    seen: set[str] = set()
    for param in route.header_params:
        pwhere = f"{where}.header_params.{param.wire}"
        if param.wire in seen:
            errors.append(f"{pwhere}: duplicate request header")
        seen.add(param.wire)
        if not HTTP_HEADER_NAME_RE.fullmatch(param.wire):
            errors.append(
                f"{pwhere}: header name must be one canonical lower-case HTTP token"
            )
        if param.wire in RUNTIME_OWNED_REQUEST_HEADERS:
            errors.append(
                f"{pwhere}: header is owned by authentication, tracing, proxy, or HTTP framing middleware"
            )
        errors += _validate_refs(rmap, param.type, pwhere)
        target = rmap.underlying(param.type)
        inner = target.item if isinstance(target, ListOf) else target
        if isinstance(target, MapOf) or not rmap.is_scalar_like(inner):
            errors.append(
                f"{pwhere}: request headers must be scalar, enum, or a list of those -- "
                f"{param.type} has no canonical HTTP field-value encoding"
            )
    return errors


'''
replace_once(
    "ridl/validate.py", query_validator_marker, header_validator + query_validator_marker
)

replace_once(
    "ridl/emit/typescript.py",
    '            "readonly query: ReadonlyArray<readonly [string, string]>;",\n            "/** JSON body, or undefined for operations that carry none. */",',
    '            "readonly query: ReadonlyArray<readonly [string, string]>;",\n'
    '            "/** Canonical lower-case request headers. Never used for routing. */",\n'
    '            "readonly headers: ReadonlyArray<readonly [string, string]>;",\n'
    '            "/** JSON body, or undefined for operations that carry none. */",',
)
replace_once(
    "ridl/emit/typescript.py",
    '''        w.blank()

    for route in client_routes(rmap):
        _emit_path_fn(rmap, route, w)
''',
    '''        w.blank()

    for route in client_routes(rmap):
        if not route.header_params:
            continue
        w.line(f"/** Typed request headers for `{route.key}`; never routing selectors. */")
        with w.block(f"export interface {naming.pascal(route.key)}Headers"):
            for param in route.header_params:
                if param.doc:
                    w.line("/** " + str(param.doc) + " */")
                optional = "" if param.required else "?"
                w.line(
                    f"readonly {json.dumps(param.wire)}{optional}: "
                    f"{type_name(rmap, param.type)};"
                )
        w.blank()

    for route in client_routes(rmap):
        _emit_path_fn(rmap, route, w)
''',
)
replace_once(
    "ridl/emit/typescript.py",
    '''    if route.query_params:
        required_query = any(p.required for p in route.query_params)
        suffix = "" if required_query else " = {}"
        args.append(f"queryParams: {naming.pascal(route.key)}Query{suffix}")
    if route.request is not None:
        args.append(f"bodyValue: {type_name(rmap, route.request)}")
''',
    '''    if route.query_params:
        required_query = any(p.required for p in route.query_params)
        suffix = "" if required_query else " = {}"
        args.append(f"queryParams: {naming.pascal(route.key)}Query{suffix}")
    if route.header_params:
        args.append(f"headerParams: {naming.pascal(route.key)}Headers")
    if route.request is not None:
        args.append(f"bodyValue: {type_name(rmap, route.request)}")
''',
)
replace_once(
    "ridl/emit/typescript.py",
    '''        if route.request is not None:
            w.line("const body = JSON.stringify(bodyValue);")
        w.line("const raw = await transport.call({")
''',
    '''        w.line("const headers: Array<readonly [string, string]> = [];")
        for param in route.header_params:
            key = json.dumps(param.wire)
            src = f"headerParams[{key}]"
            is_list = isinstance(rmap.underlying(param.type), ListOf)
            guard = f"if ({src} !== undefined && {src} !== null)"
            if is_list:
                with w.block(guard):
                    with w.block(f"for (const item of {src})"):
                        w.line(f"headers.push([{key}, queryValue(item)] as const);")
            else:
                with w.block(guard):
                    w.line(f"headers.push([{key}, queryValue({src})] as const);")

        if route.request is not None:
            w.line("const body = JSON.stringify(bodyValue);")
        w.line("const raw = await transport.call({")
''',
)
replace_once(
    "ridl/emit/typescript.py",
    '            "query,",\n        )',
    '            "query,",\n            "headers,",\n        )',
)

replace_once(
    "ridl/emit/rust.py",
    '            "pub query: Vec<(String, String)>,",\n            "/// JSON body, or `None` for operations that carry none.",',
    '            "pub query: Vec<(String, String)>,",\n'
    '            "/// Canonical lower-case request headers. Never used for routing.",\n'
    '            "pub headers: Vec<(String, String)>,",\n'
    '            "/// JSON body, or `None` for operations that carry none.",',
)
replace_once(
    "ridl/emit/rust.py",
    '''def _query_struct(route: Route) -> str:
    return f"{naming.pascal(route.key)}Query"


def _emit_operations''',
    '''def _query_struct(route: Route) -> str:
    return f"{naming.pascal(route.key)}Query"


def _headers_struct(route: Route) -> str:
    return f"{naming.pascal(route.key)}Headers"


def _emit_operations''',
)
replace_once(
    "ridl/emit/rust.py",
    '''            w.blank()

    for route in client_routes(rmap):
        _emit_path_fn(rmap, route, w)
''',
    '''            w.blank()

    for route in client_routes(rmap):
        if route.header_params:
            w.doc(f"Typed request headers for `{route.key}`; never routing selectors.", "///")
            w.line("#[derive(Clone, Debug, Default, Serialize)]")
            with w.block(f"pub struct {_headers_struct(route)}"):
                for param in route.header_params:
                    w.doc(param.doc, "///")
                    w.line(
                        f"pub {_param_ident(param)}: "
                        f"{field_type(rmap, param.type, param.required)},"
                    )
            w.blank()

    for route in client_routes(rmap):
        _emit_path_fn(rmap, route, w)
''',
)
replace_once(
    "ridl/emit/rust.py",
    '''    if route.query_params:
        args.append(f"query: &{_query_struct(route)}")
    if route.request is not None:
        args.append(f"body: &{type_name(rmap, route.request)}")
''',
    '''    if route.query_params:
        args.append(f"query: &{_query_struct(route)}")
    if route.header_params:
        args.append(f"headers: &{_headers_struct(route)}")
    if route.request is not None:
        args.append(f"body: &{type_name(rmap, route.request)}")
''',
)
replace_once(
    "ridl/emit/rust.py",
    '''        else:
            w.line("let query_pairs: Vec<(String, String)> = Vec::new();")

        if route.request is not None:
''',
    '''        else:
            w.line("let query_pairs: Vec<(String, String)> = Vec::new();")

        if route.header_params:
            w.line("let mut header_pairs: Vec<(String, String)> = Vec::new();")
            for param in route.header_params:
                ident = _param_ident(param)
                is_list = isinstance(rmap.underlying(param.type), ListOf)
                if param.required:
                    if is_list:
                        with w.block(f"for item in &headers.{ident}"):
                            w.line(
                                f'header_pairs.push(("{param.wire}".to_string(), '
                                f"query_value(item)));"
                            )
                    else:
                        w.line(
                            f'header_pairs.push(("{param.wire}".to_string(), '
                            f"query_value(&headers.{ident})));"
                        )
                else:
                    with w.block(f"if let Some(value) = &headers.{ident}"):
                        if is_list:
                            with w.block("for item in value"):
                                w.line(
                                    f'header_pairs.push(("{param.wire}".to_string(), '
                                    f"query_value(item)));"
                                )
                        else:
                            w.line(
                                f'header_pairs.push(("{param.wire}".to_string(), '
                                f"query_value(value)));"
                            )
        else:
            w.line("let header_pairs: Vec<(String, String)> = Vec::new();")

        if route.request is not None:
''',
)
replace_once(
    "ridl/emit/rust.py",
    '            "query: query_pairs,",\n            "body,",',
    '            "query: query_pairs,",\n            "headers: header_pairs,",\n            "body,",',
)

replace_once(
    "ridl/emit/go.py",
    '''    w.line("// RPCRequest is one outbound call, fully resolved.")
    with w.block("type RPCRequest struct"):
''',
    '''    w.line("// HeaderPair is one canonical lower-case HTTP request header.")
    with w.block("type HeaderPair struct"):
        w.lines("Key string", "Value string")
    w.blank()
    w.line("// RPCRequest is one outbound call, fully resolved.")
    with w.block("type RPCRequest struct"):
''',
)
replace_once(
    "ridl/emit/go.py",
    '            "PathTemplate string", "Query []QueryPair",\n            "// Body is the JSON payload, or nil for operations that carry none.",',
    '            "PathTemplate string", "Query []QueryPair",\n'
    '            "// Headers validate request metadata but never select an operation.",\n'
    '            "Headers []HeaderPair",\n'
    '            "// Body is the JSON payload, or nil for operations that carry none.",',
)
replace_once(
    "ridl/emit/go.py",
    '''        w.blank()

    for route in client_routes(rmap):
        fn = f"{naming.pascal(route.key)}Path"
''',
    '''        w.blank()

    for route in client_routes(rmap):
        if not route.header_params:
            continue
        cls = f"{naming.pascal(route.key)}Headers"
        w.line(f"// {cls} holds typed request headers for {route.key}; they never route.")
        with w.block(f"type {cls} struct"):
            for param in route.header_params:
                w.doc(param.doc, "//")
                w.line(
                    f"{naming.pascal(param.wire)} "
                    f"{field_type(rmap, param.type, param.required)}"
                )
        w.blank()
        with w.block(f"func (h {cls}) Pairs() []HeaderPair"):
            w.line("pairs := []HeaderPair{}")
            for param in route.header_params:
                fld = f"h.{naming.pascal(param.wire)}"
                key = json.dumps(param.wire)
                is_list = isinstance(rmap.underlying(param.type), ListOf)
                if is_list:
                    with w.block(f"for _, item := range {fld}"):
                        w.line(f"pairs = append(pairs, HeaderPair{{{key}, queryValue(item)}})")
                elif param.required:
                    w.line(f"pairs = append(pairs, HeaderPair{{{key}, queryValue({fld})}})")
                else:
                    with w.block(f"if {fld} != nil"):
                        w.line(f"pairs = append(pairs, HeaderPair{{{key}, queryValue(*{fld})}})")
            w.line("return pairs")
        w.blank()

    for route in client_routes(rmap):
        fn = f"{naming.pascal(route.key)}Path"
''',
)
replace_once(
    "ridl/emit/go.py",
    '''    if route.query_params:
        args.append(f"queryParams {naming.pascal(route.key)}Query")
    if route.request is not None:
        args.append(f"bodyValue {type_name(rmap, route.request)}")
''',
    '''    if route.query_params:
        args.append(f"queryParams {naming.pascal(route.key)}Query")
    if route.header_params:
        args.append(f"headerParams {naming.pascal(route.key)}Headers")
    if route.request is not None:
        args.append(f"bodyValue {type_name(rmap, route.request)}")
''',
)
replace_once(
    "ridl/emit/go.py",
    '        w.line("query := []QueryPair{}" if not route.query_params else "query := queryParams.Pairs()")\n'
    '        if route.request is not None:',
    '        w.line("query := []QueryPair{}" if not route.query_params else "query := queryParams.Pairs()")\n'
    '        w.line("headers := []HeaderPair{}" if not route.header_params else "headers := headerParams.Pairs()")\n'
    '        if route.request is not None:',
)
replace_once(
    "ridl/emit/go.py",
    '            "Query: query,", "Body: body,",',
    '            "Query: query,", "Headers: headers,", "Body: body,",',
)

replace_once(
    "ridl/emit/dart.py",
    '            "  this.query = const [],",\n            "  this.body,",',
    '            "  this.query = const [],",\n            "  this.headers = const [],",\n            "  this.body,",',
)
replace_once(
    "ridl/emit/dart.py",
    '            "final List<MapEntry<String, String>> query;",\n            "",\n            "/// JSON body, or null for operations that carry none.",',
    '            "final List<MapEntry<String, String>> query;",\n'
    '            "/// Canonical lower-case request headers. Never used for routing.",\n'
    '            "final List<MapEntry<String, String>> headers;",\n'
    '            "",\n            "/// JSON body, or null for operations that carry none.",',
)
replace_once(
    "ridl/emit/dart.py",
    '''        w.blank()

    for route in client_routes(rmap):
        _emit_path_fn(rmap, route, w)
''',
    '''        w.blank()

    for route in client_routes(rmap):
        if not route.header_params:
            continue
        cls = f"{naming.pascal(route.key)}Headers"
        w.line(f"/// Typed request headers for `{route.key}`; never routing selectors.")
        with w.block(f"class {cls}"):
            args = ", ".join(
                ("required " if p.required else "") + f"this.{_param_ident(p)}"
                for p in route.header_params
            )
            w.line(f"const {cls}({{{args}}});")
            w.blank()
            for param in route.header_params:
                w.doc(param.doc, "///")
                w.line(
                    f"final {field_type(rmap, param.type, param.required)} "
                    f"{_param_ident(param)};"
                )
            w.blank()
            with w.block("List<MapEntry<String, String>> toPairs()"):
                w.line("final pairs = <MapEntry<String, String>>[];")
                for param in route.header_params:
                    ident = _param_ident(param)
                    is_list = isinstance(rmap.underlying(param.type), ListOf)
                    inner = _to_json(
                        rmap,
                        rmap.underlying(param.type).item if is_list else param.type,
                        "item" if is_list else ident,
                        False,
                    )
                    if param.required:
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
                    else:
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
                w.line("return pairs;")
        w.blank()

    for route in client_routes(rmap):
        _emit_path_fn(rmap, route, w)
''',
)
replace_once(
    "ridl/emit/dart.py",
    '''    if route.query_params:
        args.append(f"{naming.pascal(route.key)}Query queryParams")
    if route.request is not None:
        args.append(f"{type_name(rmap, route.request)} bodyValue")
''',
    '''    if route.query_params:
        args.append(f"{naming.pascal(route.key)}Query queryParams")
    if route.header_params:
        args.append(f"{naming.pascal(route.key)}Headers headerParams")
    if route.request is not None:
        args.append(f"{type_name(rmap, route.request)} bodyValue")
''',
)
replace_once(
    "ridl/emit/dart.py",
    '''        if route.request is not None:
            w.line("final body = jsonEncode(bodyValue.toJson());")
        w.line("final raw = await transport.call(RpcRequest(")
''',
    '''        if route.header_params:
            w.line("final headers = headerParams.toPairs();")
        else:
            w.line("const headers = <MapEntry<String, String>>[];")
        if route.request is not None:
            w.line("final body = jsonEncode(bodyValue.toJson());")
        w.line("final raw = await transport.call(RpcRequest(")
''',
)
replace_once(
    "ridl/emit/dart.py",
    '            "query: query,",\n        )',
    '            "query: query,",\n            "headers: headers,",\n        )',
)

replace_once(
    "ridl/emit/gleam.py",
    '                "query: List(#(String, String)),",\n                "body: option.Option(String),",',
    '                "query: List(#(String, String)),",\n'
    '                "headers: List(#(String, String)),",\n'
    '                "body: option.Option(String),",',
)
replace_once(
    "ridl/emit/gleam.py",
    '''    for param in route.query_params:
        args.append(
            f"{_param_ident(param)} {_param_ident(param)}: "
            f"{field_type(rmap, param.type, param.required)}"
        )
    if route.request is not None:
''',
    '''    for param in route.query_params:
        args.append(
            f"{_param_ident(param)} {_param_ident(param)}: "
            f"{field_type(rmap, param.type, param.required)}"
        )
    for param in route.header_params:
        args.append(
            f"{_param_ident(param)} {_param_ident(param)}: "
            f"{field_type(rmap, param.type, param.required)}"
        )
    if route.request is not None:
''',
)
replace_once(
    "ridl/emit/gleam.py",
    '''        else:
            w.line("let query = []")
        if route.request is not None:
''',
    '''        else:
            w.line("let query = []")
        if route.header_params:
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
        if route.request is not None:
''',
)
replace_once(
    "ridl/emit/gleam.py",
    '            "query: query,", "body: body,",',
    '            "query: query,", "headers: headers,", "body: body,",',
)

replace_once(
    "ridl/emit/python.py",
    '            "query: tuple[tuple[str, str], ...] = ()",\n            "body: str | None = None",',
    '            "query: tuple[tuple[str, str], ...] = ()",\n'
    '            "headers: tuple[tuple[str, str], ...] = ()",\n'
    '            "body: str | None = None",',
)
replace_once(
    "ridl/emit/python.py",
    '''        w.blank()

    for route in client_routes(rmap):
        fn = naming.escape(naming.snake(f"{route.key}_path"), LANG)
''',
    '''        w.blank()

    for route in client_routes(rmap):
        if not route.header_params:
            continue
        cls = f"{naming.pascal(route.key)}Headers"
        w.line("@dataclass(frozen=True, slots=True)")
        with w.block(f"class {cls}:", "", ""):
            w.line(f'"""Typed request headers for `{route.key}`; never routing selectors."""')
            w.blank()
            ordered = [p for p in route.header_params if p.required]
            ordered += [p for p in route.header_params if not p.required]
            for param in ordered:
                ann = field_type(rmap, param.type, param.required)
                default = "" if param.required else " = None"
                w.line(f"{_param_ident(param)}: {ann}{default}")
            w.blank()
            with w.block("def pairs(self) -> tuple[tuple[str, str], ...]:", "", ""):
                w.line("out: list[tuple[str, str]] = []")
                for param in route.header_params:
                    ident = f"self.{_param_ident(param)}"
                    key = json.dumps(param.wire)
                    is_list = isinstance(rmap.underlying(param.type), ListOf)
                    with w.block(f"if {ident} is not None:", "", ""):
                        if is_list:
                            with w.block(f"for item in {ident}:", "", ""):
                                w.line(f"out.append(({key}, _query_value(item)))")
                        else:
                            w.line(f"out.append(({key}, _query_value({ident})))")
                w.line("return tuple(out)")
        w.blank()

    for route in client_routes(rmap):
        fn = naming.escape(naming.snake(f"{route.key}_path"), LANG)
''',
)
replace_once(
    "ridl/emit/python.py",
    '''    if route.query_params:
        default = "" if any(p.required for p in route.query_params) else \
            f" = {naming.pascal(route.key)}Query()"
        args.append(f"query: {naming.pascal(route.key)}Query{default}")
    if route.request is not None:
        args.append(f"body: {type_name(rmap, route.request)}")
''',
    '''    if route.query_params:
        default = "" if any(p.required for p in route.query_params) else \
            f" = {naming.pascal(route.key)}Query()"
        args.append(f"query: {naming.pascal(route.key)}Query{default}")
    if route.header_params:
        args.append(f"headers: {naming.pascal(route.key)}Headers")
    if route.request is not None:
        args.append(f"body: {type_name(rmap, route.request)}")
''',
)
replace_once(
    "ridl/emit/python.py",
    '        w.line("pairs = query.pairs()" if route.query_params else "pairs: tuple[tuple[str, str], ...] = ()")\n'
    '        if route.request is not None:',
    '        w.line("pairs = query.pairs()" if route.query_params else "pairs: tuple[tuple[str, str], ...] = ()")\n'
    '        w.line("header_pairs = headers.pairs()" if route.header_params else "header_pairs: tuple[tuple[str, str], ...] = ()")\n'
    '        if route.request is not None:',
)
replace_once(
    "ridl/emit/python.py",
    '            "path=path,", f"path_template={json.dumps(route.path)},", "query=pairs,",\n'
    '            "body=payload," if route.request is not None else "body=None,",',
    '            "path=path,", f"path_template={json.dumps(route.path)},", "query=pairs,",\n'
    '            "headers=header_pairs,",\n'
    '            "body=payload," if route.request is not None else "body=None,",',
)

replace_once(
    "ridl/emit/kotlin.py",
    '            "public val query: List<Pair<String, String>> = emptyList(),",\n            "public val body: String? = null,",',
    '            "public val query: List<Pair<String, String>> = emptyList(),",\n'
    '            "public val headers: List<Pair<String, String>> = emptyList(),",\n'
    '            "public val body: String? = null,",',
)
replace_once(
    "ridl/emit/kotlin.py",
    '''        w.blank()

    for route in client_routes(rmap):
        fn = naming.escape(naming.camel(f"{route.key}_path"), LANG)
''',
    '''        w.blank()

    for route in client_routes(rmap):
        if not route.header_params:
            continue
        cls = f"{naming.pascal(route.key)}Headers"
        w.line(f"/** Typed request headers for `{route.key}`; never routing selectors. */")
        with w.block(f"public data class {cls}", "(", ")"):
            for param in route.header_params:
                w.doc(param.doc, "///")
                ann = field_type(rmap, param.type, param.required)
                default = "" if param.required else " = null"
                w.line(f"public val {_param_ident(param)}: {ann}{default},")
        w.line("{")
        w.indent()
        with w.block("public fun pairs(): List<Pair<String, String>>"):
            w.line("val out = mutableListOf<Pair<String, String>>()")
            for param in route.header_params:
                ident = _param_ident(param)
                key = json.dumps(param.wire)
                is_list = isinstance(rmap.underlying(param.type), ListOf)
                if param.required:
                    if is_list:
                        w.line(f"{ident}.forEach {{ out += {key} to queryValue(it) }}")
                    else:
                        w.line(f"out += {key} to queryValue({ident})")
                else:
                    if is_list:
                        w.line(f"{ident}?.forEach {{ out += {key} to queryValue(it) }}")
                    else:
                        w.line(f"{ident}?.let {{ out += {key} to queryValue(it) }}")
            w.line("return out")
        w.dedent()
        w.line("}")
        w.blank()

    for route in client_routes(rmap):
        fn = naming.escape(naming.camel(f"{route.key}_path"), LANG)
''',
)
replace_once(
    "ridl/emit/kotlin.py",
    '''    if route.query_params:
        cls = f"{naming.pascal(route.key)}Query"
        default = "" if any(p.required for p in route.query_params) else f" = {cls}()"
        args.append(f"query: {cls}{default}")
    if route.request is not None:
        args.append(f"body: {type_name(rmap, route.request)}")
''',
    '''    if route.query_params:
        cls = f"{naming.pascal(route.key)}Query"
        default = "" if any(p.required for p in route.query_params) else f" = {cls}()"
        args.append(f"query: {cls}{default}")
    if route.header_params:
        args.append(f"headers: {naming.pascal(route.key)}Headers")
    if route.request is not None:
        args.append(f"body: {type_name(rmap, route.request)}")
''',
)
replace_once(
    "ridl/emit/kotlin.py",
    '        w.line("val pairs = query.pairs()" if route.query_params\n'
    '               else "val pairs = emptyList<Pair<String, String>>()")\n'
    '        if route.request is not None:',
    '        w.line("val pairs = query.pairs()" if route.query_params\n'
    '               else "val pairs = emptyList<Pair<String, String>>()")\n'
    '        w.line("val headerPairs = headers.pairs()" if route.header_params\n'
    '               else "val headerPairs = emptyList<Pair<String, String>>()")\n'
    '        if route.request is not None:',
)
replace_once(
    "ridl/emit/kotlin.py",
    '            "path = path,", f"pathTemplate = {json.dumps(route.path)},", "query = pairs,",\n'
    '            "body = payload," if route.request is not None else "body = null,",',
    '            "path = path,", f"pathTemplate = {json.dumps(route.path)},", "query = pairs,",\n'
    '            "headers = headerPairs,",\n'
    '            "body = payload," if route.request is not None else "body = null,",',
)

replace_once(
    "ridl/emit/swift.py",
    '            "public let query: [(String, String)]",\n            "public let body: Data?",',
    '            "public let query: [(String, String)]",\n'
    '            "public let headers: [(String, String)]",\n'
    '            "public let body: Data?",',
)
replace_once(
    "ridl/emit/swift.py",
    '            "pathTemplate: String = \\\"\\\", query: [(String, String)] = [], body: Data? = nil, "\n'
    '            "delivery: RidlDelivery = .direct, optoSync: RidlOptoSyncBinding? = nil) {",\n'
    '            "    self.key = key; self.method = method; self.path = path",\n'
    '            "    self.pathTemplate = pathTemplate",\n'
    '            "    self.query = query; self.body = body",',
    '            "pathTemplate: String = \\\"\\\", query: [(String, String)] = [], "\n'
    '            "headers: [(String, String)] = [], body: Data? = nil, "\n'
    '            "delivery: RidlDelivery = .direct, optoSync: RidlOptoSyncBinding? = nil) {",\n'
    '            "    self.key = key; self.method = method; self.path = path",\n'
    '            "    self.pathTemplate = pathTemplate",\n'
    '            "    self.query = query; self.headers = headers; self.body = body",',
)
replace_once(
    "ridl/emit/swift.py",
    '''        w.blank()

    for route in client_routes(rmap):
        fn = naming.escape(naming.camel(f"{route.key}_path"), LANG)
''',
    '''        w.blank()

    for route in client_routes(rmap):
        if not route.header_params:
            continue
        cls = f"{naming.pascal(route.key)}Headers"
        w.line(f"/// Typed request headers for `{route.key}`; never routing selectors.")
        with w.block(f"public struct {cls}"):
            for param in route.header_params:
                w.doc(param.doc, "///")
                w.line(
                    f"public let {_param_ident(param)}: "
                    f"{field_type(rmap, param.type, param.required)}"
                )
            w.blank()
            params = ", ".join(
                f"{_param_ident(p)}: {field_type(rmap, p.type, p.required)}"
                + ("" if p.required else " = nil")
                for p in route.header_params
            )
            with w.block(f"public init({params})"):
                for param in route.header_params:
                    w.line(f"self.{_param_ident(param)} = {_param_ident(param)}")
            w.blank()
            with w.block("public func pairs() -> [(String, String)]"):
                w.line("var out: [(String, String)] = []")
                for param in route.header_params:
                    ident = _param_ident(param)
                    key = json.dumps(param.wire)
                    is_list = isinstance(rmap.underlying(param.type), ListOf)
                    if param.required:
                        if is_list:
                            with w.block(f"for item in {ident}"):
                                w.line(f'out.append(({key}, "\\\\(item)"))')
                        else:
                            w.line(f'out.append(({key}, "\\\\({ident})"))')
                    else:
                        with w.block(f"if let value = {ident}"):
                            if is_list:
                                with w.block("for item in value"):
                                    w.line(f'out.append(({key}, "\\\\(item)"))')
                            else:
                                w.line(f'out.append(({key}, "\\\\(value)"))')
                w.line("return out")
        w.blank()

    for route in client_routes(rmap):
        fn = naming.escape(naming.camel(f"{route.key}_path"), LANG)
''',
)
replace_once(
    "ridl/emit/swift.py",
    '''    if route.query_params:
        args.append(f"query: {naming.pascal(route.key)}Query")
    if route.request is not None:
        args.append(f"body: {type_name(rmap, route.request)}")
''',
    '''    if route.query_params:
        args.append(f"query: {naming.pascal(route.key)}Query")
    if route.header_params:
        args.append(f"headers: {naming.pascal(route.key)}Headers")
    if route.request is not None:
        args.append(f"body: {type_name(rmap, route.request)}")
''',
)
replace_once(
    "ridl/emit/swift.py",
    '        w.line("let pairs = query.pairs()" if route.query_params\n'
    '               else "let pairs: [(String, String)] = []")\n'
    '        if route.request is not None:',
    '        w.line("let pairs = query.pairs()" if route.query_params\n'
    '               else "let pairs: [(String, String)] = []")\n'
    '        w.line("let headerPairs = headers.pairs()" if route.header_params\n'
    '               else "let headerPairs: [(String, String)] = []")\n'
    '        if route.request is not None:',
)
replace_once(
    "ridl/emit/swift.py",
    '            "path: path,", f"pathTemplate: {json.dumps(route.path)},", "query: pairs,",\n'
    '            "body: payload," if route.request is not None else "body: nil,",',
    '            "path: path,", f"pathTemplate: {json.dumps(route.path)},", "query: pairs,",\n'
    '            "headers: headerPairs,",\n'
    '            "body: payload," if route.request is not None else "body: nil,",',
)

replace_once(
    "ridl/emit/json_schema.py",
    '''    for name, schema in defs.items():
        doc = {
            "$schema": DRAFT,
            "$id": _id(rmap, f"{name}.schema.json"),
            "title": name,
            **schema,
        }
        out.append(
            Emitted(
                path=f"json-schema/{name}.schema.json",
                text=json.dumps(doc, indent=2) + "\\n",
            )
        )
    return out
''',
    '''    for name, schema in defs.items():
        doc = {
            "$schema": DRAFT,
            "$id": _id(rmap, f"{name}.schema.json"),
            "title": name,
            **schema,
        }
        out.append(
            Emitted(
                path=f"json-schema/{name}.schema.json",
                text=json.dumps(doc, indent=2) + "\\n",
            )
        )
    for route in rmap.routes:
        properties: dict[str, object] = {
            "method": {"enum": list(route.methods)},
            "pathTemplate": {"const": route.path},
        }
        required = ["method", "pathTemplate"]
        for name, params in (
            ("path", route.path_params),
            ("query", route.query_params),
            ("headers", route.header_params),
        ):
            if not params:
                continue
            properties[name] = _params_schema(rmap, params)
            if name == "path" or any(param.required for param in params):
                required.append(name)
        if route.request is not None:
            properties["body"] = _type_schema(rmap, route.request)
            required.append("body")
        operation = {
            "$schema": DRAFT,
            "$id": _id(rmap, f"operations/{route.key}.request.schema.json"),
            "title": f"{route.key} parsed request surface",
            "description": (
                "Validate parsed/coerced path, query, header, and JSON body values. "
                "Operation identity is method + pathTemplate only; request values never route."
            ),
            "type": "object",
            "additionalProperties": False,
            "required": required,
            "properties": properties,
            "$defs": defs,
            "x-ores-routing-identity": ["method", "pathTemplate"],
            "x-ores-validation-only": ["path", "query", "headers", "body"],
        }
        out.append(
            Emitted(
                path=f"json-schema/operations/{route.key}.request.schema.json",
                text=json.dumps(operation, indent=2) + "\\n",
            )
        )
    return out
''',
)
replace_once(
    "ridl/emit/json_schema.py",
    '''def _id(rmap: RouteMap, file: str) -> str:
''',
    '''def _params_schema(rmap: RouteMap, params: list[object]) -> dict:
    properties = {
        param.wire: _type_schema(rmap, param.type)
        for param in params
    }
    required = [param.wire for param in params if param.required]
    schema: dict[str, object] = {
        "type": "object",
        "additionalProperties": False,
        "properties": properties,
    }
    if required:
        schema["required"] = required
    return schema


def _id(rmap: RouteMap, file: str) -> str:
''',
)

replace_once(
    "scripts/rpc_contract/model.py",
    '    "query_schema",\n    "request_schema",',
    '    "query_schema",\n    "header_schema",\n    "request_schema",',
)
replace_once(
    "scripts/rpc_contract/model.py",
    '        ("query_schema", "querySchema"),\n        ("request_schema", "requestSchema"),',
    '        ("query_schema", "querySchema"),\n        ("header_schema", "headerSchema"),\n        ("request_schema", "requestSchema"),',
)
replace_once(
    "scripts/check-route-sync.py",
    'HTTP_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}\n',
    '''HTTP_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
HTTP_HEADER_NAME_RE = re.compile(r"^[!#$%&'*+.^_`|~0-9a-z-]+$")
RUNTIME_OWNED_REQUEST_HEADERS = {
    "authorization", "baggage", "connection", "content-encoding",
    "content-length", "content-type", "cookie", "forwarded", "host",
    "keep-alive", "proxy-authenticate", "proxy-authorization", "set-cookie",
    "te", "traceparent", "tracestate", "trailer", "transfer-encoding",
    "upgrade", "x-forwarded-for", "x-forwarded-host", "x-forwarded-proto",
    "x-real-ip",
}
''',
)
replace_once(
    "scripts/check-route-sync.py",
    '''            alias = value.get("alias_of")
            if isinstance(alias, str) and alias not in raw:
''',
    '''            header_schema = value.get("header_schema")
            if isinstance(header_schema, dict):
                if set(entry["transports"]) != {"http"}:
                    errors.append(
                        f"{label}.{key}: request headers are HTTP-only; use an HTTP-only operation"
                    )
                if header_schema.get("type") != "object":
                    errors.append(f"{label}.{key}: header_schema.type must be object")
                if header_schema.get("additionalProperties") is not False:
                    errors.append(
                        f"{label}.{key}: header_schema must set additionalProperties: false"
                    )
                properties = header_schema.get("properties")
                if not isinstance(properties, dict):
                    errors.append(f"{label}.{key}: header_schema needs properties")
                    properties = {}
                required = header_schema.get("required") or []
                if not isinstance(required, list) or not set(required).issubset(properties):
                    errors.append(
                        f"{label}.{key}: header_schema.required must name declared properties"
                    )
                for header_name in properties:
                    if not HTTP_HEADER_NAME_RE.fullmatch(header_name):
                        errors.append(
                            f"{label}.{key}.header_schema.{header_name}: header name must be canonical lower-case"
                        )
                    if header_name in RUNTIME_OWNED_REQUEST_HEADERS:
                        errors.append(
                            f"{label}.{key}.header_schema.{header_name}: runtime-owned header is forbidden"
                        )
            alias = value.get("alias_of")
            if isinstance(alias, str) and alias not in raw:
''',
)

replace_once(
    "scripts/generate-routes.py",
    '    companion: dict[str, tuple[str, str, str, str]] = {}',
    '    companion: dict[str, tuple[str, str, str, str, str]] = {}',
)
replace_once(
    "scripts/generate-routes.py",
    '        query_schema = obj.get("query_schema")\n        req_schema = obj.get("request_schema")',
    '        query_schema = obj.get("query_schema")\n        header_schema = obj.get("header_schema")\n        req_schema = obj.get("request_schema")',
)
replace_once(
    "scripts/generate-routes.py",
    '        query_t = ts_type(query_schema, "Record<string, never>") if query_schema else "Record<string, never>"\n'
    '        req_t = ts_type(req_schema, "unknown") if req_schema else "void"\n'
    '        res_t = ts_type(res_schema, "unknown") if res_schema else "unknown"\n'
    '        companion[key] = (path_t, query_t, req_t, res_t)',
    '        query_t = ts_type(query_schema, "Record<string, never>") if query_schema else "Record<string, never>"\n'
    '        header_t = ts_type(header_schema, "Record<string, never>") if header_schema else "Record<string, never>"\n'
    '        req_t = ts_type(req_schema, "unknown") if req_schema else "void"\n'
    '        res_t = ts_type(res_schema, "unknown") if res_schema else "unknown"\n'
    '        companion[key] = (path_t, query_t, header_t, req_t, res_t)',
)
replace_once(
    "scripts/generate-routes.py",
    '    for key, (path_t, query_t, req_t, res_t) in companion.items():\n'
    '        lines.append(\n'
    '            f\'  {json.dumps(key)}: {{ path: {path_t}; query: {query_t}; body: {req_t}; response: {res_t} }};\'\n'
    '        )',
    '    for key, (path_t, query_t, header_t, req_t, res_t) in companion.items():\n'
    '        lines.append(\n'
    '            f\'  {json.dumps(key)}: {{ path: {path_t}; query: {query_t}; headers: {header_t}; body: {req_t}; response: {res_t} }};\'\n'
    '        )',
)
replace_once(
    "scripts/generate-routes.py",
    '            \'    query: RouteTypes[K]["query"];\',\n            \'    body: RouteTypes[K]["body"];\',',
    '            \'    query: RouteTypes[K]["query"];\',\n'
    '            \'    headers: RouteTypes[K]["headers"];\',\n'
    '            \'    body: RouteTypes[K]["body"];\',',
)
replace_once(
    "scripts/generate-routes.py",
    '        if isinstance(obj.get("query_schema"), dict) and obj["query_schema"].get("properties"):\n'
    '            structs.append(rust_struct(f"{var}Query", obj["query_schema"]))\n'
    '        if isinstance(obj.get("request_schema"), dict)',
    '        if isinstance(obj.get("query_schema"), dict) and obj["query_schema"].get("properties"):\n'
    '            structs.append(rust_struct(f"{var}Query", obj["query_schema"]))\n'
    '        if isinstance(obj.get("header_schema"), dict) and obj["header_schema"].get("properties"):\n'
    '            structs.append(rust_struct(f"{var}Headers", obj["header_schema"]))\n'
    '        if isinstance(obj.get("request_schema"), dict)',
)

replace_once(
    "scripts/rpc_contract/projections.py",
    '''            for location, schema_field in (
                ("path", "pathParams"),
                ("query", "querySchema"),
            ):
''',
    '''            for location, schema_field in (
                ("path", "pathParams"),
                ("query", "querySchema"),
                ("header", "headerSchema"),
            ):
''',
)
replace_once(
    "scripts/rpc_contract/projections.py",
    '''        for schema_field in ("pathParams", "querySchema"):
            schema = op.get(schema_field)
''',
    '''        for location, schema_field in (
            ("path", "pathParams"),
            ("query", "querySchema"),
            ("header", "headerSchema"),
        ):
            schema = op.get(schema_field)
''',
)
replace_once(
    "scripts/rpc_contract/projections.py",
    '''                        "required": (
                            schema_field == "pathParams" or name in required
                        ),
                        "schema": property_schema,
''',
    '''                        "required": (
                            schema_field == "pathParams" or name in required
                        ),
                        "schema": property_schema,
                        "x-ores-location": location,
''',
)
replace_once(
    "scripts/rpc_contract/projections.py",
    '''            if "pathParams" in op:
                link["hrefSchema"] = op["pathParams"]
            links.append(link)
''',
    '''            if "pathParams" in op:
                link["hrefSchema"] = op["pathParams"]
            if "headerSchema" in op:
                link["x-ores-header-schema"] = op["headerSchema"]
            links.append(link)
''',
)

write(
    "idl/typespec/http-request-surface.tsp",
    '''// Human-authored TypeSpec peer for parsed HTTP request validation.
// JSON Schema peer: json-schema/http-request-surface.schema.json.
// Operation selection is method + pathTemplate only. All remaining fields are
// validation inputs and may never select an operation.

namespace Ores.Http.RequestSurface.V1;

enum HttpMethod {
  GET,
  POST,
  PUT,
  PATCH,
  DELETE,
  HEAD,
  OPTIONS,
}

model RequestSurface {
  method: HttpMethod;

  @minLength(1)
  @pattern("^/[^\\\\s]*$")
  pathTemplate: string;

  path?: Record<unknown>;
  query?: Record<unknown>;
  headers?: Record<unknown>;
  body?: unknown;
}
''',
)
write(
    "json-schema/http-request-surface.schema.json",
    json.dumps(
        {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://github.com/oresoftware/api-docs/raw/main/json-schema/http-request-surface.schema.json",
            "title": "ORES parsed HTTP request surface",
            "description": (
                "Generic parsed request envelope. Operation identity is method + pathTemplate only; "
                "path, query, headers, and body are validation inputs, never routing selectors."
            ),
            "type": "object",
            "additionalProperties": False,
            "required": ["method", "pathTemplate"],
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
                },
                "pathTemplate": {
                    "type": "string",
                    "minLength": 1,
                    "pattern": r"^/[^\s]*$",
                },
                "path": {"type": "object"},
                "query": {"type": "object"},
                "headers": {
                    "type": "object",
                    "propertyNames": {"pattern": r"^[!#$%&'*+.^_`|~0-9a-z-]+$"},
                },
                "body": True,
            },
            "x-ores-routing-identity": ["method", "pathTemplate"],
            "x-ores-validation-only": ["path", "query", "headers", "body"],
        },
        indent=2,
    )
    + "\n",
)
replace_once(
    "idl/typespec/main.tsp",
    'import "./docs-discovery.tsp";',
    'import "./docs-discovery.tsp";\nimport "./http-request-surface.tsp";',
)
write(
    "scripts/check-http-request-surface-authorities.py",
    '''#!/usr/bin/env python3
"""Fail closed when the independent TypeSpec/JSON Schema request peers drift."""
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_FIELDS = ("method", "pathTemplate", "path", "query", "headers", "body")
EXPECTED_REQUIRED = ("method", "pathTemplate")
EXPECTED_METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")


def audit(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    schema = json.loads((root / "json-schema/http-request-surface.schema.json").read_text())
    tsp = (root / "idl/typespec/http-request-surface.tsp").read_text()

    properties = tuple(schema.get("properties", {}).keys())
    if properties != EXPECTED_FIELDS:
        errors.append(f"JSON Schema fields {properties} != {EXPECTED_FIELDS}")
    if tuple(schema.get("required", ())) != EXPECTED_REQUIRED:
        errors.append("JSON Schema required fields must be method + pathTemplate")
    if schema.get("additionalProperties") is not False:
        errors.append("JSON Schema request envelope must be closed")
    if tuple(schema.get("x-ores-routing-identity", ())) != EXPECTED_REQUIRED:
        errors.append("routing identity must be method + pathTemplate only")
    if tuple(schema.get("properties", {}).get("method", {}).get("enum", ())) != EXPECTED_METHODS:
        errors.append("JSON Schema HTTP method enum drift")

    model = re.search(r"model\\s+RequestSurface\\s*\\{(?P<body>.*?)\\n\\}", tsp, re.S)
    if not model:
        errors.append("TypeSpec RequestSurface model not found")
    else:
        fields = tuple(re.findall(r"^\\s*([A-Za-z][A-Za-z0-9]*)(?:\\?)?:", model.group("body"), re.M))
        if fields != EXPECTED_FIELDS:
            errors.append(f"TypeSpec fields {fields} != {EXPECTED_FIELDS}")
        optional = set(re.findall(r"^\\s*([A-Za-z][A-Za-z0-9]*)\\?:", model.group("body"), re.M))
        required = tuple(field for field in fields if field not in optional)
        if required != EXPECTED_REQUIRED:
            errors.append(f"TypeSpec required fields {required} != {EXPECTED_REQUIRED}")

    enum = re.search(r"enum\\s+HttpMethod\\s*\\{(?P<body>.*?)\\n\\}", tsp, re.S)
    if not enum:
        errors.append("TypeSpec HttpMethod enum not found")
    else:
        methods = tuple(re.findall(r"^\\s*([A-Z]+),?\\s*$", enum.group("body"), re.M))
        if methods != EXPECTED_METHODS:
            errors.append(f"TypeSpec HTTP methods {methods} != {EXPECTED_METHODS}")

    forbidden = {"routeByHeader", "routeByQuery", "dispatchHeaders", "dispatchQuery"}
    if forbidden & set(properties):
        errors.append("request authority exposes forbidden dispatch selectors")
    return errors


def main() -> int:
    errors = audit()
    if errors:
        print("HTTP request-surface authority mismatch:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("HTTP request-surface TypeSpec/JSON Schema peers agree")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
''',
)
write(
    "scripts/test_http_request_surface_authorities.py",
    '''#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "request_surface_authority", ROOT / "scripts/check-http-request-surface-authorities.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RequestSurfaceAuthorityTests(unittest.TestCase):
    def copy(self, root: Path) -> Path:
        shutil.copytree(ROOT / "idl", root / "idl")
        shutil.copytree(ROOT / "json-schema", root / "json-schema")
        return root

    def test_current_peers_agree(self) -> None:
        self.assertEqual([], MODULE.audit(ROOT))

    def test_missing_typespec_headers_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "idl/typespec/http-request-surface.tsp"
            path.write_text(path.read_text().replace("  headers?: Record<unknown>;\\n", ""))
            self.assertTrue(MODULE.audit(root))

    def test_header_dispatch_extension_is_a_veto(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = self.copy(Path(tmp))
            path = root / "json-schema/http-request-surface.schema.json"
            schema = json.loads(path.read_text())
            schema["properties"]["routeByHeader"] = {"type": "string"}
            path.write_text(json.dumps(schema))
            self.assertTrue(MODULE.audit(root))


if __name__ == "__main__":
    unittest.main()
''',
)

example = json.loads(read("examples/demo.route-map.json"))
get_matter = example["map"]["get_matter"]
get_matter["header_params"] = {
    "x-client-version": {
        "type": "String",
        "doc": "Required client contract version. Validation-only; never a routing selector.",
    },
    "if-none-match": {
        "type": "String",
        "required": False,
        "doc": "Optional cache validator forwarded by the HTTP transport.",
    },
}
write("examples/demo.route-map.json", json.dumps(example, indent=2) + "\n")

ridl_cfg = json.loads(read("ridl.json"))
ridl_cfg["languages"] = [
    "dart", "gleam", "go", "json-schema", "kotlin", "python", "rust", "swift", "typescript"
]
write("ridl.json", json.dumps(ridl_cfg, indent=2) + "\n")

replace_once(
    "scripts/test_request_surface_contracts.py",
    '''                "query_schema": {
                    "type": "object",
                    "properties": {
                        "dryRun": {"type": "boolean"},
                        "limit": {"type": "integer"},
                    },
                },
                "request_schema": {
''',
    '''                "query_schema": {
                    "type": "object",
                    "properties": {
                        "dryRun": {"type": "boolean"},
                        "limit": {"type": "integer"},
                    },
                },
                "header_schema": {
                    "type": "object",
                    "additionalProperties": False,
                    "required": ["x-client-version"],
                    "properties": {
                        "x-client-version": {"type": "string"},
                        "if-match": {"type": "string"},
                    },
                },
                "request_schema": {
''',
)
replace_once(
    "scripts/test_request_surface_contracts.py",
    '        self.assertIn(\'"limit"?: number\', generated)\n        self.assertIn(\'"name": string\', generated)\n'
    '        self.assertIn(\'path: RouteTypes[K]["path"]\', generated)\n'
    '        self.assertIn(\'query: RouteTypes[K]["query"]\', generated)\n'
    '        self.assertIn(\'body: RouteTypes[K]["body"]\', generated)',
    '        self.assertIn(\'"limit"?: number\', generated)\n'
    '        self.assertIn(\'"x-client-version": string\', generated)\n'
    '        self.assertIn(\'"if-match"?: string\', generated)\n'
    '        self.assertIn(\'"name": string\', generated)\n'
    '        self.assertIn(\'path: RouteTypes[K]["path"]\', generated)\n'
    '        self.assertIn(\'query: RouteTypes[K]["query"]\', generated)\n'
    '        self.assertIn(\'headers: RouteTypes[K]["headers"]\', generated)\n'
    '        self.assertIn(\'body: RouteTypes[K]["body"]\', generated)',
)
text = read("scripts/test_request_surface_contracts.py")
needle = '''                "query_schema": {
                    "type": "object",
                    "properties": {"limit": {"type": "integer"}},
                },
                "request_schema": {
'''
replacement = '''                "query_schema": {
                    "type": "object",
                    "properties": {"limit": {"type": "integer"}},
                },
                "header_schema": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {"x-client-version": {"type": "string"}},
                },
                "request_schema": {
'''
if text.count(needle) != 1:
    raise SystemExit("Rust v1 request-surface test mapping not found exactly once")
text = text.replace(needle, replacement, 1)
text = text.replace(
    '        self.assertIn("pub struct UpdateItemQuery", generated)\n'
    '        self.assertIn("pub limit: Option<i64>", generated)\n'
    '        self.assertIn("pub struct UpdateItemRequest", generated)',
    '        self.assertIn("pub struct UpdateItemQuery", generated)\n'
    '        self.assertIn("pub limit: Option<i64>", generated)\n'
    '        self.assertIn("pub struct UpdateItemHeaders", generated)\n'
    '        self.assertIn("pub x_client_version: Option<String>", generated)\n'
    '        self.assertIn("pub struct UpdateItemRequest", generated)',
    1,
)
openapi_needle = '''                    "query_schema": {
                        "type": "object",
                        "properties": {"dryRun": {"type": "boolean"}},
                    },
                    "request_schema": {
'''
openapi_repl = '''                    "query_schema": {
                        "type": "object",
                        "properties": {"dryRun": {"type": "boolean"}},
                    },
                    "header_schema": {
                        "type": "object",
                        "additionalProperties": False,
                        "required": ["x-client-version"],
                        "properties": {"x-client-version": {"type": "string"}},
                    },
                    "request_schema": {
'''
if text.count(openapi_needle) != 1:
    raise SystemExit("OpenAPI v1 request-surface test mapping not found")
text = text.replace(openapi_needle, openapi_repl, 1)
text = text.replace(
    '        self.assertFalse(parameters[("dryRun", "query")]["required"])\n',
    '        self.assertFalse(parameters[("dryRun", "query")]["required"])\n'
    '        self.assertTrue(parameters[("x-client-version", "header")]["required"])\n',
    1,
)
write("scripts/test_request_surface_contracts.py", text)

write(
    "scripts/test_typed_request_headers.py",
    '''#!/usr/bin/env python3
from __future__ import annotations

import json
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator

from ridl.emit import dart, gleam, go, json_schema, kotlin, python, rust, swift, typescript
from ridl.model import parse_route_map
from ridl.validate import validate

ROOT = Path(__file__).resolve().parents[1]


class TypedRequestHeaderTests(unittest.TestCase):
    def route_map(self):
        return parse_route_map(json.loads((ROOT / "examples/demo.route-map.json").read_text()))

    def test_header_contract_is_typed_in_every_emitter(self) -> None:
        rmap = self.route_map()
        self.assertEqual([], validate(rmap))
        emitters = (dart, gleam, go, kotlin, python, rust, swift, typescript)
        for emitter in emitters:
            generated = "\\n".join(item.text for item in emitter.emit(rmap))
            self.assertIn("x-client-version", generated, emitter.__name__)
            self.assertIn("headers", generated.lower(), emitter.__name__)

    def test_header_names_and_ownership_are_linted(self) -> None:
        base = json.loads((ROOT / "examples/demo.route-map.json").read_text())
        for name in ("X-Client-Version", "authorization", "content-length"):
            case = json.loads(json.dumps(base))
            case["map"]["get_matter"]["header_params"] = {name: "String"}
            errors = validate(parse_route_map(case))
            self.assertTrue(any("header" in error for error in errors), (name, errors))

    def test_header_contract_is_http_only(self) -> None:
        case = json.loads((ROOT / "examples/demo.route-map.json").read_text())
        case["map"]["get_matter"]["transports"] = ["http", "websocket"]
        errors = validate(parse_route_map(case))
        self.assertTrue(any("headers are HTTP-only" in error for error in errors), errors)

    def test_generated_runtime_schema_checks_every_request_surface(self) -> None:
        rmap = self.route_map()
        generated = {item.path: json.loads(item.text) for item in json_schema.emit(rmap)}
        schema = generated["json-schema/operations/get_matter.request.schema.json"]
        validator = Draft202012Validator(schema)
        valid = {
            "method": "GET",
            "pathTemplate": "/v1/matters/{id}",
            "path": {"id": "4f867eb4-27d4-47b9-83ce-3379c13f24ec"},
            "query": {"include_facts": True},
            "headers": {"x-client-version": "2026.09", "if-none-match": '"etag"'},
        }
        self.assertEqual([], list(validator.iter_errors(valid)))
        for mutation in (
            {**valid, "headers": {}},
            {**valid, "method": "POST"},
            {**valid, "query": {"include_facts": "yes"}},
            {**valid, "headers": {"x-client-version": 3}},
            {**valid, "routeByHeader": "x-client-version"},
        ):
            self.assertTrue(list(validator.iter_errors(mutation)), mutation)
        self.assertEqual(["method", "pathTemplate"], schema["x-ores-routing-identity"])
        self.assertEqual(["path", "query", "headers", "body"], schema["x-ores-validation-only"])


if __name__ == "__main__":
    unittest.main()
''',
)

write(
    "docs/typed-request-surfaces.md",
    '''# Typed HTTP request surfaces

ORES route contracts distinguish **operation identity** from request validation:

- routing identity is exactly HTTP method plus URL path/template;
- path variables, query parameters, request headers, and JSON bodies are typed
  validation inputs and may never select a different operation;
- duplicate method+path slots are a build veto even when their query or header
  schemas differ.

## Compile-time path

RIDL v2 accepts `path_params`, `query_params`, `header_params`, and a typed
`request` record. The eight generated language surfaces carry all four inputs
into the transport request. Header names are retained exactly on the wire while
language identifiers are derived deterministically.

Headers are limited to canonical lower-case HTTP tokens with scalar, enum, or
list-of-scalar values. Authentication, cookies, tracing, proxy forwarding,
content framing, and hop-by-hop headers remain runtime-owned and cannot be
introduced by a business route map.

## Runtime and pre-deploy path

The JSON Schema emitter writes one Draft 2020-12 parsed-request schema per
operation under `json-schema/operations/`. These schemas validate the coerced
logical values after HTTP parsing and before a handler runs. Each schema is
closed and records:

```json
{
  "x-ores-routing-identity": ["method", "pathTemplate"],
  "x-ores-validation-only": ["path", "query", "headers", "body"]
}
```

CI regenerates the artifacts, checks drift, compiles generated targets, and
executes positive and mutation cases. Deploy pipelines should run the same
`ridl check`, `ridl drift`, and Draft 2020-12 validation suite before promotion.

## Peer authorities

`idl/typespec/http-request-surface.tsp` and
`json-schema/http-request-surface.schema.json` are independent, human-authored
peer authorities for the generic parsed envelope. Neither is generated from the
other. `scripts/check-http-request-surface-authorities.py` normalizes their
public field/required/method shapes and fails closed on disagreement.
''',
)

replace_once(
    ".github/workflows/ci.yml",
    '''      - name: Check dual-primary and strict IDL admission
        run: |
          python scripts/test_cross_check_rpc_idl.py -v
          python scripts/cross-check-rpc-idl.py
          python scripts/test_audit_rpc_idl.py -v
          python scripts/audit-rpc-idl.py
''',
    '''      - name: Check dual-primary and strict IDL admission
        run: |
          python scripts/test_cross_check_rpc_idl.py -v
          python scripts/cross-check-rpc-idl.py
          python scripts/test_audit_rpc_idl.py -v
          python scripts/audit-rpc-idl.py
          python scripts/test_http_request_surface_authorities.py -v
          python scripts/check-http-request-surface-authorities.py
''',
)
replace_once(
    ".github/workflows/ci.yml",
    '      - run: python scripts/test_request_surface_contracts.py -v\n'
    '      - run: python scripts/test_e2e_rpc.py -v',
    '      - run: python scripts/test_request_surface_contracts.py -v\n'
    '      - run: python scripts/test_typed_request_headers.py -v\n'
    '      - run: python scripts/test_e2e_rpc.py -v',
)

print("typed request-header implementation applied")
