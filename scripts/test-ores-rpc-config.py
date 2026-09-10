from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import textwrap
import unittest

MODULE_PATH = Path(__file__).with_name("check-ores-rpc-config.py")
spec = importlib.util.spec_from_file_location("ores_rpc_config", MODULE_PATH)
assert spec and spec.loader
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

BASE = textwrap.dedent('''\
schemaVersion = "ores.rpc.config/v1"
repositoryMode = "combined"
strict = true

[tjsv]
repository = "ORESoftware/typespec-json-schema-validator"
consumerLock = "contracts/tjsv-consumer.lock.json"
failClosed = true

[flags2env]
provider = "flags-2-env"
contract = ".cli-flags.toml"
contractPresent = false
requiredWhenExecutable = true
rejectUnknownFlags = true
secretsFromEnvironmentOnly = true
precedence = "flags>env>toml"

[rpc]
routeMapGlob = "examples/*.route-map.json"
contractBundle = "scripts/rpc-contract-bundle.py"
transports = ["http", "tcp", "websocket", "nats"]
framings = ["json", "ndjson", "length-prefixed"]
maxFrameBytes = 8388608

[client]
enabled = true
root = "clients"

[server]
enabled = true
root = "rust"

[[environment]]
name = "ORES_RPC_MAX_FRAME_BYTES"
target = "rpc.maxFrameBytes"
valueType = "uint32"
source = "flags-2-env"
secret = false
''')


class OresRpcConfigTests(unittest.TestCase):
    def make_repo(self, config: str = BASE):
        tmp = tempfile.TemporaryDirectory()
        root = Path(tmp.name)
        for path in ["contracts", "examples", "scripts", "clients", "rust"]:
            (root / path).mkdir(parents=True, exist_ok=True)
        (root / ".ores-rpc.toml").write_text(config, encoding="utf-8")
        (root / "contracts/tjsv-consumer.lock.json").write_text(
            json.dumps({"repository": "ORESoftware/typespec-json-schema-validator"}),
            encoding="utf-8",
        )
        (root / "examples/test.route-map.json").write_text("{}\n", encoding="utf-8")
        (root / "scripts/rpc-contract-bundle.py").write_text("# fixture\n", encoding="utf-8")
        return tmp, root

    def test_combined_config_passes(self):
        tmp, root = self.make_repo()
        self.addCleanup(tmp.cleanup)
        loaded = mod.load_config(root / ".ores-rpc.toml", {})
        self.assertEqual(loaded["repositoryMode"], "combined")

    def test_environment_override_is_typed_and_applied(self):
        tmp, root = self.make_repo()
        self.addCleanup(tmp.cleanup)
        loaded = mod.load_config(
            root / ".ores-rpc.toml",
            {"ORES_RPC_MAX_FRAME_BYTES": "4096"},
        )
        self.assertEqual(loaded["rpc"]["maxFrameBytes"], 4096)

    def test_secret_binding_is_rejected(self):
        tmp, root = self.make_repo(BASE.replace("secret = false", "secret = true"))
        self.addCleanup(tmp.cleanup)
        with self.assertRaisesRegex(mod.ConfigError, "secret values"):
            mod.load_config(root / ".ores-rpc.toml", {})

    def test_unknown_root_key_is_rejected(self):
        tmp, root = self.make_repo(
            BASE.replace("strict = true", "strict = true\nshadow = true")
        )
        self.addCleanup(tmp.cleanup)
        with self.assertRaisesRegex(mod.ConfigError, "root keys differ"):
            mod.load_config(root / ".ores-rpc.toml", {})

    def test_mode_role_mismatch_is_rejected(self):
        tmp, root = self.make_repo(
            BASE.replace('repositoryMode = "combined"', 'repositoryMode = "client-only"')
        )
        self.addCleanup(tmp.cleanup)
        with self.assertRaisesRegex(mod.ConfigError, "repositoryMode contradicts"):
            mod.load_config(root / ".ores-rpc.toml", {})

    def test_parent_traversal_is_rejected(self):
        tmp, root = self.make_repo(BASE.replace('root = "clients"', 'root = "../clients"', 1))
        self.addCleanup(tmp.cleanup)
        with self.assertRaisesRegex(mod.ConfigError, "must not traverse"):
            mod.load_config(root / ".ores-rpc.toml", {})

    def test_declared_flags_contract_must_exist(self):
        tmp, root = self.make_repo(
            BASE.replace("contractPresent = false", "contractPresent = true")
        )
        self.addCleanup(tmp.cleanup)
        with self.assertRaisesRegex(mod.ConfigError, "flags-2-env contract is missing"):
            mod.load_config(root / ".ores-rpc.toml", {})

    def test_bad_env_override_is_rejected_without_echoing_value(self):
        tmp, root = self.make_repo()
        self.addCleanup(tmp.cleanup)
        with self.assertRaisesRegex(mod.ConfigError, "must be an unsigned decimal integer"):
            mod.load_config(
                root / ".ores-rpc.toml",
                {"ORES_RPC_MAX_FRAME_BYTES": "not-a-number"},
            )


if __name__ == "__main__":
    unittest.main()
