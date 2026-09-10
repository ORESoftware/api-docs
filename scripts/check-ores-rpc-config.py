from __future__ import annotations

import copy
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CONFIG_PATH = ROOT / ".ores-rpc.toml"
INSTANCE_PATH = ROOT / "tmp/ores-rpc-config-instances/OresRpcConfig/valid/api-docs.json"
ENV_NAME = re.compile(r"^[A-Z][A-Z0-9_]*$")
TARGET_NAME = re.compile(r"^[a-z][A-Za-z0-9]*(?:\.[a-z][A-Za-z0-9]*)+$")
EXPECTED_TOP = {
    "schemaVersion", "repositoryMode", "strict", "tjsv", "flags2env",
    "rpc", "client", "server", "environment",
}


class ConfigError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ConfigError(message)


def exact_keys(value: object, expected: set[str], where: str) -> dict:
    require(isinstance(value, dict), f"{where} must be a table")
    actual = set(value)
    require(
        actual == expected,
        f"{where} keys differ: expected={sorted(expected)} actual={sorted(actual)}",
    )
    return value


def safe_repo_path(value: object, where: str) -> str:
    require(isinstance(value, str) and value != "", f"{where} must be a non-empty string")
    path = PurePosixPath(value)
    require(not path.is_absolute(), f"{where} must be repository-relative")
    require(".." not in path.parts, f"{where} must not traverse parents")
    require("\\" not in value and "\x00" not in value, f"{where} contains unsafe path syntax")
    return value


def parse_uint32(raw: str, where: str) -> int:
    require(
        re.fullmatch(r"(?:0|[1-9][0-9]{0,9})", raw) is not None,
        f"{where} must be an unsigned decimal integer",
    )
    value = int(raw, 10)
    require(1 <= value <= 16 * 1024 * 1024, f"{where} outside 1..16777216")
    return value


def set_target(config: dict, target: str, value: object) -> None:
    parts = target.split(".")
    node: object = config
    for part in parts[:-1]:
        require(
            isinstance(node, dict) and part in node,
            f"environment target has unknown parent: {target}",
        )
        node = node[part]
    require(
        isinstance(node, dict) and parts[-1] in node,
        f"environment target is unknown: {target}",
    )
    node[parts[-1]] = value


def load_config(path: Path = CONFIG_PATH, environ: dict[str, str] | None = None) -> dict:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    exact_keys(data, EXPECTED_TOP, "root")
    require(data["schemaVersion"] == "ores.rpc.config/v1", "unsupported schemaVersion")
    require(
        data["repositoryMode"] in {"client-only", "server-only", "combined"},
        "unsupported repositoryMode",
    )
    require(data["strict"] is True, "strict must remain true")

    tjsv = exact_keys(data["tjsv"], {"repository", "consumerLock", "failClosed"}, "tjsv")
    require(
        tjsv["repository"] == "ORESoftware/typespec-json-schema-validator",
        "wrong TJSV repository",
    )
    require(tjsv["failClosed"] is True, "TJSV admission must fail closed")
    lock_rel = safe_repo_path(tjsv["consumerLock"], "tjsv.consumerLock")
    lock_path = path.parent / lock_rel
    require(lock_path.is_file(), "TJSV consumer lock does not exist")
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    require(
        lock.get("repository") == tjsv["repository"],
        "TJSV consumer lock repository mismatch",
    )

    f2e = exact_keys(
        data["flags2env"],
        {
            "provider", "contract", "contractPresent", "requiredWhenExecutable",
            "rejectUnknownFlags", "secretsFromEnvironmentOnly", "precedence",
        },
        "flags2env",
    )
    require(f2e["provider"] == "flags-2-env", "flags2env.provider must be flags-2-env")
    require(
        f2e["requiredWhenExecutable"] is True,
        "executables must normalize argv through flags-2-env",
    )
    require(f2e["rejectUnknownFlags"] is True, "unknown flags must fail closed")
    require(
        f2e["secretsFromEnvironmentOnly"] is True,
        "secret values must remain environment/secret-store only",
    )
    require(
        f2e["precedence"] == "flags>env>toml",
        "configuration precedence must remain flags>env>toml",
    )
    contract_rel = safe_repo_path(f2e["contract"], "flags2env.contract")
    require(
        isinstance(f2e["contractPresent"], bool),
        "flags2env.contractPresent must be boolean",
    )
    if f2e["contractPresent"]:
        require(
            (path.parent / contract_rel).is_file(),
            "declared flags-2-env contract is missing",
        )

    rpc = exact_keys(
        data["rpc"],
        {"routeMapGlob", "contractBundle", "transports", "framings", "maxFrameBytes"},
        "rpc",
    )
    route_glob = safe_repo_path(rpc["routeMapGlob"], "rpc.routeMapGlob")
    require("*" in route_glob, "rpc.routeMapGlob must identify a bounded family")
    require(any(path.parent.glob(route_glob)), "rpc.routeMapGlob matched no route maps")
    bundle_rel = safe_repo_path(rpc["contractBundle"], "rpc.contractBundle")
    require((path.parent / bundle_rel).is_file(), "rpc.contractBundle does not exist")
    require(
        rpc["transports"] == ["http", "tcp", "websocket", "nats"],
        "transport inventory drift",
    )
    require(
        rpc["framings"] == ["json", "ndjson", "length-prefixed"],
        "framing inventory drift",
    )
    require(
        isinstance(rpc["maxFrameBytes"], int) and not isinstance(rpc["maxFrameBytes"], bool),
        "rpc.maxFrameBytes must be integer",
    )
    require(
        1 <= rpc["maxFrameBytes"] <= 16 * 1024 * 1024,
        "rpc.maxFrameBytes outside safety bound",
    )

    client = exact_keys(data["client"], {"enabled", "root"}, "client")
    server = exact_keys(data["server"], {"enabled", "root"}, "server")
    for role, section in (("client", client), ("server", server)):
        require(isinstance(section["enabled"], bool), f"{role}.enabled must be boolean")
        root_rel = safe_repo_path(section["root"], f"{role}.root")
        if section["enabled"]:
            require((path.parent / root_rel).is_dir(), f"enabled {role} root does not exist")
    mode_expected = {
        "client-only": (True, False),
        "server-only": (False, True),
        "combined": (True, True),
    }[data["repositoryMode"]]
    require(
        (client["enabled"], server["enabled"]) == mode_expected,
        "repositoryMode contradicts client/server enablement",
    )

    env_rows = data["environment"]
    require(
        isinstance(env_rows, list) and env_rows,
        "environment must be a non-empty array of tables",
    )
    names: set[str] = set()
    targets: set[str] = set()
    normalized = copy.deepcopy(data)
    for index, row in enumerate(env_rows):
        row = exact_keys(
            row,
            {"name", "target", "valueType", "source", "secret"},
            f"environment[{index}]",
        )
        name = row["name"]
        target = row["target"]
        require(
            isinstance(name, str) and ENV_NAME.fullmatch(name) is not None,
            f"invalid environment name at index {index}",
        )
        require(
            isinstance(target, str) and TARGET_NAME.fullmatch(target) is not None,
            f"invalid environment target at index {index}",
        )
        require(
            name not in names and target not in targets,
            "environment names and targets must be unique",
        )
        names.add(name)
        targets.add(target)
        require(row["source"] == "flags-2-env", "environment source must be flags-2-env")
        require(
            row["secret"] is False,
            "secret values may not be represented in .ores-rpc.toml",
        )
        require(row["valueType"] == "uint32", "unsupported environment valueType")
        if environ is not None and name in environ:
            set_target(normalized, target, parse_uint32(environ[name], name))
    return normalized


def write_instance(config: dict, output: Path = INSTANCE_PATH) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(config, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    if len(sys.argv) != 1:
        raise ConfigError(
            "fixed checker accepts no command-line arguments; argv normalization belongs to flags-2-env"
        )
    config = load_config(CONFIG_PATH, dict(os.environ))
    write_instance(config)
    print(
        json.dumps(
            {
                "schema": "ores.api-docs.ores-rpc-config-check/v1",
                "status": "passed",
                "instance": str(INSTANCE_PATH.relative_to(ROOT)),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ConfigError, OSError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(
            json.dumps(
                {
                    "schema": "ores.api-docs.ores-rpc-config-check/v1",
                    "status": "failed",
                    "error": str(error),
                }
            ),
            file=sys.stderr,
        )
        raise SystemExit(3)
