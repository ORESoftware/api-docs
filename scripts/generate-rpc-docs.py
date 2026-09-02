#!/usr/bin/env python3
"""Generate and verify RPC documentation from RIDL plus language runtime surfaces.

This gate deliberately joins three things that used to drift independently:

* the RIDL operation contract (path, methods, payloads, transports, streaming);
* the generated RPC seam in every supported language; and
* the machine-readable documentation index served to humans and tooling.

It is dependency-free and fails closed on missing languages, source paths,
transport/stream witnesses, symbols, or generated documentation drift.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any

CONFIG_VERSION = "1.0.0"
MANIFEST_VERSION = "1.0.0"
DOC_VERSION = "1.0.0"
SUPPORTED_LANGUAGES = (
    "rust",
    "typescript",
    "dart",
    "gleam",
    "go",
    "python",
    "kotlin",
    "swift",
)
SUPPORTED_ROLES = {"client", "server", "both"}
LANGUAGE_MECHANISMS = {
    "rust": {"trait"},
    "typescript": {"interface"},
    "dart": {"abstract_interface_class"},
    "gleam": {"function_type"},
    "go": {"interface"},
    "python": {"protocol"},
    "kotlin": {"interface"},
    "swift": {"protocol"},
}
IDENTIFIER = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
SERVICE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")


class CouplingError(ValueError):
    pass


def _load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise CouplingError(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise CouplingError(f"invalid JSON in {path}: {exc}") from exc


def _closed_object(
    value: Any,
    where: str,
    allowed: set[str],
    required: set[str],
) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise CouplingError(f"{where} must be an object")
    unknown = sorted(set(value) - allowed)
    if unknown:
        raise CouplingError(f"{where} has unknown fields: {unknown}")
    missing = sorted(required - set(value))
    if missing:
        raise CouplingError(f"{where} is missing required fields: {missing}")
    return value


def _repo_path(root: Path, raw: Any, where: str, *, must_exist: bool) -> Path:
    if not isinstance(raw, str) or not raw or "\\" in raw or "\x00" in raw:
        raise CouplingError(f"{where} must be a non-empty POSIX repository path")
    pure = PurePosixPath(raw)
    if pure.is_absolute() or ".." in pure.parts or "." in pure.parts:
        raise CouplingError(f"{where} must stay inside the repository")
    path = (root / pure).resolve()
    try:
        path.relative_to(root)
    except ValueError as exc:
        raise CouplingError(f"{where} escapes the repository") from exc
    if must_exist and not path.is_file():
        raise CouplingError(f"{where} does not exist: {raw}")
    return path


def _canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def _sha256(value: Any) -> str:
    return hashlib.sha256(_canonical_bytes(value)).hexdigest()


def _type_value(route: dict[str, Any], name: str) -> Any:
    value = route.get(name)
    if isinstance(value, dict) and set(value) <= {
        "type",
        "required",
        "default",
        "doc",
    }:
        return value.get("type")
    return value


def _route_descriptor(service: str, key: str, raw: Any) -> dict[str, Any]:
    if not isinstance(raw, dict):
        raise CouplingError(f"map.{key} must be an object in RIDL v2")
    allowed = {
        "path",
        "methods",
        "summary",
        "doc",
        "path_params",
        "query_params",
        "request",
        "response",
        "errors",
        "delivery",
        "opto_sync",
        "transports",
        "stream",
        "binding",
        "deprecated",
        "client",
        "content_type",
        "request_schema",
        "response_schema",
    }
    unknown = sorted(set(raw) - allowed)
    if unknown:
        raise CouplingError(f"map.{key} has unknown RIDL fields: {unknown}")
    path = raw.get("path")
    methods = raw.get("methods")
    if (
        not isinstance(path, str)
        or not path.startswith("/")
        or any(character.isspace() for character in path)
    ):
        raise CouplingError(f"map.{key}.path must be an absolute HTTP-style path")
    if (
        not isinstance(methods, list)
        or not methods
        or any(not isinstance(method, str) for method in methods)
    ):
        raise CouplingError(f"map.{key}.methods must be a non-empty string array")
    transports = raw.get("transports", ["http"])
    if (
        not isinstance(transports, list)
        or not transports
        or any(not isinstance(transport, str) for transport in transports)
    ):
        raise CouplingError(f"map.{key}.transports must be a non-empty string array")
    stream = raw.get("stream", "unary")
    if not isinstance(stream, str):
        raise CouplingError(f"map.{key}.stream must be a string")
    descriptor = {
        "service": service,
        "operation": key,
        "rpc_method_id": f"{service}.{key}",
        "path": path,
        "methods": methods,
        "transports": transports,
        "stream": stream,
        "delivery": raw.get("delivery", "direct"),
        "content_type": raw.get("content_type", "json"),
        "request": _type_value(raw, "request"),
        "response": _type_value(raw, "response"),
        "errors": raw.get("errors", {}),
    }
    descriptor["contract_sha256"] = _sha256(descriptor)
    return descriptor


def _quoted(value: str) -> tuple[str, str]:
    return (json.dumps(value), repr(value))


def _surface_has_operation(text: str, descriptor: dict[str, Any]) -> bool:
    positions: list[int] = []
    for token in _quoted(descriptor["operation"]):
        start = 0
        while True:
            index = text.find(token, start)
            if index < 0:
                break
            positions.append(index)
            start = index + len(token)
    expected: list[str] = [descriptor["path"], *descriptor["methods"]]
    expected.extend(descriptor["transports"])
    expected.append(descriptor["stream"])
    for position in positions:
        window = text[max(0, position - 256) : position + 2048]
        if all(any(token in window for token in _quoted(item)) for item in expected):
            return True
    return False


def _validate_surface(root: Path, raw: Any, index: int) -> dict[str, Any]:
    where = f"surfaces[{index}]"
    surface = _closed_object(
        raw,
        where,
        {"language", "role", "mechanism", "file", "symbol", "source_sha256"},
        {"language", "role", "mechanism", "file", "symbol"},
    )
    language = surface["language"]
    if language not in SUPPORTED_LANGUAGES:
        raise CouplingError(f"{where}.language is unsupported: {language!r}")
    role = surface["role"]
    if role not in SUPPORTED_ROLES:
        raise CouplingError(f"{where}.role must be one of {sorted(SUPPORTED_ROLES)}")
    mechanism = surface["mechanism"]
    if mechanism not in LANGUAGE_MECHANISMS[language]:
        raise CouplingError(
            f"{where}.mechanism {mechanism!r} is not valid for {language}; "
            f"expected {sorted(LANGUAGE_MECHANISMS[language])}"
        )
    symbol = surface["symbol"]
    if not isinstance(symbol, str) or not IDENTIFIER.fullmatch(symbol):
        raise CouplingError(f"{where}.symbol must be a portable identifier")
    path = _repo_path(root, surface["file"], f"{where}.file", must_exist=True)
    source = path.read_text(encoding="utf-8")
    if "generated by ridl" not in source[:4096].lower():
        raise CouplingError(f"{where}.file is not visibly generated by RIDL")
    if symbol not in source:
        raise CouplingError(
            f"{where}.symbol {symbol!r} is absent from {surface['file']}"
        )
    actual_sha = hashlib.sha256(path.read_bytes()).hexdigest()
    claimed = surface.get("source_sha256")
    if claimed is not None:
        if not isinstance(claimed, str) or not SHA256.fullmatch(claimed):
            raise CouplingError(f"{where}.source_sha256 must be lowercase SHA-256")
        if claimed != actual_sha:
            raise CouplingError(
                f"{where}.source_sha256 does not match {surface['file']}"
            )
    return {
        "language": language,
        "role": role,
        "mechanism": mechanism,
        "file": surface["file"],
        "symbol": symbol,
        "source_sha256": actual_sha,
        "_source": source,
    }


def _generate_one(root: Path, manifest_path: Path) -> tuple[Path, dict[str, Any]]:
    raw = _closed_object(
        _load_json(manifest_path),
        str(manifest_path.relative_to(root)),
        {
            "$schema",
            "schema_version",
            "service",
            "route_map",
            "docs_output",
            "required_languages",
            "surfaces",
        },
        {
            "schema_version",
            "service",
            "route_map",
            "docs_output",
            "required_languages",
            "surfaces",
        },
    )
    if raw["schema_version"] != MANIFEST_VERSION:
        raise CouplingError(
            f"{manifest_path}: schema_version must be {MANIFEST_VERSION}"
        )
    service = raw["service"]
    if not isinstance(service, str) or not SERVICE.fullmatch(service):
        raise CouplingError(f"{manifest_path}: service is invalid")
    route_map_path = _repo_path(
        root, raw["route_map"], "route_map", must_exist=True
    )
    output_path = _repo_path(
        root, raw["docs_output"], "docs_output", must_exist=False
    )
    required = raw["required_languages"]
    if (
        not isinstance(required, list)
        or not required
        or len(required) != len(set(required))
    ):
        raise CouplingError(
            f"{manifest_path}: required_languages must be a unique non-empty array"
        )
    if any(language not in SUPPORTED_LANGUAGES for language in required):
        raise CouplingError(
            f"{manifest_path}: required_languages contains an unsupported language"
        )
    surfaces_raw = raw["surfaces"]
    if not isinstance(surfaces_raw, list) or not surfaces_raw:
        raise CouplingError(f"{manifest_path}: surfaces must be a non-empty array")
    surfaces = [
        _validate_surface(root, surface, index)
        for index, surface in enumerate(surfaces_raw)
    ]
    languages = [surface["language"] for surface in surfaces]
    if len(languages) != len(set(languages)):
        raise CouplingError(
            f"{manifest_path}: each language may have only one authoritative surface"
        )
    if set(languages) != set(required):
        raise CouplingError(
            f"{manifest_path}: language surfaces {sorted(languages)} do not match "
            f"required {sorted(required)}"
        )

    route_map = _closed_object(
        _load_json(route_map_path),
        raw["route_map"],
        {
            "schema_version",
            "service",
            "title",
            "version",
            "description",
            "types",
            "map",
            "files",
        },
        {"schema_version", "service", "map"},
    )
    if route_map["schema_version"] != "2.0.0":
        raise CouplingError(
            f"{raw['route_map']}: only RIDL 2.0.0 is supported"
        )
    if route_map["service"] != service:
        raise CouplingError(
            f"{manifest_path}: service {service!r} disagrees with route map "
            f"{route_map['service']!r}"
        )
    routes = route_map["map"]
    if not isinstance(routes, dict) or not routes:
        raise CouplingError(f"{raw['route_map']}: map must be a non-empty object")
    operations = [
        _route_descriptor(service, key, value) for key, value in routes.items()
    ]

    for surface in surfaces:
        for descriptor in operations:
            if not _surface_has_operation(surface["_source"], descriptor):
                raise CouplingError(
                    f"{surface['file']}: no complete operation witness for "
                    f"{descriptor['rpc_method_id']} "
                    "(path/method/transports/stream must stay together)"
                )

    public_surfaces = [
        {key: value for key, value in surface.items() if key != "_source"}
        for surface in sorted(surfaces, key=lambda item: item["language"])
    ]
    document: dict[str, Any] = {
        "schema_version": DOC_VERSION,
        "service": service,
        "service_version": route_map.get("version"),
        "title": route_map.get("title"),
        "description": route_map.get("description"),
        "generated_from": raw["route_map"],
        "surface_manifest": str(manifest_path.relative_to(root).as_posix()),
        "route_map_sha256": hashlib.sha256(route_map_path.read_bytes()).hexdigest(),
        "language_surfaces": public_surfaces,
        "operations": operations,
    }
    document["document_sha256"] = _sha256(document)
    return output_path, document


def generate(root: Path, config_path: Path) -> list[tuple[Path, dict[str, Any]]]:
    config = _closed_object(
        _load_json(config_path),
        str(config_path.relative_to(root)),
        {"schema_version", "manifests"},
        {"schema_version", "manifests"},
    )
    if config["schema_version"] != CONFIG_VERSION:
        raise CouplingError(
            f"{config_path}: schema_version must be {CONFIG_VERSION}"
        )
    manifests = config["manifests"]
    if (
        not isinstance(manifests, list)
        or not manifests
        or len(manifests) != len(set(manifests))
    ):
        raise CouplingError(
            f"{config_path}: manifests must be a unique non-empty array"
        )
    outputs: list[tuple[Path, dict[str, Any]]] = []
    seen_outputs: set[Path] = set()
    for index, raw_path in enumerate(manifests):
        path = _repo_path(
            root, raw_path, f"manifests[{index}]", must_exist=True
        )
        output, document = _generate_one(root, path)
        if output in seen_outputs:
            raise CouplingError(
                f"duplicate docs output: {output.relative_to(root)}"
            )
        seen_outputs.add(output)
        outputs.append((output, document))
    return outputs


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--config", default="rpc-coupling.json")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    config = _repo_path(root, args.config, "config", must_exist=True)
    try:
        outputs = generate(root, config)
    except CouplingError as exc:
        print(f"rpc-doc coupling failed: {exc}", file=sys.stderr)
        return 1

    drift: list[str] = []
    for output, document in outputs:
        rendered = (
            json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
        )
        if args.write:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(rendered, encoding="utf-8")
        else:
            try:
                current = output.read_text(encoding="utf-8")
            except FileNotFoundError:
                drift.append(
                    f"missing generated RPC docs: {output.relative_to(root)}"
                )
                continue
            if current != rendered:
                drift.append(
                    f"{output.relative_to(root)} differs from its RIDL/language "
                    "surfaces; run scripts/generate-rpc-docs.py --write"
                )
    if drift:
        for item in drift:
            print(f"rpc-doc coupling failed: {item}", file=sys.stderr)
        return 1
    print(f"rpc-doc coupling ok ({len(outputs)} document(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
