from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).parent

RPC_SPEC = importlib.util.spec_from_file_location(
    "ores_rpc_config_salvage_runtime",
    HERE / "ores_rpc_config.py",
)
assert RPC_SPEC and RPC_SPEC.loader
rpc = importlib.util.module_from_spec(RPC_SPEC)
RPC_SPEC.loader.exec_module(rpc)

BASE_SPEC = importlib.util.spec_from_file_location(
    "ores_rpc_config_existing_tests",
    HERE / "test_ores_rpc_config.py",
)
assert BASE_SPEC and BASE_SPEC.loader
existing = importlib.util.module_from_spec(BASE_SPEC)
BASE_SPEC.loader.exec_module(existing)
BASE = existing.BASE


class SalvagedOresRpcConfigBoundaryTests(unittest.TestCase):
    def repo(self) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        tmp = tempfile.TemporaryDirectory()
        root = Path(tmp.name)
        (root / "server").mkdir()
        (root / "web").mkdir()
        (root / "contracts").mkdir()
        (root / "contracts/routes.json").write_text("{}\n", encoding="utf-8")
        (root / ".cli-flags.toml").write_text("[flags]\n", encoding="utf-8")
        return tmp, root

    def load(self, text: str = BASE):
        tmp, root = self.repo()
        self.addCleanup(tmp.cleanup)
        path = root / ".ores-rpc.toml"
        path.write_text(text, encoding="utf-8")
        return rpc.load_config(path, root), root

    def test_declared_flags_contract_file_must_exist(self):
        tmp, root = self.repo()
        self.addCleanup(tmp.cleanup)
        (root / ".cli-flags.toml").unlink()
        path = root / ".ores-rpc.toml"
        path.write_text(BASE, encoding="utf-8")

        with self.assertRaises(rpc.ConfigError) as ctx:
            rpc.load_config(path, root)

        self.assertEqual(ctx.exception.code, "path-missing")

    def test_endpoint_env_cannot_reference_secret(self):
        text = BASE.replace(
            'endpointEnv = "API_BASE_URL"',
            'endpointEnv = "SERVICE_TOKEN"',
        )

        with self.assertRaises(rpc.ConfigError) as ctx:
            self.load(text)

        self.assertEqual(ctx.exception.code, "endpoint-env-secret")


if __name__ == "__main__":
    unittest.main()
