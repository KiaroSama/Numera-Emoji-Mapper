"""Actual native local-ingest command, with a public real-image source oracle."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from PIL import Image
from emojikit import identity, media
from tests.reference.catalog import Catalog

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "native/target/debug" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")


def assert_same_png_pixels(test, actual: Path, expected: Path):
    """Compare a generated PNG to its independent historical Pillow reference."""
    signature = b"\x89PNG\r\n\x1a\n"
    test.assertTrue(expected.read_bytes().startswith(signature), "source oracle must remain PNG")
    test.assertTrue(actual.read_bytes().startswith(signature), "generated candidate must remain PNG")
    with Image.open(expected) as reference, Image.open(actual) as observed:
        test.assertEqual(reference.format, "PNG")
        test.assertEqual(reference.mode, "RGBA")
        test.assertEqual(reference.size, (100, 100))
        test.assertEqual((observed.format, observed.mode, observed.size),
                         (reference.format, reference.mode, reference.size))
        with reference.convert("RGBA") as wanted, observed.convert("RGBA") as got:
            test.assertEqual(got.tobytes(), wanted.tobytes())


class NativeLocalCommand(unittest.TestCase):
    def test_undecodable_catalog_rival_refuses_merge_and_retains_incoming_bytes(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            source = ROOT / "assets/numera-emoji-mapper-logo.png"
            expected = media.to_static_png(source, folder / "expected.png")
            key, phash = identity.fingerprint(expected, "static")
            rival = folder / "rival.png"
            rival.write_bytes(b"undecodable retained catalog media")
            with Catalog(data / "catalog.db") as catalog:
                catalog.add(content_key=key, fmt="static", file_path=rival, phash=phash,
                            emojis=["⭐"], keywords=["original"], file_unique_id="prior-unique")
                prior = [(item.content_key, item.file_path, item.keywords) for item in catalog.all_items()]
            command = [str(BINARY), "add-media", str(source), "--as", "static", "--data-dir", str(data)]
            result = subprocess.run(command, cwd=folder, env={**os.environ, "PYO3_PYTHON": sys.executable},
                stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=30,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(result.returncode, 4, result.stderr)
            self.assertIn("new=0 dedup=0 failed=1", result.stdout)
            with Catalog(data / "catalog.db") as catalog:
                self.assertEqual([(item.content_key, item.file_path, item.keywords) for item in catalog.all_items()], prior)
                self.assertEqual(catalog.seen_file_unique_id("prior-unique"), key)
            self.assertEqual(rival.read_bytes(), b"undecodable retained catalog media")
            retained = list((data / "media/static").glob("*.png"))
            self.assertEqual(len(retained), 1)
            assert_same_png_pixels(self, retained[0], expected)
            self.assertEqual(list((data / "tmp").iterdir()), [])

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
