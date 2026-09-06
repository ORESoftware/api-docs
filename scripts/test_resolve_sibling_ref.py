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
    "resolve_sibling_ref", ROOT / "scripts" / "resolve-sibling-ref.py"
)
mod = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(mod)


class ResolveSiblingRef(unittest.TestCase):
    def test_matching_branch_wins_without_querying_fallback(self):
        calls: list[str] = []

        def lookup(repository: str, ref: str, token: str | None, api_url: str) -> int:
            self.assertEqual(repository, "Example/sibling")
            self.assertEqual(token, "token")
            self.assertEqual(api_url, "https://github.example/api/v3")
            calls.append(ref)
            return 200

        resolution = mod.resolve_ref(
            "Example/sibling",
            "DEN-4078-route-sync",
            token="token",
            api_url="https://github.example/api/v3",
            status_lookup=lookup,
        )
        self.assertEqual(resolution.ref, "DEN-4078-route-sync")
        self.assertEqual(resolution.source, "matching")
        self.assertEqual(calls, ["DEN-4078-route-sync"])

    def test_missing_matching_branch_falls_back_to_main(self):
        statuses = {"DEN-4078-route-sync": 404, "main": 200}
        resolution = mod.resolve_ref(
            "Example/sibling",
            "DEN-4078-route-sync",
            status_lookup=lambda _repo, ref, _token, _api: statuses[ref],
        )
        self.assertEqual(resolution.ref, "main")
        self.assertEqual(resolution.source, "fallback")
        self.assertEqual(resolution.matching_status, 404)
        self.assertEqual(resolution.fallback_status, 200)

    def test_access_failure_is_not_disguised_as_missing_branch(self):
        with self.assertRaisesRegex(mod.ResolutionError, "HTTP 403"):
            mod.resolve_ref(
                "Example/sibling",
                "DEN-4078-route-sync",
                status_lookup=lambda *_args: 403,
            )

    def test_both_missing_or_unreadable_fails_closed(self):
        with self.assertRaisesRegex(mod.ResolutionError, "neither"):
            mod.resolve_ref(
                "Example/sibling",
                "DEN-4078-route-sync",
                status_lookup=lambda *_args: 404,
            )

    def test_branch_url_encodes_slashes(self):
        self.assertEqual(
            mod.branch_url(
                "https://api.github.com/",
                "Example/sibling",
                "DEN-4078/route-sync",
            ),
            "https://api.github.com/repos/Example/sibling/branches/"
            "DEN-4078%2Froute-sync",
        )

    def test_cli_writes_outputs_records_and_explains_fallback(self):
        statuses = {"DEN-4078-route-sync": 404, "main": 200}
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "github-output"
            log = Path(tmp) / ".ridl" / "sibling-resolution.jsonl"
            stdout = io.StringIO()
            stderr = io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                rc = mod.run(
                    [
                        "--repository",
                        "Example/sibling",
                        "--matching-ref",
                        "DEN-4078-route-sync",
                        "--resolution-log",
                        str(log),
                    ],
                    environ={
                        "GITHUB_OUTPUT": str(output),
                        "GITHUB_TOKEN": "token",
                    },
                    status_lookup=lambda _repo, ref, _token, _api: statuses[ref],
                )
            self.assertEqual(rc, 0, stderr.getvalue())
            self.assertEqual(output.read_text(), "ref=main\nsource=fallback\n")
            record = json.loads(log.read_text())
            self.assertEqual(
                record,
                {
                    "fallback_ref": "main",
                    "matching_ref": "DEN-4078-route-sync",
                    "repository": "Example/sibling",
                    "resolved_ref": "main",
                    "source": "fallback",
                },
            )
            self.assertIn("no readable matching branch", stdout.getvalue())
            self.assertIn("sibling PR", stdout.getvalue())
            self.assertEqual(stderr.getvalue(), "")

            explanation = io.StringIO()
            with contextlib.redirect_stderr(explanation):
                rc = mod.run(
                    ["--explain-log", str(log)],
                    environ={},
                )
            self.assertEqual(rc, 0)
            self.assertIn("used fallback main", explanation.getvalue())
            self.assertIn("not been pushed or merged", explanation.getvalue())

    def test_matching_resolution_context_is_unambiguous(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "resolution.jsonl"
            mod.write_resolution_log(
                log,
                "Example/sibling",
                "DEN-4078-route-sync",
                "main",
                mod.Resolution("DEN-4078-route-sync", "matching", 200, None),
            )
            self.assertEqual(
                mod.resolution_context_lines(log),
                [
                    "route-map-sync sibling context: Example/sibling was checked "
                    "at matching branch DEN-4078-route-sync."
                ],
            )

    def test_invalid_repository_slug_fails_closed(self):
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            rc = mod.run(
                [
                    "--repository",
                    "Example/sibling\nrun: unsafe",
                    "--matching-ref",
                    "main",
                ],
                environ={},
                status_lookup=lambda *_args: 200,
            )
        self.assertEqual(rc, 1)
        self.assertIn("invalid repository", stderr.getvalue())

    def test_explain_mode_rejects_resolution_arguments(self):
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            rc = mod.run(
                [
                    "--explain-log",
                    "resolution.jsonl",
                    "--repository",
                    "Example/sibling",
                ],
                environ={},
            )
        self.assertEqual(rc, 2)
        self.assertIn("cannot be combined", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
