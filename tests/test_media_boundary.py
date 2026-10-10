"""Exact codec leaves remain usable without importing the retired application commands."""
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from emojikit import media
from tests.test_native_local_cli import ROOT

RUNS_ON_NATIVE_WINDOWS = True


class MediaBoundary(unittest.TestCase):
    def test_codec_timeout_and_svg_do_not_import_application_cli(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            path = Path(folder) / "art.svg"
            path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#ff0000"/></svg>', encoding="utf-8")
            with mock.patch.dict(sys.modules, {"emojikit.build_pack": None, "emojikit.make_emoji_pngs": None}):
                self.assertGreater(media.ff_timeout(), 0)
                image = media._load_image(path)
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.getpixel((32, 32)), (255, 0, 0, 255))
                from coins import _paprika_api, _provider_publish
                self.assertEqual(_paprika_api._fit_100(image).size, (100, 100))
                keywords = Path(folder) / "keywords.csv"
                keywords.write_text('ticker,keywords\nABC,"abc, Alpha"\n', encoding="utf-8")
                self.assertEqual(_provider_publish.load_keywords(keywords), {"abc": "abc, Alpha"})

    def test_gif_animation_metadata_is_codec_only(self):
        from PIL import Image
        from emojikit.media_bridge import execute

        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder)
            first = Image.new("RGBA", (4, 4), "red")
            second = Image.new("RGBA", (4, 4), "blue")
            try:
                first.save(folder / "still.gif")
                first.save(folder / "moving.gif", save_all=True, append_images=[second], duration=100)
            finally:
                first.close()
                second.close()
            (folder / "bad.gif").write_bytes(b"not a GIF")
            with mock.patch.dict(sys.modules, {"emojikit.add_media": None}):
                for name, expected in (("still.gif", False), ("moving.gif", True), ("bad.gif", False)):
                    with self.subTest(name=name):
                        self.assertIs(execute({"operation": "animation_info", "source": str(folder / name),
                                               "format": "static"}), expected)
