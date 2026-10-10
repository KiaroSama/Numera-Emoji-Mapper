"""Codec-only preview bytes match the original real static and Lottie renderer."""
from pathlib import Path
import tempfile
import unittest

from emojikit import media_bridge
from tests.reference import panel_preview
from tests.test_native_local_cli import ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativePreviewCodec(unittest.TestCase):
    def test_static_and_lottie_preview_bytes_match_source(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder)
            from tests._video_fixtures import BLUE, RED, encode
            video = encode(folder, "preview-alpha", [RED, BLUE, RED])
            for source in (ROOT / "assets/numera-emoji-mapper-logo.png",
                           ROOT / "tests/fixtures/lottie/red_circle_512.tgs", video):
                fmt = {".tgs": "animated", ".webm": "video"}.get(source.suffix, "static")
                for size, fps in ((104, 15), (72, 10)):
                    for still in (True, False):
                        with self.subTest(format=fmt, size=size, still=still):
                            expected = panel_preview.preview_bytes("fixture:" + fmt, source,
                                                                   folder / "catalog.db", fps, still, size)
                            output = folder / f"{fmt}-{size}-{still}.webp"
                            result = media_bridge.execute({"operation": "preview", "source": str(source),
                                "format": fmt, "key": "fixture:" + fmt, "fps": fps, "size": size,
                                "still": still, "output": str(output)})
                            self.assertEqual(result, {"bytes": len(expected)})
                            self.assertEqual(output.read_bytes(), expected)
            self.assertFalse((folder / "catalog.db").exists())
