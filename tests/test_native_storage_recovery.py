"""Native/Python interoperability at the durable ownership boundary."""
import json
from pathlib import Path
import tempfile
import unittest

from emojikit.packstate import LockBusy, exclusive_lock

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent


class NativeStorage(unittest.TestCase):
    def test_legacy_and_native_exclude_each_other_and_release(self):
        from emojikit import _native

        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            path = Path(directory) / "family.lock"
            with exclusive_lock(path):
                with self.assertRaises(RuntimeError):
                    _native.NativeLease(str(path), "2026-10-07 00:00:00 UTC")
            native = _native.NativeLease(str(path), "2026-10-07 00:00:00 UTC")
            try:
                # The mandatory Windows lock must not cover the holder record.
                self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["started"],
                                 "2026-10-07 00:00:00 UTC")
                with self.assertRaises(LockBusy):
                    with exclusive_lock(path):
                        self.fail("legacy writer entered native critical section")
            finally:
                native.close()
            with exclusive_lock(path):
                self.assertTrue(path.is_file(), "lock inode must never be deleted")

    def test_atomic_utf8_replaces_and_leaves_no_sibling(self):
        from emojikit import _native

        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            path = Path(directory) / "state.json"
            _native.write_state_json(str(path), json.dumps({"key": "😀", "uploaded": True}))
            _native.write_state_json(str(path), json.dumps({"key": "é", "uploaded": False}))
            self.assertEqual(json.loads(path.read_text(encoding="utf-8")),
                             {"key": "é", "uploaded": False})
            self.assertEqual(list(Path(directory).iterdir()), [path])
