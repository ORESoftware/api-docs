#!/usr/bin/env python3
"""Generate one digest-bound RPC contract bundle for docs and language runtimes.

The v1 route map is parsed once into a normalized contract. OpenAPI, OpenRPC,
Connect, Hyper-Schema, and Rust/TypeScript/Dart/Gleam/Go route surfaces are all
rendered from that same in-memory value. Every artifact carries the same
SHA-256 contract identifier; a consumer can refuse a mismatched docs/runtime
pair before it sends a call.

This command performs no network or repository writes.
"""
from __future__ import annotations

import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from rpc_contract import (  # noqa: E402,F401
    ContractError,
    build_contract,
    canonical_bytes,
    default_maps,
    gen_go,
    generate_one,
    project_connect,
    project_hyper_schema,
    project_openapi,
    project_openrpc,
    run,
    sha256_hex,
    verify_bundle,
    verify_ridl_emitters,
)

if __name__ == "__main__":
    raise SystemExit(run())
