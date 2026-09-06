#!/usr/bin/env python3
"""Conformance tests for the ridl frame envelope.

The fixtures under `examples/frames/` are the contract between this reference
implementation and every port. Run the ports' own suites too:

    node --experimental-strip-types --test runtime/typescript/frame.conformance.test.ts
    cargo test --manifest-path runtime/rust/Cargo.toml frame
"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from ridl.framing import (  # noqa: E402
    MAX_FRAME_BYTES,
    Correlator,
    Frame,
    FrameError,
    decode,
    decode_stream,
)

FIXTURES = json.loads(
    (Path(__file__).resolve().parent.parent / "examples/frames/conformance.json").read_text("utf-8")
)


class Conformance(unittest.TestCase):
    def test_every_fixture_round_trips(self) -> None:
        for case in FIXTURES["cases"]:
            with self.subTest(case["name"]):
                frame = decode(case["encoded"])
                self.assertEqual(frame.encode().decode("utf-8"), case["encoded"])
                self.assertEqual(frame.encode_tcp()[:4].hex(), case["tcp_prefix_hex"])

    def test_member_order_is_fixed_not_alphabetical(self) -> None:
        encoded = Frame.call("1", "healthz", "GET", "/healthz").encode().decode("utf-8")
        self.assertTrue(encoded.startswith('{"v":1,"id":"1","t":"call","key":'), encoded)

    def test_absent_body_and_null_body_stay_distinguishable(self) -> None:
        self.assertFalse(decode('{"v":1,"id":"1","t":"end"}').has_body)
        null_body = decode('{"v":1,"id":"1","t":"data","body":null}')
        self.assertTrue(null_body.has_body)
        self.assertIsNone(null_body.body)

    def test_unknown_members_are_refused_not_ignored(self) -> None:
        with self.assertRaisesRegex(FrameError, "unknown frame member"):
            decode('{"v":1,"id":"1","t":"end","deadline":"5s"}')

    def test_non_ascii_is_literal_not_escaped(self) -> None:
        encoded = Frame.data("1", {"text": "café"}).encode().decode("utf-8")
        self.assertIn("café", encoded)
        self.assertNotIn("\\u00e9", encoded)

    def test_a_corrupt_length_prefix_cannot_force_a_huge_allocation(self) -> None:
        with self.assertRaisesRegex(FrameError, "over the"):
            decode_stream((0xFFFFFFFF).to_bytes(4, "big") + b"{}")

    def test_a_partial_tail_is_left_for_the_next_read(self) -> None:
        whole = Frame.call("1", "healthz", "GET", "/healthz").encode_tcp()
        partial = Frame.end("1").encode_tcp()[:3]
        frames, rest = decode_stream(whole + partial)
        self.assertEqual(len(frames), 1)
        self.assertEqual(len(rest), 3)

    def test_addressing_fields_are_rejected_on_non_call_frames(self) -> None:
        with self.assertRaisesRegex(FrameError, "carries no addressing fields"):
            decode('{"v":1,"id":"1","t":"end","key":"healthz","method":"GET","path":"/healthz"}')

    def test_an_oversized_frame_is_refused(self) -> None:
        with self.assertRaisesRegex(FrameError, "over the"):
            Frame.data("1", {"blob": "x" * (MAX_FRAME_BYTES + 1)}).encode()

    def test_correlation_ids_are_monotonic_not_content_derived(self) -> None:
        c = Correlator("c7-")
        first, second = c.take(), c.take()
        self.assertNotEqual(first, second)
        # Two identical calls must not collide -- the trap the opto-sync
        # minted record id fell into.
        self.assertEqual([first, second], ["c7-1", "c7-2"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
