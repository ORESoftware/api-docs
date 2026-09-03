#!/usr/bin/env python3
"""Fail closed when a RIDL runtime escapes the four-language frame contract."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "runtime" / "conformance.json"
EXPECTED_LANGUAGES = ("dart", "go", "rust", "typescript")
EXPECTED_CONSTANTS = {
    "dart": (
        r"const int frameVersion = 1;",
        r"const int maxFrameBytes = 8 \* 1024 \* 1024;",
    ),
    "go": (
        r"FrameVersion\s+uint8 = 1",
        r"MaxFrameBytes\s+= 8 \* 1024 \* 1024",
    ),
    "rust": (
        r"pub const FRAME_VERSION: u8 = 1;",
        r"pub const MAX_FRAME_BYTES: usize = 8 \* 1024 \* 1024;",
    ),
    "typescript": (
        r"export const FRAME_VERSION = 1 as const;",
        r"export const MAX_FRAME_BYTES = 8 \* 1024 \* 1024;",
    ),
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def read(relative: str) -> str:
    path = ROOT / relative
    require(path.is_file(), f"missing runtime contract file: {relative}")
    return path.read_text(encoding="utf-8")


def main() -> int:
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    require(manifest.get("schemaVersion") == 1, "unsupported runtime manifest")
    require(manifest.get("profile") == "ridl-v2-frame-v1", "unexpected runtime profile")
    require(manifest.get("authority") == "ridl/framing.py", "runtime authority drift")
    require(manifest.get("frameVersion") == 1, "frame version drift")
    require(manifest.get("maxFrameBytes") == 8 * 1024 * 1024, "frame byte limit drift")
    require(manifest.get("tcpLengthPrefixBytes") == 4, "TCP prefix width drift")

    languages = manifest.get("languages")
    require(isinstance(languages, dict), "runtime languages must be an object")
    require(tuple(sorted(languages)) == EXPECTED_LANGUAGES, "runtime language set drift")

    fixture_path = manifest.get("fixture")
    require(isinstance(fixture_path, str), "fixture path missing")
    fixture = json.loads(read(fixture_path))
    require(fixture.get("frame_version") == 1, "fixture frame version drift")
    cases = fixture.get("cases")
    require(isinstance(cases, list) and cases, "frame fixture corpus is empty")

    for language in EXPECTED_LANGUAGES:
        entry = languages[language]
        require(isinstance(entry, dict), f"{language} entry must be an object")
        source_path = entry.get("source")
        test_path = entry.get("test")
        command = entry.get("command")
        require(
            all(isinstance(value, str) and value for value in (source_path, test_path, command)),
            f"{language} manifest entry is incomplete",
        )
        source = read(source_path)
        test = read(test_path)
        require("conformance.json" in test, f"{language} test is not fixture-bound")
        for pattern in EXPECTED_CONSTANTS[language]:
            require(
                re.search(pattern, source) is not None,
                f"{language} wire constant drift: {pattern}",
            )

    typescript = read(languages["typescript"]["source"])
    for forbidden in ("node:", "Buffer", "process.", "require("):
        require(
            forbidden not in typescript,
            f"TypeScript frame runtime is not browser-safe: {forbidden}",
        )

    dart = read(languages["dart"]["source"])
    require(
        "package:" not in dart,
        "Dart frame runtime must remain Flutter-safe and dependency-free",
    )
    require("dart:io" not in dart, "Dart frame runtime must not require dart:io")

    go = read(languages["go"]["source"])
    allowed_go_imports = {
        "bytes",
        "encoding/binary",
        "encoding/json",
        "errors",
        "fmt",
        "sort",
        "strconv",
        "strings",
        "sync/atomic",
        "unicode/utf8",
    }
    imports = set(re.findall(r'^\s*"([^"]+)"$', go, flags=re.MULTILINE))
    require(imports == allowed_go_imports, f"Go runtime import drift: {sorted(imports)}")

    rust_root = read("runtime/rust/src/lib.rs")
    require(
        '#[path = "../frame.rs"]' in rust_root and "pub mod frame;" in rust_root,
        "Rust frame module is not compiled by its crate",
    )

    print("rpc runtime manifest: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
