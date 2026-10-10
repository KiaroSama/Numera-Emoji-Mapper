"""Actual native local-ingest command, with a public real-image source oracle."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from emojikit import identity, media
from tests.reference.catalog import Catalog

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "native/target/debug" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")


class NativeLocalCommand(unittest.TestCase):
    def test_real_image_ingest_and_repeat_match_source_identity(self):
        self.assertTrue(BINARY.is_file(), "build the native executable before CLI parity")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            source = ROOT / "assets/numera-emoji-mapper-logo.png"
            expected_file = media.to_static_png(source, folder / "expected.png")
            expected_key, expected_hash = identity.fingerprint(expected_file, "static")
            command = [str(BINARY), "add-media", str(source), "--as", "static",
                       "--data-dir", str(folder / "collection"), "--emoji", "😀",
                       "--phash-threshold", "-1"]
            environment = {**os.environ, "PYO3_PYTHON": sys.executable}
            for n in range(2):
                result = subprocess.run(command, cwd=folder, env=environment, encoding="utf-8",
                    stdin=subprocess.DEVNULL, capture_output=True, timeout=30,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout,
                    f"Done. new={int(n == 0)} dedup={int(n == 1)} failed=0\n"
                    "  catalog static: 1 total (1 pending upload)\n")
            with Catalog(folder / "collection/catalog.db") as catalog:
                items = catalog.all_items()
                self.assertEqual(len(items), 1)
                self.assertEqual((items[0].content_key, items[0].phash), (expected_key, expected_hash))
                self.assertIs(identity.same_image(Path(items[0].file_path), expected_file, "static"), True)
            self.assertEqual(list((folder / "collection/tmp").iterdir()), [])
