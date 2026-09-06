"""Generation, verification, and CLI orchestration for RPC bundles."""
from __future__ import annotations

import argparse
import json
import re
import tempfile
from pathlib import Path
from typing import Any

from .languages import (
    _mechanism_manifest,
    _write,
    gen_dart_bound,
    gen_gleam_bound,
    gen_go,
    gen_rust_bound,
    gen_typescript_bound,
)
from .model import (
    EXPECTED_RIDL_EMITTERS,
    ROOT,
    ContractError,
    build_contract,
    sha256_hex,
)
from .projections import (
    project_connect,
    project_hyper_schema,
    project_openapi,
    project_openrpc,
)


def generate_one(map_path: Path, out_root: Path) -> Path:
    contract = build_contract(map_path)
    raw = json.loads(map_path.read_text(encoding="utf-8"))
    mapping = raw["map"]
    stem = map_path.name.removesuffix(".route-map.json").replace("-", "_")
    target = out_root / stem

    _write(target / "contract.json", contract)
    _write(target / "docs" / "openapi.json", project_openapi(contract))
    _write(target / "docs" / "openrpc.json", project_openrpc(contract))
    _write(target / "docs" / "connect.json", project_connect(contract))
    _write(target / "docs" / "hyper-schema.json", project_hyper_schema(contract))
    _write(
        target / "typescript" / "routes.ts",
        gen_typescript_bound(contract, mapping),
    )
    _write(
        target / "rust" / "routes.rs",
        gen_rust_bound(contract, mapping),
    )
    _write(
        target / "dart" / "routes.dart",
        gen_dart_bound(contract, mapping),
    )
    _write(
        target / "gleam" / "routes.gleam",
        gen_gleam_bound(contract, mapping),
    )
    _write(target / "go" / "routes.go", gen_go(contract))
    _write(target / "go" / "go.mod", "module example.invalid/ores/rpccontract\n\ngo 1.23\n")
    return target


def _expected_rpc_extension(operation: dict[str, Any], digest: str) -> dict[str, Any]:
    extension: dict[str, Any] = {
        "contractSha256": digest,
        "key": operation["key"],
        "transports": operation["transports"],
        "delivery": operation["delivery"],
    }
    for field in ("tcpFraming", "aliasOf", "optoSync"):
        if operation.get(field) is not None:
            extension[field] = operation[field]
    return extension


def _expected_document_bindings(
    name: str, contract: dict[str, Any], digest: str
) -> list[dict[str, Any]]:
    bindings: list[dict[str, Any]] = []
    for operation in contract["operations"]:
        extension = _expected_rpc_extension(operation, digest)
        if name in {"openapi", "hyper-schema"}:
            for method in operation["methods"]:
                bindings.append(
                    {
                        "key": operation["key"],
                        "path": operation["path"],
                        "methods": [method],
                        "extension": extension,
                    }
                )
        elif name == "openrpc":
            bindings.append(
                {
                    "key": operation["key"],
                    "path": operation["path"],
                    "methods": operation["methods"],
                    "extension": extension,
                }
            )
        elif name == "connect" and re.fullmatch(
            r"[A-Z][A-Za-z0-9]*", operation["key"]
        ):
            bindings.append(
                {
                    "key": operation["key"],
                    "path": operation["path"],
                    "methods": ["POST"],
                    "extension": extension,
                }
            )
    return bindings


def _document_bindings(name: str, document: dict[str, Any]) -> list[dict[str, Any]]:
    bindings: list[dict[str, Any]] = []
    if name == "openapi":
        for path, path_item in document.get("paths", {}).items():
            if not isinstance(path_item, dict):
                continue
            for method, operation in path_item.items():
                if not isinstance(operation, dict) or "x-ores-rpc" not in operation:
                    continue
                bindings.append(
                    {
                        "key": operation.get("operationId"),
                        "path": path,
                        "methods": [method.upper()],
                        "extension": operation.get("x-ores-rpc"),
                    }
                )
    elif name == "openrpc":
        for method in document.get("methods", []):
            if not isinstance(method, dict) or "x-ores-rpc" not in method:
                continue
            bindings.append(
                {
                    "key": method.get("name"),
                    "path": method.get("x-http-path"),
                    "methods": method.get("x-http-methods"),
                    "extension": method.get("x-ores-rpc"),
                }
            )
    elif name == "connect":
        for service in document.get("services", {}).values():
            if not isinstance(service, dict):
                continue
            for method_name, method in service.get("methods", {}).items():
                if not isinstance(method, dict) or "x-ores-rpc" not in method:
                    continue
                bindings.append(
                    {
                        "key": method_name,
                        "path": method.get("path"),
                        "methods": [method.get("httpMethod")],
                        "extension": method.get("x-ores-rpc"),
                    }
                )
    elif name == "hyper-schema":
        for link in document.get("links", []):
            if not isinstance(link, dict) or "x-ores-rpc" not in link:
                continue
            bindings.append(
                {
                    "key": link.get("rel"),
                    "path": link.get("href"),
                    "methods": [link.get("method")],
                    "extension": link.get("x-ores-rpc"),
                }
            )
    else:
        raise ContractError(f"unknown RPC documentation projection {name!r}")
    return bindings


def _sorted_bindings(bindings: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        bindings,
        key=lambda binding: json.dumps(
            binding, ensure_ascii=False, separators=(",", ":"), sort_keys=True
        ),
    )


def _capture(text: str, pattern: str, language: str) -> str:
    match = re.search(pattern, text, flags=re.MULTILINE | re.DOTALL)
    if match is None:
        raise ContractError(f"{language} missing machine-readable RPC mechanism manifest")
    return match.group(1)


def parse_language_mechanism_manifest(
    language: str, text: str
) -> dict[str, Any]:
    """Parse an emitted language surface back into the canonical mechanism map.

    Verification must compare the represented object, not merely search for
    strings: otherwise stale or inert constants can pass while the executable
    route metadata disagrees with the served API documentation.
    """

    if language == "typescript":
        payload = _capture(
            text,
            r"export const RPC_MECHANISMS = (\{.*?\}) as const;",
            language,
        )
    elif language == "rust":
        payload = _capture(
            text,
            r'pub const RPC_MECHANISMS_JSON: &str = r###"(.*?)"###;',
            language,
        )
    elif language == "dart":
        payload = _capture(
            text,
            r"const String rpcMechanismsJson = r'''(.*?)''';",
            language,
        )
    elif language == "gleam":
        literal = _capture(
            text,
            r"^pub const rpc_mechanisms_json: String = ([^\n]+)$",
            language,
        )
        try:
            payload = json.loads(literal)
        except json.JSONDecodeError as exc:
            raise ContractError(f"{language} RPC mechanism string is invalid") from exc
    elif language == "go":
        literal = _capture(
            text,
            r"^const RPCMechanismsJSON = ([^\n]+)$",
            language,
        )
        try:
            payload = json.loads(literal)
        except json.JSONDecodeError as exc:
            raise ContractError(f"{language} RPC mechanism string is invalid") from exc
    else:
        raise ContractError(f"unknown RPC language surface {language!r}")

    try:
        parsed = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise ContractError(f"{language} RPC mechanism manifest is invalid JSON") from exc
    if not isinstance(parsed, dict):
        raise ContractError(f"{language} RPC mechanism manifest must be an object")
    return parsed


def verify_bundle(target: Path) -> None:
    contract = json.loads((target / "contract.json").read_text(encoding="utf-8"))
    semantic = {
        key: value
        for key, value in contract.items()
        if key not in {"contractSha256", "source"}
    }
    digest = sha256_hex(semantic)
    if digest != contract.get("contractSha256"):
        raise ContractError(f"{target}: contract digest mismatch")

    schema_names = {
        "openapi": "openapi-3.1-subset.schema.json",
        "openrpc": "openrpc-1.3-subset.schema.json",
        "connect": "connect-json-unary.schema.json",
        "hyper-schema": "json-hyper-schema-links.schema.json",
    }
    try:
        from jsonschema import Draft202012Validator
    except ImportError as exc:
        raise ContractError("jsonschema is required for bundle verification") from exc
    for name, schema_name in schema_names.items():
        doc = json.loads((target / "docs" / f"{name}.json").read_text(encoding="utf-8"))
        if doc.get("x-ores-rpc-contract-sha256") != digest:
            raise ContractError(f"{target}: {name} digest mismatch")
        schema = json.loads((ROOT / "json-schema" / schema_name).read_text(encoding="utf-8"))
        errors = sorted(
            Draft202012Validator(schema).iter_errors(doc),
            key=lambda error: list(error.absolute_path),
        )
        if errors:
            raise ContractError(f"{target}: {name} schema error: {errors[0]}")
        expected = _sorted_bindings(_expected_document_bindings(name, contract, digest))
        actual = _sorted_bindings(_document_bindings(name, doc))
        if actual != expected:
            raise ContractError(
                f"{target}: {name} RPC mechanisms differ from the normalized contract"
            )

    language_files = {
        "typescript": target / "typescript" / "routes.ts",
        "rust": target / "rust" / "routes.rs",
        "dart": target / "dart" / "routes.dart",
        "gleam": target / "gleam" / "routes.gleam",
        "go": target / "go" / "routes.go",
    }
    expected_manifest = _mechanism_manifest(contract)
    for language, path in language_files.items():
        text = path.read_text(encoding="utf-8")
        if digest not in text:
            raise ContractError(f"{target}: {language} missing contract digest")
        actual_manifest = parse_language_mechanism_manifest(language, text)
        if actual_manifest != expected_manifest:
            raise ContractError(
                f"{target}: {language} RPC mechanisms differ from the normalized contract"
            )


def default_maps() -> list[Path]:
    return [
        path
        for path in sorted((ROOT / "examples").glob("*.route-map.json"))
        if str(json.loads(path.read_text(encoding="utf-8")).get("schema_version", ""))
        == "1.0.0"
    ]


def verify_ridl_emitters() -> None:
    actual = tuple(
        sorted(
            path.stem
            for path in (ROOT / "ridl" / "emit").glob("*.py")
            if path.name not in {"__init__.py", "base.py", "json_schema.py"}
        )
    )
    if actual != EXPECTED_RIDL_EMITTERS:
        raise ContractError(
            f"RIDL emitter set {actual!r} != {EXPECTED_RIDL_EMITTERS!r}"
        )


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--map", action="append", dest="maps", default=[])
    parser.add_argument("--out", type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify every emitted docs and language surface after generation",
    )
    args = parser.parse_args(argv)

    maps = [Path(item) for item in args.maps] if args.maps else default_maps()
    maps = [path if path.is_absolute() else ROOT / path for path in maps]
    if not maps:
        parser.error("no v1 route maps")

    verify_ridl_emitters()

    owns_tmp = args.out is None
    temp: tempfile.TemporaryDirectory[str] | None = None
    if owns_tmp:
        temp = tempfile.TemporaryDirectory()
        out = Path(temp.name)
    else:
        out = args.out
        out.mkdir(parents=True, exist_ok=True)

    index: dict[str, Any] = {
        "formatVersion": 1,
        "contracts": [],
    }
    try:
        for map_path in maps:
            target = generate_one(map_path, out)
            if args.check:
                verify_bundle(target)
            contract = json.loads((target / "contract.json").read_text(encoding="utf-8"))
            index["contracts"].append(
                {
                    "source": contract["source"],
                    "service": contract["service"],
                    "contractSha256": contract["contractSha256"],
                    "operationCount": len(contract["operations"]),
                }
            )
        index["contracts"].sort(key=lambda item: item["source"])
        index["catalogSha256"] = sha256_hex(index["contracts"])
        _write(out / "index.json", index)
        if args.check:
            reread = json.loads((out / "index.json").read_text(encoding="utf-8"))
            if reread["catalogSha256"] != sha256_hex(reread["contracts"]):
                raise ContractError("catalog digest mismatch")
    finally:
        if temp is not None:
            temp.cleanup()
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
