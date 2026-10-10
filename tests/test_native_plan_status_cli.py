"""Actual native plan report matches retained source text and never edits durable data."""
from contextlib import closing
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import unittest

from tests.reference import plan_status
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativePlanStatusCLI(unittest.TestCase):
    def test_retired_ids_and_capacity_report_match_source_without_writes(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            root = Path(directory) / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            data = root / "collection"
            data.mkdir()
            state = {"sets": [{"index": 1, "name": "fixture1_by_YourEmojiBot", "keys": ["s:a", "s:held"]},
                              {"index": 2, "name": "fixture2_by_YourEmojiBot", "keys": ["s:b"]}]}
            plan = {"version": 1, "per_set": 200, "targets": [["s:a", 2], ["s:b", 2], ["s:new", 1]],
                    "counts": {"1": 200, "2": 2}, "logo_slots": {"1": 1, "2": 0},
                    "moves": [{"key": "s:a", "from_pack": 1, "to_pack": 2}],
                    "held": [{"key": "s:held", "from_pack": 1}], "excluded": ["s:held"]}
            ids = {"s:a": "111111111", "s:held": "222222222"}
            with closing(sqlite3.connect(data / "catalog.db")) as db:
                db.execute("CREATE TABLE publications(base TEXT,content_key TEXT,custom_emoji_id TEXT)")
                db.executemany("INSERT INTO publications VALUES('fixture',?,?)", ids.items())
                db.commit()
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            (data / "pack_plan.json").write_text(json.dumps(plan), encoding="utf-8")
            before = {p.name: p.read_bytes() for p in data.iterdir()}
            result = subprocess.run([str(executable), "plan-status", "--base", "fixture"], cwd=directory,
                stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(result.returncode, 3, result.stderr)
            expected = plan_status.render(plan_status.summarize(plan, state), ids, None)
            self.assertEqual(result.stdout, expected + "\n")
            self.assertEqual({p.name: p.read_bytes() for p in data.iterdir()}, before)
            calls = []
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    self.rfile.read(int(self.headers["Content-Length"]))
                    method = self.path.rsplit("/", 1)[-1]
                    calls.append(method)
                    if method != "getStickerSet":
                        self.send_error(400)
                        return
                    body = json.dumps({"ok": True, "result": {"stickers": [{"custom_emoji_id": ids["s:a"]}]}}).encode("utf-8")
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
                result = subprocess.run([str(executable), "plan-status", "--base", "fixture", "--live"],
                    cwd=directory, env=environment, stdin=subprocess.DEVNULL, capture_output=True,
                    encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 3, result.stderr)
                expected = plan_status.render(plan_status.summarize(plan, state), ids, {ids["s:a"]})
                self.assertEqual(result.stdout, expected + "\n")
                self.assertEqual(calls, ["getStickerSet", "getStickerSet"])
                self.assertEqual({p.name: p.read_bytes() for p in data.iterdir()}, before)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive())
