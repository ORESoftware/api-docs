"""Public API for digest-bound RPC contract generation."""
from .bundle import default_maps, generate_one, run, verify_bundle, verify_ridl_emitters
from .languages import gen_go
from .model import ContractError, build_contract, canonical_bytes, sha256_hex
from .projections import project_connect, project_hyper_schema, project_openapi, project_openrpc

__all__ = [
    "ContractError",
    "build_contract",
    "canonical_bytes",
    "default_maps",
    "gen_go",
    "generate_one",
    "project_connect",
    "project_hyper_schema",
    "project_openapi",
    "project_openrpc",
    "run",
    "sha256_hex",
    "verify_bundle",
    "verify_ridl_emitters",
]
