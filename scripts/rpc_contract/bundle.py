"""Generation, verification, and CLI orchestration for RPC bundles."""
from __future__ import annotations

import argparse
import json
import re
import tempfile
from pathlib import Path
from typing import Any

from .languages import (
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
        extensions: list[dict[str, Any]] = []
        pending: list[Any] = [doc]
        while pending:
            value = pending.pop()
            if isinstance(value, dict):
                extension = value.get("x-ores-rpc")
                if isinstance(extension, dict):
                    extensions.append(extension)
                pending.extend(value.values())
            elif isinstance(value, list):
                pending.extend(value)
        has_connect_methods = any(
            re.fullmatch(r"[A-Z][A-Za-z0-9]*", operation["key"])
            for operation in contract["operations"]
        )
        if not extensions and contract["operations"] and (name != "connect" or has_connect_methods):
            raise ContractError(f"{target}: {name} has no RPC operation extensions")
        if any(extension.get("contractSha256") != digest for extension in extensions):
            raise ContractError(f"{target}: {name} contains an unbound RPC operation")

    language_files = {
        "typescript": target / "typescript" / "routes.ts",
        "rust": target / "rust" / "routes.rs",
        "dart": target / "dart" / "routes.dart",
        "gleam": target / "gleam" / "routes.gleam",
        "go": target / "go" / "routes.go",
    }
    for language, path in language_files.items():
        text = path.read_text(encoding="utf-8")
        if digest not in text:
            raise ContractError(f"{target}: {language} missing contract digest")
        for operation in contract["operations"]:
            for required in (
                operation["key"],
                operation["path"],
                *operation["methods"],
                *operation["transports"],
                operation["delivery"],
                *([operation["tcpFraming"]] if operation.get("tcpFraming") else []),
                *([operation["aliasOf"]] if operation.get("aliasOf") else []),
                *list((operation.get("optoSync") or {}).values()),
            ):
                if required not in text:
                    raise ContractError(
                        f"{target}: {language} missing {required!r} "
                        f"for {operation['key']}"
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
