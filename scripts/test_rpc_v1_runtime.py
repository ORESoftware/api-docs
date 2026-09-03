#!/usr/bin/env python3
"""Fail closed when RPC v1 authorities, fixtures, or four runtimes drift."""

from __future__ import annotations

import json
import re
import struct
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "runtime" / "v1-conformance.json"
EXPECTED_LANGUAGES = ("dart", "go", "rust", "typescript")
EXPECTED_AUTHORITIES = {
    "typespec": "idl/typespec/v1.tsp",
    "jsonSchema": [
        "json-schema/rpc-call.schema.json",
        "json-schema/rpc-receipt.schema.json",
    ],
    "protobuf": "idl/protobuf/ores/rpc/v1/rpc.proto",
}
EXPECTED_SOURCE_SETS = {
    "dart": (
        "clients/dart/lib/rpc_v1.dart",
        "clients/dart/lib/src/rpc_v1.dart",
        "clients/dart/lib/src/rpc_v1_codec.dart",
        "clients/dart/lib/src/rpc_v1_models.dart",
        "clients/dart/lib/src/rpc_v1_support.dart",
    ),
    "go": (
        "clients/go/decode.go",
        "clients/go/encode.go",
        "clients/go/framing.go",
        "clients/go/types.go",
        "clients/go/validate.go",
    ),
    "rust": (
        "rust/src/rpc_v1.rs",
        "rust/src/rpc_v1/decode.rs",
        "rust/src/rpc_v1/helpers.rs",
        "rust/src/rpc_v1/receipt.rs",
        "rust/src/rpc_v1/types.rs",
    ),
    "typescript": (
        "clients/typescript/src/rpc.js",
        "clients/typescript/src/rpc.d.ts",
    ),
}
CALL_ORDER = (
    "v", "op", "id", "key", "transport", "path", "query", "body", "traceId", "spanId"
)
RECEIPT_ORDER = (
    "v", "op", "id", "key", "transport", "ok", "status", "body", "error", "traceId", "spanId"
)
ALLOWED_GO_IMPORTS = {
    "bytes",
    "encoding/binary",
    "encoding/json",
    "errors",
    "fmt",
    "regexp",
    "sort",
    "strconv",
    "strings",
    "sync/atomic",
    "unicode/utf8",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def read(relative: str) -> str:
    path = ROOT / relative
    require(path.is_file(), f"missing RPC v1 contract file: {relative}")
    return path.read_text(encoding="utf-8")


def load_json(relative: str) -> dict[str, Any]:
    value = json.loads(read(relative))
    require(isinstance(value, dict), f"{relative} must contain an object")
    return value


def ordered_subset(keys: list[str], expected: tuple[str, ...], label: str) -> None:
    positions = [expected.index(key) for key in keys]
    require(positions == sorted(positions), f"{label} top-level member order drift")


def audit_fixture(path: str) -> None:
    fixture = load_json(path)
    require(fixture.get("schemaVersion") == 1, "fixture schema version drift")
    require(fixture.get("profile") == "ores-rpc-v1-call-receipt", "fixture profile drift")
    require(fixture.get("maxFrameBytes") == 8 * 1024 * 1024, "fixture byte limit drift")
    require(fixture.get("tcpLengthPrefixBytes") == 4, "fixture TCP prefix width drift")

    valid = fixture.get("valid")
    invalid = fixture.get("invalid")
    require(isinstance(valid, list) and valid, "valid fixture corpus is empty")
    require(isinstance(invalid, list) and invalid, "invalid fixture corpus is empty")
    names: set[str] = set()
    valid_kinds: set[str] = set()
    invalid_kinds: set[str] = set()

    for case in valid:
        require(isinstance(case, dict), "valid fixture entry must be an object")
        name = case.get("name")
        kind = case.get("kind")
        encoded = case.get("encoded")
        prefix = case.get("tcp_prefix_hex")
        require(isinstance(name, str) and name and name not in names, "fixture names must be unique")
        names.add(name)
        require(kind in {"call", "receipt"}, f"{name}: unknown fixture kind")
        valid_kinds.add(kind)
        require(isinstance(encoded, str), f"{name}: encoded frame missing")
        require(len(encoded.encode("utf-8")) <= 8 * 1024 * 1024, f"{name}: frame exceeds limit")
        parsed = json.loads(encoded)
        require(isinstance(parsed, dict), f"{name}: frame must be an object")
        require(parsed.get("v") == 1, f"{name}: wrong frame version")
        require(parsed.get("op") == kind, f"{name}: op/kind mismatch")
        require("t" not in parsed, f"{name}: RIDL v2 discriminator leaked into v1")
        expected_order = CALL_ORDER if kind == "call" else RECEIPT_ORDER
        ordered_subset(list(parsed), expected_order, name)
        canonical = json.dumps(parsed, separators=(",", ":"), ensure_ascii=False)
        require(canonical == encoded, f"{name}: encoded bytes are not canonical")
        expected_prefix = struct.pack(">I", len(encoded.encode("utf-8"))).hex()
        require(prefix == expected_prefix, f"{name}: TCP length witness drift")
        if kind == "receipt":
            ok = parsed.get("ok")
            require(isinstance(ok, bool), f"{name}: receipt ok must be boolean")
            status = parsed.get("status")
            if status is not None:
                require(isinstance(status, int) and not isinstance(status, bool), f"{name}: status type drift")
                require((200 <= status <= 399) if ok else (400 <= status <= 599), f"{name}: status/state mismatch")
            require(("error" not in parsed) if ok else ("error" in parsed), f"{name}: error/state mismatch")
            require(ok or "body" not in parsed, f"{name}: failed receipt carries body")

    for case in invalid:
        require(isinstance(case, dict), "invalid fixture entry must be an object")
        name = case.get("name")
        kind = case.get("kind")
        encoded = case.get("encoded")
        require(isinstance(name, str) and name and name not in names, "fixture names must be unique")
        names.add(name)
        require(kind in {"call", "receipt"}, f"{name}: unknown invalid fixture kind")
        invalid_kinds.add(kind)
        require(isinstance(encoded, str), f"{name}: invalid encoded frame missing")
        json.loads(encoded)

    require(valid_kinds == {"call", "receipt"}, "valid corpus must cover call and receipt")
    require(invalid_kinds == {"call", "receipt"}, "invalid corpus must cover call and receipt")
    require(any("ridl-v2" in name for name in names), "fixture lacks v1/v2 isolation case")


def go_imports(text: str) -> set[str]:
    imports: set[str] = set()
    for block, single in re.findall(
        r'import\s*\((.*?)\)|import\s+"([^"]+)"', text, flags=re.DOTALL
    ):
        if single:
            imports.add(single)
        else:
            imports.update(re.findall(r'^\s*"([^"]+)"', block, flags=re.MULTILINE))
    return imports


def main() -> int:
    manifest = load_json("runtime/v1-conformance.json")
    require(manifest.get("schemaVersion") == 1, "unsupported RPC v1 manifest")
    require(manifest.get("profile") == "ores-rpc-v1-call-receipt", "unexpected RPC v1 profile")
    require(manifest.get("authorities") == EXPECTED_AUTHORITIES, "RPC v1 authority inventory drift")
    require(manifest.get("frameVersion") == 1, "RPC v1 version drift")
    require(manifest.get("maxFrameBytes") == 8 * 1024 * 1024, "RPC v1 byte limit drift")
    require(manifest.get("tcpLengthPrefixBytes") == 4, "RPC v1 TCP prefix drift")
    require(manifest.get("statusPresence") == "optional-compatible", "RPC v1 status compatibility drift")

    for authority in [
        EXPECTED_AUTHORITIES["typespec"],
        *EXPECTED_AUTHORITIES["jsonSchema"],
        EXPECTED_AUTHORITIES["protobuf"],
    ]:
        read(authority)

    typespec = read(EXPECTED_AUTHORITIES["typespec"])
    call_schema = load_json(EXPECTED_AUTHORITIES["jsonSchema"][0])
    receipt_schema = load_json(EXPECTED_AUTHORITIES["jsonSchema"][1])
    protobuf = read(EXPECTED_AUTHORITIES["protobuf"])
    for document, label in ((call_schema, "call"), (receipt_schema, "receipt")):
        require(document.get("$schema") == "https://json-schema.org/draft/2020-12/schema", f"{label} schema draft drift")
        require(document.get("additionalProperties") is False, f"{label} schema must remain closed")
    require("status?: int32;" in typespec, "TypeSpec v1 status-presence contract drift")
    require("status" not in set(receipt_schema.get("required", [])), "JSON Schema v1 status became silently required")
    require("optional uint32 status = 7;" in protobuf, "Protobuf v1 status-presence ledger drift")

    fixture_path = manifest.get("fixture")
    require(isinstance(fixture_path, str), "RPC v1 fixture path missing")
    audit_fixture(fixture_path)

    languages = manifest.get("languages")
    require(isinstance(languages, dict), "RPC v1 languages must be an object")
    require(tuple(sorted(languages)) == EXPECTED_LANGUAGES, "RPC v1 language set drift")

    sources_by_language: dict[str, str] = {}
    for language in EXPECTED_LANGUAGES:
        entry = languages[language]
        require(isinstance(entry, dict), f"{language} entry must be an object")
        sources = entry.get("sources")
        test_path = entry.get("test")
        command = entry.get("command")
        require(tuple(sources or ()) == EXPECTED_SOURCE_SETS[language], f"{language} source set drift")
        require(isinstance(test_path, str) and test_path, f"{language} test path missing")
        require(isinstance(command, str) and command, f"{language} command missing")
        source_text = "\n".join(read(path) for path in sources)
        test_text = read(test_path)
        require("rpc-v1/conformance.json" in test_text, f"{language} test is not fixture-bound")
        require("successful receipt" in source_text and "error receipt" in source_text, f"{language} receipt state machine markers missing")
        sources_by_language[language] = source_text

    require("export const RPC_VERSION = 1;" in sources_by_language["typescript"], "TypeScript version drift")
    require("export const MAX_FRAME_BYTES = 8 * 1024 * 1024;" in sources_by_language["typescript"], "TypeScript byte limit drift")
    for forbidden in ("node:", "Buffer", "process.", "require("):
        require(forbidden not in sources_by_language["typescript"], f"TypeScript RPC subpath is not browser-safe: {forbidden}")

    require("const int rpcV1Version = 1;" in sources_by_language["dart"], "Dart version drift")
    require("const int rpcV1MaxFrameBytes = 8 * 1024 * 1024;" in sources_by_language["dart"], "Dart byte limit drift")
    require("dart:io" not in sources_by_language["dart"], "Dart RPC runtime must not require dart:io")
    require("package:" not in sources_by_language["dart"], "Dart RPC runtime must remain dependency-free")

    require("RPCVersion        uint8 = 1" in sources_by_language["go"], "Go version drift")
    require("MaxFrameBytes           = 8 * 1024 * 1024" in sources_by_language["go"], "Go byte limit drift")
    imports = go_imports(sources_by_language["go"])
    require(imports <= ALLOWED_GO_IMPORTS, f"Go runtime import drift: {sorted(imports - ALLOWED_GO_IMPORTS)}")

    require("pub const RPC_V1_VERSION: u32 = 1;" in sources_by_language["rust"], "Rust version drift")
    require("MAX_FRAME_BYTES" in sources_by_language["rust"], "Rust byte limit is not wired")
    rust_root = read("rust/src/lib.rs")
    require("pub mod rpc_v1;" in rust_root, "Rust RPC v1 module is not compiled")
    require("RpcV1Receipt" in rust_root and "RpcV1Call" in rust_root, "Rust RPC v1 public exports missing")

    typescript_root = read("clients/typescript/src/index.js")
    for legacy in ("compileValidator", "parseRouteMap", "inferMethods", "MAX_FRAME_BYTES"):
        require(f"export function {legacy}" in typescript_root or f"export const {legacy}" in typescript_root, f"legacy TypeScript export lost: {legacy}")
    for strict_export in ("decodeRpcV1Call", "decodeRpcV1Receipt", "RPC_V1_MAX_FRAME_BYTES"):
        require(strict_export in typescript_root, f"strict TypeScript root alias missing: {strict_export}")

    print("RPC v1 four-language conformance: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
