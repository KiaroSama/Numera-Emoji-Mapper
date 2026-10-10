"""Native pack download uses loopback API and real public-image identity evidence."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs

from emojikit import identity, media
from tests.reference.catalog import Catalog
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativePackCommand(unittest.TestCase):
    def test_download_reencode_and_seen_file_skip_through_native_cli(self):
        self.assertTrue(BINARY.is_file(), "native executable required by CLI parity")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            payload_file = media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png",
                                               folder / "source.png")
            payload = payload_file.read_bytes()
            media.reencode_in_place(payload_file, "static")
            expected_key, expected_hash = identity.fingerprint(payload_file, "static")
            observed = []
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    size = int(self.headers["Content-Length"])
                    data = parse_qs(self.rfile.read(size).decode("utf-8"))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append((method, data))
                    if method == "getStickerSet":
                        self.reply(json.dumps({"ok": True, "result": {"name": "fixture", "stickers": [
                            {"file_id": "fixture-file", "file_unique_id": "fixture-unique",
                             "emoji": "😀", "keywords": ["public-art"]}]}}).encode("utf-8"))
                    elif method == "getFile":
                        self.reply(b'{"ok":true,"result":{"file_path":"fixture.png"}}')
                    else:
                        self.send_error(400)
                def do_GET(self):
                    observed.append(("download", self.path))
                    self.reply(payload)
                def reply(self, body):
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token",
                    "PYO3_PYTHON": sys.executable,
                    "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                command = [str(BINARY), "fetch-pack", "fixture", "--repaintable", "skip",
                           "--data-dir", str(folder / "collection"), "--limit", "1"]
                for n in range(2):
                    result = subprocess.run(command, cwd=folder, env=environment, encoding="utf-8",
                        stdin=subprocess.DEVNULL, capture_output=True, timeout=30,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout,
                        f"Done. new={int(n == 0)} dedup={int(n == 1)} failed=0\n"
                        "  catalog static: 1 total (1 pending upload)\n")
                with Catalog(folder / "collection/catalog.db") as catalog:
                    item, = catalog.all_items()
                    self.assertEqual((item.content_key, item.phash), (expected_key, expected_hash))
                    self.assertEqual(catalog.seen_file_unique_id("fixture-unique"), expected_key)
                    self.assertIs(identity.same_image(Path(item.file_path), payload_file, "static"), True)
                self.assertEqual([method for method, _ in observed],
                                 ["getStickerSet", "getFile", "download", "getStickerSet"])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "loopback API thread leaked")
