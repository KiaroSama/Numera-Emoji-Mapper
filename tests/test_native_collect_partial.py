"""Requested-ID CLI preserves good mixed media when another file or ID fails."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from emojikit import identity, media
from tests.reference.catalog import Catalog
from tests._video_fixtures import RED, BLUE, encode

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "native/target/debug" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")
RUNS_ON_NATIVE_WINDOWS = True


class MixedCollectionFailures(unittest.TestCase):
    def test_good_formats_survive_corrupt_download_and_unresolved_id(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve()
            sources = {
                "static": media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png", root / "static.png"),
                "animated": ROOT / "tests/fixtures/lottie/red_circle_512.tgs",
                "video": encode(root, "motion", [RED, BLUE, RED]),
            }
            payloads = {fmt: path.read_bytes() for fmt, path in sources.items()}
            expected = {}
            for fmt, source in sources.items():
                copy = root / ("expected" + source.suffix)
                copy.write_bytes(payloads[fmt])
                media.reencode_in_place(copy, fmt)
                expected[fmt] = identity.fingerprint(copy, fmt)[0]
            payloads["bad"] = b"not a decodable image"
            ids = [str(111111111111111 + n) for n in range(5)]
            stickers = [{"custom_emoji_id": ids[n], "file_id": fmt, "file_unique_id": "fixture-" + fmt,
                         "emoji": "😀", "is_video": fmt == "video", "is_animated": fmt == "animated"}
                        for n, fmt in enumerate(["static", "animated", "video", "bad"])]
            observed = []

            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)

                def log_message(self, *args):
                    pass

                def reply(self, body):
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)

                def do_POST(self):
                    body = self.rfile.read(int(self.headers["Content-Length"]))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    if method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "getCustomEmojiStickers":
                        result = stickers
                    elif method == "getFile":
                        from urllib.parse import parse_qs
                        fmt = parse_qs(body.decode())["file_id"][0]
                        result = {"file_path": fmt}
                    else:
                        self.send_error(400)
                        return
                    self.reply(json.dumps({"ok": True, "result": result}).encode())

                def do_GET(self):
                    fmt = self.path.rsplit("/", 1)[-1]
                    observed.append("download:" + fmt)
                    self.reply(payloads[fmt])

            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                env = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token", "PYO3_PYTHON": sys.executable,
                       "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                command = [str(BINARY), "fetch-emoji-ids", "--data-dir", str(root / "collection"),
                           "--repaintable", "keep"]
                for cid in ids:
                    command += ["--id", cid]
                result = subprocess.run(command, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                                        capture_output=True, encoding="utf-8", timeout=60,
                                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertIn("Done. unique_ids=5 new=3 dedup=0 failed=1 missing=1", result.stdout)
                with Catalog(root / "collection/catalog.db") as catalog:
                    items = catalog.all_items()
                    self.assertEqual({item.fmt: item.content_key for item in items}, expected)
                    for item in items:
                        self.assertEqual(catalog.seen_file_unique_id("fixture-" + item.fmt), item.content_key)
                        self.assertTrue(Path(item.file_path).is_file())
                    self.assertIsNone(catalog.seen_file_unique_id("fixture-bad"))
                self.assertEqual(list((root / "collection/tmp").glob("native-*.*")), [])
                self.assertEqual(list((root / "collection/tmp/native-codec").iterdir()), [])
                self.assertEqual(set(value for value in observed if value.startswith("download:")),
                                 {"download:" + fmt for fmt in payloads})
                self.assertFalse(any(value in observed for value in ("createNewStickerSet", "addStickerToSet")))
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "loopback server leaked")
