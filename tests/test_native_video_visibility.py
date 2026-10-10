"""Blank VP9 is skipped by the real native publisher; fade-in remains usable."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

from emojikit import identity, media_bridge
from tests._video_fixtures import RED, encode
from tests.reference.catalog import Catalog
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeVideoVisibility(unittest.TestCase):
    def test_blank_refuses_publication_but_fade_in_is_visible(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            transparent = bytes(100 * 100 * 4)
            blank = encode(folder, "blank", [transparent] * 30)
            fade = encode(folder, "fade", [transparent] * 15 + [RED] * 15)
            self.assertIs(media_bridge.execute({"operation": "usable", "source": str(fade), "format": "video"}), True)
            data = folder / "collection"
            key, phash = identity.fingerprint(blank, "video")
            with Catalog(data / "catalog.db") as catalog:
                catalog.add(content_key=key, fmt="video", file_path=blank, phash=phash)
            observed = []
            class API(BaseHTTPRequestHandler):
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    self.rfile.read(int(self.headers.get("Content-Length", "0")))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    body = json.dumps({"ok": True, "result": {"username": "YourEmojiBot"}} if method == "getMe"
                                      else {"ok": True, "result": True}).encode()
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            server = ThreadingHTTPServer(("127.0.0.1", 0), API)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture", "--title", "Fixture",
                    "--mixed", "--no-brand-logo", "--user-id", "1", "--data-dir", str(data)], cwd=ROOT,
                    env={**os.environ, "GENERAL_BOT_TOKEN": "test-only-token", "PYO3_PYTHON": sys.executable,
                         "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"},
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=20,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(observed, ["getMe"], "blank media reached a wire mutation")
                self.assertEqual(result.returncode, 0, result.stderr)
                state = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                self.assertIn(key, state["skipped"])
                self.assertEqual(state["sets"], [])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive())
