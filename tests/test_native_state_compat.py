"""Retained state exports preserve native lock lifetime, bytes and callback ownership."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from emojikit import packstate
from tests.reference import packstate as source

ROOT = Path(__file__).resolve().parents[1]
RUNS_ON_NATIVE_WINDOWS = True


class NativeStateCompatibility(unittest.TestCase):
    def test_exported_state_lease_callback_and_atomic_bytes(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder)
            path = root / "family.lock"
            descriptors = []
            with packstate.exclusive_lock(path, stale_after=0, on_acquired=descriptors.append) as heartbeat:
                self.assertTrue(callable(heartbeat))
                os.fstat(descriptors[0])
                record = json.loads(path.read_text(encoding="utf-8").strip())
                self.assertEqual(record["pid"], os.getpid())
                self.assertIn("started", record)
                heartbeat()
                with self.assertRaises(packstate.LockBusy):
                    with packstate.exclusive_lock(path):
                        self.fail("second writer entered")
                with self.assertRaises(source.LockBusy):
                    with source.exclusive_lock(path):
                        self.fail("source writer entered native lock")
            with self.assertRaises(OSError):
                os.fstat(descriptors[0])
            self.assertTrue(path.exists(), "stable lock target must never be deleted")
            with source.exclusive_lock(path):
                with self.assertRaises(packstate.LockBusy):
                    with packstate.exclusive_lock(path):
                        self.fail("native writer entered source lock")
            def broken(_fd):
                raise LookupError("callback failed")
            with self.assertRaises(LookupError):
                with packstate.exclusive_lock(path, on_acquired=broken):
                    self.fail("callback failure must prevent entry")
            with packstate.exclusive_lock(path):
                pass
            native_path, source_path = root / "native.json", root / "source.json"
            value = {"label": "é 😀", "real": float("inf"), "keys": ["a", "b"]}
            packstate.write_json_atomic(native_path, value)
            source.write_json_atomic(source_path, value)
            self.assertEqual(native_path.read_bytes(), source_path.read_bytes())
            before = native_path.read_bytes()
            with self.assertRaises(TypeError):
                packstate.write_json_atomic(native_path, {"bad": object()})
            self.assertEqual(native_path.read_bytes(), before)
            blocked = root / "blocked.json"
            blocked.mkdir()
            (blocked / "keep").write_bytes(b"retained")
            with self.assertRaises(OSError):
                packstate.write_json_atomic(blocked, {"new": True})
            self.assertEqual((blocked / "keep").read_bytes(), b"retained")
            self.assertEqual(list(root.glob("*.tmp")), [])
            valid = {"base": "fixture", "done": [], "sets": [{"name": "fixture1", "index": 1, "count": 1}]}
            for state in (valid, {**valid, "sets": [{"name": "fixture1", "index": True, "count": False}]}):
                source.validate_state_shape(state, base="fixture", per_set=200)
                packstate.validate_state_shape(state, base="fixture", per_set=200)
            for state in ({**valid, "done": None}, {**valid, "sets": [{"name": "fixture1", "index": 0, "count": 1}]}, {**valid, "in_flight": {"key": "a", "operation": "replace", "set_name": "fixture"}}):
                with self.assertRaises(source.StateInvalid):
                    source.validate_state_shape(state, base="fixture", per_set=200)
                with self.assertRaises(packstate.StateInvalid):
                    packstate.validate_state_shape(state, base="fixture", per_set=200)
            intent = packstate.make_intent(key="a", operation="replace", set_name="fixture", set_index=1, expected_before=1)
            self.assertEqual(intent["operation"], "replace", "intent builder must not impose publisher-only operation validation")
            self.assertTrue(intent["started_utc"].endswith(" UTC"))
