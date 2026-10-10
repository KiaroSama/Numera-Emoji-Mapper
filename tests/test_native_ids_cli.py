"""Native tint collection cannot shortcut through a known source file identity."""
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
from emojikit.repaint import repaint_in_place
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeTintCommand(unittest.TestCase):
    def test_known_file_unique_id_does_not_skip_requested_repaint(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            source = media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png", folder / "source.png")
            payload = source.read_bytes()
            expected = folder / "expected.png"
            expected.write_bytes(payload)
            media.reencode_in_place(expected, "static")
            self.assertTrue(repaint_in_place(expected, "static", (255, 0, 0)))
            expected_key, _ = identity.fingerprint(expected, "static")
            source_key, source_hash = identity.fingerprint(source, "static")
            with Catalog(data / "catalog.db") as catalog:
                catalog.add(content_key=source_key, fmt="static", file_path=source,
                            emojis=["😀"], phash=source_hash, file_unique_id="known-source")
            observed = []
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    self.rfile.read(int(self.headers["Content-Length"]))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    if method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "getCustomEmojiStickers":
                        result = [{"custom_emoji_id": "111111111111111", "file_id": "source-file",
                                   "file_unique_id": "known-source", "emoji": "😀", "needs_repainting": True}]
                    elif method == "getFile":
                        result = {"file_path": "source.png"}
                    else:
                        self.send_error(400)
                        return
                    self.reply(json.dumps({"ok": True, "result": result}).encode("utf-8"))
                def do_GET(self):
                    observed.append("download")
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
                result = subprocess.run([str(BINARY), "fetch-emoji-ids", "--id", "111111111111111",
                    "--tint", "#FF0000", "--data-dir", str(data)], cwd=folder, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=30,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Done. unique_ids=1 new=1 dedup=0 failed=0 missing=0 repainted=1\n", result.stdout)
                self.assertIn("  catalog static: 2 total (2 pending upload)\n", result.stdout)
                self.assertIn("download", observed)
                with Catalog(data / "catalog.db") as catalog:
                    self.assertEqual(len(catalog.all_items()), 2)
                    tinted = catalog.get(expected_key)
                    self.assertIsNotNone(tinted)
                    self.assertIn("tint:#FF0000", tinted.keywords)
                    self.assertIs(identity.same_image(Path(tinted.file_path), expected, "static"), True)
                    self.assertEqual(catalog.seen_file_unique_id("known-source"), source_key)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "tint fixture leaked a thread")
