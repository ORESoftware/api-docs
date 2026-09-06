#!/usr/bin/env python3
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "install_into", ROOT / "scripts" / "install-into.py"
)
mod = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(mod)


class InstallInto(unittest.TestCase):
    def test_generated_workflow_resolves_each_sibling_before_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            rc = mod.main(
                [
                    str(target),
                    "--checkout",
                    "Example/api-server.rs",
                    "Example/lib-core",
                    "--no-hooks",
                ]
            )
            self.assertEqual(rc, 0)

            workflow = (
                target / ".github" / "workflows" / "route-map-sync.yml"
            ).read_text(encoding="utf-8")
            self.assertNotIn("ubuntu-latest", workflow)
            self.assertNotIn("actions/checkout@v", workflow)
            self.assertNotIn("actions/setup-python@v", workflow)
            self.assertIn(mod.CHECKOUT_ACTION, workflow)
            self.assertIn(mod.SETUP_PYTHON_ACTION, workflow)
            self.assertIn(
                "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
                workflow,
            )
            self.assertIn("path: source", workflow)
            self.assertIn("working-directory: source", workflow)
            self.assertIn(
                "MATCHING_REF: ${{ github.head_ref || github.ref_name }}",
                workflow,
            )
            self.assertIn("--fallback-ref main", workflow)
            self.assertEqual(
                workflow.count(
                    '--resolution-log "$GITHUB_WORKSPACE/.ridl/sibling-resolution.jsonl"'
                ),
                2,
            )
            self.assertIn(
                '--explain-log "$GITHUB_WORKSPACE/.ridl/sibling-resolution.jsonl"',
                workflow,
            )
            self.assertIn(
                ': > "$GITHUB_WORKSPACE/.ridl/sibling-resolution.jsonl"',
                workflow,
            )
            self.assertIn(
                "ref: ${{ steps.resolve_sibling_0.outputs.ref }}", workflow
            )
            self.assertIn(
                "ref: ${{ steps.resolve_sibling_1.outputs.ref }}", workflow
            )
            self.assertIn("path: api-server.rs", workflow)
            self.assertIn("path: lib-core", workflow)
            self.assertNotIn("path: ../", workflow)
            self.assertEqual(
                workflow.count(
                    "token: ${{ secrets.ROUTE_SYNC_GITHUB_TOKEN || github.token }}"
                ),
                2,
            )
            self.assertEqual(
                workflow.count(
                    "GITHUB_TOKEN: ${{ secrets.ROUTE_SYNC_GITHUB_TOKEN || github.token }}"
                ),
                2,
            )
            self.assertLess(
                workflow.index(
                    "Resolve Example/api-server.rs at matching branch before main"
                ),
                workflow.index(
                    "Check out Example/api-server.rs at resolved revision"
                ),
            )

            manifest = json.loads(
                (
                    target / "scripts" / "vendor" / "MANIFEST.json"
                ).read_text(encoding="utf-8")
            )
            self.assertIn("scripts/resolve-sibling-ref.py", manifest["files"])
            self.assertTrue(
                (target / "scripts" / "resolve-sibling-ref.py").stat().st_mode
                & 0o111
            )

    def test_invalid_checkout_slug_fails_before_installing(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                rc = mod.main(
                    [
                        str(target),
                        "--checkout",
                        "Example/sibling\nrun: unsafe",
                        "--no-hooks",
                    ]
                )
            self.assertEqual(rc, 2)
            self.assertIn("invalid --checkout", stderr.getvalue())
            self.assertFalse((target / "ridl.json").exists())


if __name__ == "__main__":
    unittest.main()
