"""Native reorder moves existing file IDs and keeps a pinned logo and custom IDs intact."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs
from unittest import mock

from tests.reference.catalog import Catalog
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeReorderCommand(unittest.TestCase):
    def test_move_keeps_custom_ids_and_logo_first(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            with Catalog(data / "catalog.db") as catalog:
                with mock.patch("tests.reference.catalog.catalog_identity",
                                side_effect=lambda cat, key, fmt, path, phash: (key, True)):
                    for key in ("a", "b"):
                        catalog.add(content_key=key, fmt="static", file_path=folder / f"{key}.png")
                catalog.mark_uploaded("a", "111111111", base="fixture", set_name="fixture1_by_YourEmojiBot")
                catalog.mark_uploaded("b", "222222222", base="fixture", set_name="fixture1_by_YourEmojiBot")
            state = {"base": "fixture", "sets": [{"name": "fixture1_by_YourEmojiBot", "title": "Fixture 1",
                "fmt": "mixed", "index": 1, "keys": ["b", "a"], "live": 3, "logo": True}],
                "sent": [], "sent_full": [], "skipped": []}
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            live = [{"file_id": "logo", "custom_emoji_id": "333333333"},
                    {"file_id": "file-b", "custom_emoji_id": "222222222"},
                    {"file_id": "file-a", "custom_emoji_id": "111111111"}]
            observed = []
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    size = int(self.headers["Content-Length"])
                    form = parse_qs(self.rfile.read(size).decode("utf-8"))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append((method, form))
                    if method == "getStickerSet":
                        result = {"stickers": live}
                    elif method == "setStickerPositionInSet":
                        file = form["sticker"][0]
                        index = next(i for i, sticker in enumerate(live) if sticker["file_id"] == file)
                        sticker = live.pop(index)
                        live.insert(int(form["position"][0]), sticker)
                        result = True
                    else:
                        self.send_error(400)
                        return
                    body = json.dumps({"ok": True, "result": result}).encode("utf-8")
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
                    "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                command = [str(BINARY), "sync-order", "--base", "fixture", "--data-dir", str(data), "--apply"]
                for _ in range(2):
                    result = subprocess.run(command, cwd=folder, env=environment,
                        stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout, "", "source apply reports via logging, not JSON stdout")
                self.assertEqual([s["file_id"] for s in live], ["logo", "file-a", "file-b"])
                self.assertEqual([s["custom_emoji_id"] for s in live], ["333333333", "111111111", "222222222"])
                self.assertEqual(sum(method == "setStickerPositionInSet" for method, _ in observed), 1)
                state = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                self.assertEqual(state["sets"][0]["keys"], ["a", "b"])
                self.assertTrue(state["sets"][0]["logo"])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "reorder fixture leaked a thread")
