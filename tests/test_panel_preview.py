"""Real preview responses keep dimensions, motion and transparent video posters."""
from __future__ import annotations

import gzip
import io
import json
import subprocess
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer
from pathlib import Path
from urllib import error, request

from PIL import Image

from emojikit import panel
from emojikit import media
from emojikit import panel_preview


LOTTIE = {"v": "5.7.4", "w": 512, "h": 512, "fr": 30, "ip": 0, "op": 30,
       "layers": [{"ty": 1, "ind": 1, "sw": 80, "sh": 80, "sc": "#ff0000",
                   "ip": 0, "op": 30, "st": 0, "ks": {
                       "o": {"a": 0, "k": 100}, "r": {"a": 0, "k": 0},
                       "a": {"a": 0, "k": [0, 0, 0]},
                       "s": {"a": 0, "k": [100, 100, 100]},
                       "p": {"a": 1, "k": [
                           {"t": 0, "s": [80, 200, 0], "e": [400, 200, 0],
                            "i": {"x": 1, "y": 1}, "o": {"x": 0, "y": 0}},
                           {"t": 30, "s": [400, 200, 0]}]}}}]}


def _write_tgs(path: Path) -> None:
    path.write_bytes(gzip.compress(json.dumps(LOTTIE).encode("utf-8"), mtime=0))


class PreviewResponses(unittest.TestCase):
    def test_sized_motion_and_video_posters_are_real_cached_images(self):
        temp_root = Path(__file__).resolve().parent.parent / "logs" / "test-temp"
        temp_root.mkdir(parents=True, exist_ok=True)
        temp = tempfile.TemporaryDirectory(dir=temp_root)
        self.addCleanup(temp.cleanup)
        data = Path(temp.name)
        png, tgs, video = data / "source.png", data / "source.tgs", data / "source.webm"
        with Image.new("RGBA", (100, 100), (240, 20, 40, 128)) as img:
            img.save(png, "PNG")
        _write_tgs(tgs)
        subprocess.run([media.ffmpeg_path(), "-v", "error", "-y", "-loop", "1", "-i", str(png),
                        "-t", "0.1", "-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p",
                        "-threads", "1", "-auto-alt-ref", "0", str(video)],
                       check=True, timeout=20, stdin=subprocess.DEVNULL, capture_output=True)
        server = ThreadingHTTPServer(("127.0.0.1", 0), panel.make_handler(
            [], {"animated": tgs, "video": video, "static": png}, data / "catalog.db", "test-token"))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def stop():
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
            self.assertFalse(thread.is_alive())

        self.addCleanup(stop)
        base = f"http://127.0.0.1:{server.server_address[1]}/preview/"

        def get(path):
            with request.urlopen(base + path, timeout=10) as response:
                return response.read()

        moving = get("animated?fps=10&size=72")
        with Image.open(io.BytesIO(moving)) as img:
            self.assertEqual(img.size, (72, 72))
            self.assertEqual(img.n_frames, 10)
        cache_times = {p.name: p.stat().st_mtime_ns for p in (data / "preview").iterdir()}
        self.assertEqual(get("animated?fps=10&size=72"), moving)
        self.assertEqual({p.name: p.stat().st_mtime_ns for p in (data / "preview").iterdir()}, cache_times)
        for kind in ("static", "video", "animated"):
            with self.subTest(format=kind), Image.open(io.BytesIO(get(kind + "?still=1&size=72"))) as img:
                self.assertEqual(img.size, (72, 72))
                self.assertEqual(getattr(img, "n_frames", 1), 1)
                if kind == "video":
                    self.assertLess(img.convert("RGBA").getextrema()[3][1], 200,
                                    "the video decoder silently dropped alpha")
        with self.assertRaises(error.HTTPError) as refused:
            get("animated?fps=1000&size=10000")
        self.assertEqual(refused.exception.code, 400)

    def test_a_video_preview_moves_and_keeps_its_alpha(self):
        """Video cards are animated images by default, so the preview must move."""
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        data = Path(temp.name)
        for i in range(10):
            with Image.new("RGBA", (100, 100), (25 * i, 200 - 15 * i, 90, 128)) as img:
                img.save(data / f"f{i:02d}.png", "PNG")
        video = data / "clip.webm"
        subprocess.run([media.ffmpeg_path(), "-v", "error", "-y", "-framerate", "10",
                        "-i", str(data / "f%02d.png"), "-c:v", "libvpx-vp9",
                        "-pix_fmt", "yuva420p", "-threads", "1", "-auto-alt-ref", "0", str(video)],
                       check=True, timeout=20, stdin=subprocess.DEVNULL, capture_output=True)
        db = data / "catalog.db"
        first = panel_preview.preview_bytes("v:clip", video, db, 10, still=False, size=72)
        with Image.open(io.BytesIO(first)) as img:
            self.assertEqual(img.size, (72, 72))
            self.assertGreater(img.n_frames, 1, "a video preview must animate")
            self.assertLess(img.convert("RGBA").getextrema()[3][1], 200,
                            "the video decoder silently dropped alpha")
        self.assertEqual(panel_preview.preview_bytes("v:clip", video, db, 10, still=False, size=72), first)
        self.assertTrue(any("@v10-72" in p.name for p in (data / "preview").iterdir()))


class TheWarmUpRendersBeforeTheScroll(unittest.TestCase):
    """Every preview miss used to be paid at scroll time, behind a bound of 2.

    Measured on the owner's catalog: one animation costs ~155 ms and one video
    poster ~613 ms, so a cold tier arrived two files at a time -- which is what
    "the animations load in pieces" was. The warm-up renders what the page is
    about to ask for, in grid order, so the scroll meets a warm cache.
    """

    def _data(self) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        return Path(temp.name)

    def test_it_renders_grid_order_and_a_bad_file_cannot_stop_it(self):
        data = self._data()
        tgs, broken = data / "good.tgs", data / "broken.tgs"
        _write_tgs(tgs)
        broken.write_bytes(b"not a lottie at all")
        view = [{"key": "l:logo", "fmt": "static", "isLogo": True, "included": True},
                {"key": "a:bad", "fmt": "animated", "included": True},
                {"key": "a:good", "fmt": "animated", "included": True}]
        by_key = {"l:logo": tgs, "a:bad": broken, "a:good": tgs}

        counts = panel_preview.warm(view, by_key, data / "catalog.db", 15)

        # The unreadable file is skipped, the one after it is still rendered:
        # a warm-up that dies on the first bad row warms nothing. Still and
        # animation, at the full and the compact tier.
        self.assertEqual(counts, {"rendered": 4, "cached": 0, "failed": 4})
        # The logo is a preview-only card with no catalog media.
        good = [n for n in (data / "preview").iterdir()]
        self.assertEqual(len(good), 4, good)

    def test_statics_are_skipped_except_the_holding_trays_still(self):
        data = self._data()
        png, tgs = data / "s.png", data / "a.tgs"
        with Image.new("RGBA", (100, 100), (10, 200, 90, 255)) as img:
            img.save(png, "PNG")
        _write_tgs(tgs)
        view = [{"key": "s:shown", "fmt": "static", "included": True},
                {"key": "a:moving", "fmt": "animated", "included": True},
                {"key": "s:held", "fmt": "static", "included": False}]
        by_key = {"s:shown": png, "a:moving": tgs, "s:held": png}
        db = data / "catalog.db"

        first = panel_preview.warm(view, by_key, db, 30)

        self.assertEqual(first, {"rendered": 5, "cached": 0, "failed": 0})
        self.assertIsNone(panel_preview.cached_path("s:shown", png, db, 15, True, 104),
                          "the grid shows a static from /img/, never its preview")
        self.assertIsNotNone(panel_preview.cached_path("s:held", png, db, 10, True, 72))
        second = panel_preview.warm(view, by_key, db, 30)
        self.assertEqual(second, {"rendered": 0, "cached": 5, "failed": 0})

    def test_the_tiers_follow_the_frame_rate_cap(self):
        self.assertEqual(panel_preview.page_tiers(30),
                         {"full": {"size": 104, "fps": 15}, "compact": {"size": 72, "fps": 10}})
        self.assertEqual(panel_preview.page_tiers(12)["full"]["fps"], 12)
        self.assertEqual(panel_preview.page_tiers(12)["compact"]["fps"], 10)

    def test_a_key_with_a_path_separator_is_refused(self):
        with self.assertRaises(ValueError):
            panel_preview.cached_path("a:../x", Path("x.tgs"), self._data() / "c.db", 15)

    def test_a_set_stop_event_ends_it_without_finishing_the_catalog(self):
        data = self._data()
        tgs = data / "a.tgs"
        _write_tgs(tgs)
        stop = threading.Event()
        stop.set()

        # Ctrl+C must not wait for a thousand renders to finish.
        counts = panel_preview.warm(
            [{"key": f"a:k{i}", "fmt": "animated"} for i in range(50)],
            {f"a:k{i}": tgs for i in range(50)}, data / "catalog.db", 15, stop=stop)

        self.assertEqual(counts["rendered"], 0)
        self.assertFalse((data / "preview").exists())

if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel_preview -v")
