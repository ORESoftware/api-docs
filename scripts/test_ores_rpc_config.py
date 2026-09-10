from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

MODULE_PATH = Path(__file__).with_name("ores_rpc_config.py")
SPEC = importlib.util.spec_from_file_location("ores_rpc_config", MODULE_PATH)
assert SPEC and SPEC.loader
rpc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rpc)


BASE = '''schemaVersion = "ores.rpc.config.v1"
repositoryMode = "hybrid"
strict = true
flagsContract = ".cli-flags.toml"

[[env]]
name = "apiBaseUrl"
env = "API_BASE_URL"
valueType = "string"
required = true
secret = false
allowArgv = true

[[env]]
name = "serviceToken"
env = "SERVICE_TOKEN"
valueType = "string"
required = true
secret = true
allowArgv = false

[[targets]]
name = "api"
role = "server"
roots = ["server"]
rpcVersion = "v1"
transports = ["http"]
framing = "json"
routeMap = "contracts/routes.json"

[[targets]]
name = "browser"
role = "client"
roots = ["web"]
rpcVersion = "v1"
transports = ["http", "websocket"]
framing = "json"
endpointEnv = "API_BASE_URL"
propagateHeaders = ["traceparent", "x-request-id"]
'''


class OresRpcConfigTests(unittest.TestCase):
    def repo(self) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        tmp = tempfile.TemporaryDirectory()
        root = Path(tmp.name)
        (root / "server").mkdir()
        (root / "web").mkdir()
        (root / "contracts").mkdir()
        (root / "contracts/routes.json").write_text("{}\n", encoding="utf-8")
        (root / ".cli-flags.toml").write_text("[flags]\n", encoding="utf-8")
        return tmp, root

    def parse(self, text: str = BASE):
        tmp, root = self.repo()
        self.addCleanup(tmp.cleanup)
        path = root / ".ores-rpc.toml"
        path.write_text(text, encoding="utf-8")
        return rpc.load_config(path, root), root

    def reject(self, text: str, code: str):
        with self.assertRaises(rpc.ConfigError) as ctx:
            self.parse(text)
        self.assertEqual(ctx.exception.code, code)

    def test_valid_separate_root_hybrid(self):
        config, _ = self.parse()
        self.assertEqual(config["repositoryMode"], "hybrid")
        self.assertEqual(rpc.resolve_target(config, target_name=None, source_path="server/src/main.rs")["name"], "api")
        self.assertEqual(rpc.resolve_target(config, target_name=None, source_path="web/src/app.ts")["name"], "browser")

    def test_same_root_requires_explicit_target_when_ambiguous(self):
        text = '''schemaVersion = "ores.rpc.config.v1"
repositoryMode = "hybrid"
strict = true
allowOverlappingRoots = true
[[targets]]
name = "server"
role = "server"
roots = ["."]
rpcVersion = "v1"
transports = ["http"]
framing = "json"
[[targets]]
name = "client"
role = "client"
roots = ["."]
rpcVersion = "v1"
transports = ["http"]
framing = "json"
'''
        config, _ = self.parse(text)
        self.assertEqual(rpc.resolve_target(config, target_name="server", source_path=None)["role"], "server")
        with self.assertRaises(rpc.ConfigError) as ctx:
            rpc.resolve_target(config, target_name=None, source_path="server/file.rs")
        self.assertEqual(ctx.exception.code, "target-ambiguous")

    def test_overlap_fails_without_opt_in(self):
        self.reject(BASE.replace('roots = ["web"]', 'roots = ["server/sub"]'), "root-overlap")

    def test_secret_cannot_be_argv(self):
        self.reject(BASE.replace('secret = true\nallowArgv = false', 'secret = true\nallowArgv = true'), "secret-argv")

    def test_argv_binding_requires_flags_contract(self):
        self.reject(BASE.replace('flagsContract = ".cli-flags.toml"\n', ''), "argv-without-flags-contract")

    def test_endpoint_env_is_env_name_not_literal_url(self):
        self.reject(BASE.replace('endpointEnv = "API_BASE_URL"', 'endpointEnv = "https://example.test"'), "endpoint-env")

    def test_endpoint_env_must_be_declared(self):
        self.reject(BASE.replace('endpointEnv = "API_BASE_URL"', 'endpointEnv = "OTHER_URL"'), "endpoint-env-undeclared")

    def test_server_rejects_client_fields(self):
        self.reject(BASE.replace('routeMap = "contracts/routes.json"', 'routeMap = "contracts/routes.json"\nendpointEnv = "API_BASE_URL"'), "server-client-field")

    def test_v1_tcp_must_be_single_transport(self):
        text = BASE.replace('transports = ["http"]\nframing = "json"\nrouteMap', 'transports = ["http", "tcp"]\nframing = "length-prefixed"\nrouteMap', 1)
        self.reject(text, "v1-tcp-framing")

    def test_v1_tcp_length_prefix_is_valid(self):
        text = BASE.replace('transports = ["http"]\nframing = "json"\nrouteMap', 'transports = ["tcp"]\nframing = "length-prefixed"\nrouteMap', 1)
        config, _ = self.parse(text)
        self.assertEqual(config["targets"][0]["framing"], "length-prefixed")

    def test_v2_requires_frame(self):
        text = BASE.replace('rpcVersion = "v1"\ntransports = ["http"]\nframing = "json"', 'rpcVersion = "v2"\ntransports = ["http"]\nframing = "json"', 1)
        self.reject(text, "v2-frame-required")

    def test_v2_nats_is_not_claimed_supported(self):
        text = BASE.replace('rpcVersion = "v1"\ntransports = ["http"]\nframing = "json"', 'rpcVersion = "v2"\ntransports = ["nats"]\nframing = "frame"', 1)
        self.reject(text, "v2-nats-unsupported")

    def test_unknown_keys_fail_closed(self):
        self.reject(BASE.replace('strict = true', 'strict = true\nmystery = true'), "config-unknown-key")
        self.reject(BASE.replace('name = "api"\nrole', 'name = "api"\nmystery = true\nrole', 1), "target-unknown-key")
        self.reject(BASE.replace('name = "apiBaseUrl"\nenv', 'name = "apiBaseUrl"\nmystery = true\nenv', 1), "env-unknown-key")

    def test_unsafe_paths_are_rejected(self):
        for bad in ["../server", "/server", "server\\sub", "C:/server"]:
            with self.subTest(path=bad):
                self.reject(BASE.replace('roots = ["server"]', f'roots = ["{bad}"]', 1), "target-root-path")
        self.reject(BASE.replace('routeMap = "contracts/routes.json"', 'routeMap = "../routes.json"'), "route-map-path")

    def test_repository_mode_mismatch(self):
        self.reject(BASE.replace('repositoryMode = "hybrid"', 'repositoryMode = "server-only"'), "repository-role-mismatch")

    def test_duplicates_rejected(self):
        self.reject(BASE.replace('name = "browser"', 'name = "api"'), "target-duplicate")
        self.reject(BASE.replace('env = "SERVICE_TOKEN"', 'env = "API_BASE_URL"'), "env-duplicate")
        self.reject(BASE.replace('["traceparent", "x-request-id"]', '["traceparent", "traceparent"]'), "propagate-headers")

    @unittest.skipIf(not hasattr(Path, "symlink_to"), "symlink unavailable")
    def test_symlinked_route_map_rejected(self):
        tmp, root = self.repo()
        self.addCleanup(tmp.cleanup)
        target = root / "real.json"
        target.write_text("{}\n", encoding="utf-8")
        link = root / "contracts/routes.json"
        link.unlink()
        try:
            link.symlink_to(target)
        except OSError:
            self.skipTest("symlinks unavailable")
        path = root / ".ores-rpc.toml"
        path.write_text(BASE, encoding="utf-8")
        with self.assertRaises(rpc.ConfigError) as ctx:
            rpc.load_config(path, root)
        self.assertEqual(ctx.exception.code, "path-symlink")

    def test_malformed_toml_reports_bounded_code(self):
        tmp, root = self.repo()
        self.addCleanup(tmp.cleanup)
        path = root / ".ores-rpc.toml"
        path.write_text('schemaVersion = "unterminated\nSECRET=never-echo\n', encoding="utf-8")
        with self.assertRaises(rpc.ConfigError) as ctx:
            rpc.load_config(path, root)
        self.assertEqual(ctx.exception.code, "config-syntax")
        self.assertNotIn("SECRET", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
