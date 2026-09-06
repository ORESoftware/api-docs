#!/usr/bin/env python3
"""Vendor the ridl toolchain into a consumer repo.

Every repo that owns or publishes a route map needs the same four things: the
`ridl` package, the sync checker, a CI workflow, and the git hooks. Copying them
by hand is how the previous generation ended up with four divergent copies of a
303-line checker and eight copies of a hook, none of which was verified to match
the others. This script makes the copy reproducible and re-runnable, and writes
a manifest so the gate can tell whether a vendored copy has drifted.

    ./scripts/install-into.py ../../premarital-asset-protection/pmap-lib-core \
        --maps route-maps/api.route-map.json route-maps/web.route-map.json \
        --sources ../pmap-api-server.rs/src
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

SOURCE_ROOT = Path(__file__).resolve().parent.parent

WORKFLOW = """\
name: route-map-sync

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
{extra_checkouts}\
      - uses: actions/setup-python@v5
        with:
          python-version: '3.11'
      # No pip install: the checker and generator are standard library only, so
      # CI and a developer laptop cannot reach different verdicts.
      - name: Validate the route maps and the generated clients
        run: python3 scripts/check-route-sync.py --root .
"""

HOOK = """\
#!/bin/sh
# Installed by ridl. Runs the route-map gate before {when}.
#
# The gate validates every route map, checks the handlers against it, and fails
# if the generated clients no longer match. Regenerate with `ridl generate`
# rather than editing generated files.
set -e
root=$(git rev-parse --show-toplevel)
script="$root/scripts/check-route-sync.py"
if [ ! -f "$script" ]; then
  echo "route-map gate: $script is missing; skipping" >&2
  exit 0
fi
exec python3 "$script" --root "$root"
"""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_tree(src: Path, dst: Path, manifest: dict[str, str]) -> None:
    for item in sorted(src.rglob("*")):
        if item.is_dir() or "__pycache__" in item.parts:
            continue
        target = dst / item.relative_to(src)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item, target)
        manifest[str(target.relative_to(dst.parent.parent))] = sha256(target)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", type=Path, help="repo to install into")
    parser.add_argument(
        "--maps", nargs="*", default=[],
        help="a map path, or MAP=SRC1,SRC2 to pair one map with the sources that serve it",
    )
    parser.add_argument("--sources", nargs="*", default=[])
    parser.add_argument("--identical-to", nargs="*", default=[])
    parser.add_argument("--languages", nargs="*", default=[])
    parser.add_argument("--out", default="generated")
    parser.add_argument("--skip-source", action="store_true")
    parser.add_argument("--skip-drift", action="store_true")
    parser.add_argument("--allow-docs-merge", action="store_true")
    parser.add_argument(
        "--checkout", nargs="*", default=[],
        help="extra owner/repo to check out in CI (siblings the gate compares against)",
    )
    parser.add_argument("--no-hooks", action="store_true")
    args = parser.parse_args(argv)

    target = args.target.resolve()
    if not target.is_dir():
        print(f"no such repo: {target}", file=sys.stderr)
        return 1

    manifest: dict[str, str] = {}

    vendor = target / "scripts" / "vendor"
    vendor.mkdir(parents=True, exist_ok=True)
    copy_tree(SOURCE_ROOT / "ridl", vendor / "ridl", manifest)

    # v1 consumers historically copied a single route-map schema here. Keep that
    # path current so a re-run of this installer cannot leave a pre-NATS vendor.
    schema_v1 = SOURCE_ROOT / "json-schema" / "route-map.schema.json"
    if schema_v1.is_file():
        dest = vendor / "route-map.schema.json"
        shutil.copy2(schema_v1, dest)
        manifest["scripts/vendor/route-map.schema.json"] = sha256(dest)
    schema_dir = vendor / "json-schema"
    schema_dir.mkdir(parents=True, exist_ok=True)
    for name in (
        "route-map.schema.json",
        "route-map-v2.schema.json",
        "rpc-call.schema.json",
        "rpc-receipt.schema.json",
        "rpc-frame.schema.json",
        "opto-sync-envelope.schema.json",
        "telemetry-attributes.schema.json",
    ):
        src = SOURCE_ROOT / "json-schema" / name
        if not src.is_file():
            continue
        dest = schema_dir / name
        shutil.copy2(src, dest)
        manifest[f"scripts/vendor/json-schema/{name}"] = sha256(dest)

    checker = target / "scripts" / "check-route-sync.py"
    shutil.copy2(SOURCE_ROOT / "scripts" / "check-route-sync.py", checker)
    checker.chmod(0o755)
    manifest["scripts/check-route-sync.py"] = sha256(checker)

    runtime_src = SOURCE_ROOT / "runtime"
    if runtime_src.is_dir():
        copy_tree(runtime_src, target / "scripts" / "vendor" / "runtime", manifest)

    launcher = target / "scripts" / "ridl"
    launcher.write_text(
        "#!/usr/bin/env python3\n"
        '"""Run the vendored ridl toolchain: `scripts/ridl check|generate|drift`."""\n'
        "import sys\n"
        "from pathlib import Path\n"
        "\n"
        "ROOT = Path(__file__).resolve().parent.parent\n"
        'sys.path.insert(0, str(ROOT / "scripts" / "vendor"))\n'
        "\n"
        "from ridl.cli import run\n"
        "\n"
        'if __name__ == "__main__":\n'
        '    argv = sys.argv[1:]\n'
        '    if "--root" not in argv:\n'
        '        argv += ["--root", str(ROOT)]\n'
        "    raise SystemExit(run(argv))\n",
        encoding="utf-8",
    )
    launcher.chmod(0o755)
    manifest["scripts/ridl"] = sha256(launcher)

    maps: list[object] = []
    for entry in args.maps:
        if "=" in entry:
            path, _, sources = entry.partition("=")
            maps.append({"path": path, "sources": [s for s in sources.split(",") if s]})
        else:
            maps.append(entry)

    config = {
        "maps": maps,
        "sources": args.sources,
        "identical_to": args.identical_to,
        "languages": args.languages,
        "out": args.out,
    }
    if args.skip_source:
        config["skip_source"] = True
    if args.skip_drift:
        config["skip_drift"] = True
    if args.allow_docs_merge:
        config["allow_docs_merge"] = True
    config = {k: v for k, v in config.items() if v not in ([], None)}
    rendered = json.dumps(config, indent=2) + "\n"
    (target / "ridl.json").write_text(rendered, encoding="utf-8")
    # A repo migrating from v1 still has route-sync.json. Write the same content
    # there rather than leaving a second, older answer lying around.
    legacy = target / "route-sync.json"
    if legacy.is_file():
        legacy.write_text(rendered, encoding="utf-8")

    extra = "".join(
        f"      - uses: actions/checkout@v4\n"
        f"        with:\n"
        f"          repository: {repo}\n"
        f"          path: ../{repo.split('/')[-1]}\n"
        for repo in args.checkout
    )
    workflows = target / ".github" / "workflows"
    workflows.mkdir(parents=True, exist_ok=True)
    (workflows / "route-map-sync.yml").write_text(
        WORKFLOW.format(extra_checkouts=extra), encoding="utf-8"
    )

    if not args.no_hooks:
        hooks = target / ".githooks"
        hooks.mkdir(exist_ok=True)
        for name, when in (("pre-commit", "a commit"), ("pre-push", "a push")):
            path = hooks / name
            path.write_text(HOOK.format(when=when), encoding="utf-8")
            path.chmod(0o755)

    (target / "scripts" / "vendor" / "MANIFEST.json").write_text(
        json.dumps(
            {
                "source": "ORESoftware/api-docs",
                "note": "Regenerate with api-docs/scripts/install-into.py; do not edit in place.",
                "files": dict(sorted(manifest.items())),
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )

    print(f"ridl installed into {target} ({len(manifest)} vendored files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
