#!/usr/bin/env python3
"""Tests for the v2 route-sync gate.

Every case here is a defect the v1 regex checker actually had. Measured against
`canonical-web-server.rs`, v1 reported 19 paths and most were wrong: it dropped
every `nest()` prefix, skipped registrations `cargo fmt` had wrapped, ignored
`any(...)` routes entirely, and picked up a `#[cfg(test)]` router in their place.
A gate that confidently reports the wrong route set is worse than no gate, so
these stay as regression tests rather than living only in a commit message.

    python3 scripts/test_check_route_sync.py -v
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("check_route_sync", HERE / "check-route-sync.py")
assert _spec and _spec.loader
check_route_sync = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(check_route_sync)

ROOT = HERE.parent


def scan(files: dict[str, str]) -> "check_route_sync.SourceIndex":
    """Scan a synthetic crate laid out exactly like a real one."""
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "src"
        for rel, text in files.items():
            target = src / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text, encoding="utf-8")
        return check_route_sync.scan_rust([src])


class Scanning(unittest.TestCase):
    def test_a_rustfmt_wrapped_registration_is_still_found(self) -> None:
        """v1 required the path literal and the verb on one physical line, so
        `cargo fmt` wrapping a registration turned 'registered' into 'missing'."""
        index = scan({
            "mod.rs": """
                pub fn router() -> Router {
                    Router::new()
                        .route(
                            "/v1/matters/{id}/walk",
                            post(walk_matter),
                        )
                }
            """
        })
        self.assertIn("/v1/matters/{id}/walk", index.routes)
        self.assertEqual(index.routes["/v1/matters/{id}/walk"], {"POST"})

    def test_nest_prefixes_are_applied(self) -> None:
        """v1 had no model of router composition, so a route registered as
        `/health` inside `nest("/api", nest("/v1", ..))` was recorded as
        `/health` rather than `/api/v1/health`."""
        index = scan({
            "mod.rs": """
                pub fn router() -> Router {
                    Router::new().nest("/api", api::router())
                }
            """,
            "api/mod.rs": """
                pub fn router() -> Router {
                    Router::new().route("/info", get(info)).nest("/v1", v1::router())
                }
            """,
            "api/v1/mod.rs": """
                pub fn router() -> Router {
                    Router::new().route("/health", get(health))
                }
            """,
        })
        self.assertIn("/api/v1/health", index.routes)
        self.assertIn("/api/info", index.routes)
        self.assertNotIn("/health", index.routes)

    def test_a_cfg_test_router_is_not_mistaken_for_production(self) -> None:
        """The only `/ws` GET v1 ever saw came from a `#[cfg(test)]` router,
        while both production registrations were dropped."""
        index = scan({
            "mod.rs": """
                pub fn router() -> Router {
                    Router::new().route("/ws", any(upgrade))
                }

                #[cfg(test)]
                mod tests {
                    fn test_router() -> Router {
                        Router::new().route("/only-in-tests", get(fixture))
                    }
                }
            """
        })
        self.assertIn("/ws", index.routes)
        self.assertNotIn("/only-in-tests", index.routes)

    def test_an_any_route_is_captured_not_dropped(self) -> None:
        """v1 knew the seven verb helpers and not `any`, and a registration with
        no recognised verb was skipped entirely -- silently."""
        index = scan({"mod.rs": 'pub fn router() -> Router { Router::new().route("/ws", any(upgrade)) }'})
        self.assertEqual(index.routes.get("/ws"), {"ANY"})

    def test_both_path_parameter_spellings_normalize(self) -> None:
        """Axum 0.7 spells it `:id`, 0.8 and every route map spell it `{id}`."""
        self.assertEqual(check_route_sync.normalize_path("/legacy/:id"), "/legacy/{id}")
        self.assertEqual(check_route_sync.normalize_path("/v1/{id}"), "/v1/{id}")

    def test_join_path_edges(self) -> None:
        self.assertEqual(check_route_sync.join_path("/api", "/v1"), "/api/v1")
        self.assertEqual(check_route_sync.join_path("", "/x"), "/x")
        self.assertEqual(check_route_sync.join_path("/", "/x"), "/x")
        self.assertEqual(check_route_sync.join_path("/api", "/"), "/api")


class Identical(unittest.TestCase):
    def test_identical_compares_bytes_not_parsed_json(self) -> None:
        """v1's message said 'byte-for-byte' while it compared parsed objects,
        so two published copies could differ in key order and still pass."""
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "a.json", Path(tmp) / "b.json"
            a.write_text('{"x":1,"y":2}')
            b.write_text('{"y":2,"x":1}')
            self.assertTrue(check_route_sync.identical(a, b))
            b.write_text('{"x":1,"y":2}')
            self.assertEqual(check_route_sync.identical(a, b), [])


class Repository(unittest.TestCase):
    def test_the_repo_is_in_sync(self) -> None:
        """The gate run over this repository, the way CI runs it."""
        self.assertEqual(check_route_sync.run(["--root", str(ROOT)]), 0)

    def test_config_is_discoverable(self) -> None:
        self.assertIsInstance(check_route_sync.load_config(ROOT), dict)


if __name__ == "__main__":
    unittest.main(verbosity=2)
