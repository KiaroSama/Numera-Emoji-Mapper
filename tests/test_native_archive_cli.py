"""Actual archive CLI preserves per-file commits when a later real source is missing."""
from contextlib import closing
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeArchiveCLI(unittest.TestCase):
    def test_missing_second_source_keeps_first_move_recorded(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            root = Path(directory).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            data = root / "collection"
            data.mkdir()
            source = data / "first.png"
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
            original = source.read_bytes()
            key = "s:" + "a" * 32
            missing_key = "s:" + "b" * 32
            from tests.reference.catalog import Catalog
            with Catalog(data / "catalog.db") as catalog:
                catalog.add(content_key=key, fmt="static", file_path=source, emojis=["😀"], keywords=["first"], source="fixture")
                catalog.add(content_key=missing_key, fmt="static", file_path=data / "missing.png", emojis=["⭐"], keywords=["missing"], source="fixture")
                catalog.mark_uploaded(key, "111111111", base="fixture", set_name="fixture_by_YourEmojiBot")
                catalog.mark_uploaded(missing_key, "222222222", base="fixture", set_name="fixture_by_YourEmojiBot")
            with closing(sqlite3.connect(data / "catalog.db")) as db:
                db.execute("UPDATE publications SET set_name=NULL WHERE content_key=?", (missing_key,))
                db.commit()
            state = {"sets": [{"name": "fixture_by_YourEmojiBot", "title": "Fixture Pack", "fmt": "mixed", "index": 1, "live": 200}]}
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            live = [{"custom_emoji_id": "000000000"}, {"custom_emoji_id": "111111111", "emoji": "😀"},
                    {"custom_emoji_id": "222222222", "emoji": "⭐"}]
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
                    if method != "getStickerSet":
                        self.send_error(400)
                        return
                    body = json.dumps({"ok": True, "result": {"stickers": live}}).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            environment = {**os.environ, "COLLECTION_PACK_BASE": "fixture", "GENERAL_BOT_TOKEN": "test-only-token",
                           "BRAND_LOGO_PATH": str(ROOT / "assets/numera-emoji-mapper-logo.png"),
                           "EMOJI_ARCHIVE_DIR": str(root / "archive"),
                           "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
            def run(*args):
                return subprocess.run([str(executable), "pack-archive", *args], cwd=directory, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=15,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            try:
                result = run("--sync")
                self.assertEqual(result.returncode, 1, result.stderr)
                archived = root / "archive/Fixture Pack" / ("002_static_" + "a" * 12 + ".png")
                self.assertEqual(archived.read_bytes(), original)
                self.assertFalse(source.exists())
                with closing(sqlite3.connect(data / "catalog.db")) as db:
                    path = db.execute("SELECT file_path FROM items WHERE content_key=?", (key,)).fetchone()[0]
                from emojikit.media_paths import resolve
                self.assertEqual(resolve(data, path).resolve(), archived.resolve())
                self.assertFalse((root / "archive/Fixture Pack/_history.json").exists())
                shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", data / "missing.png")
                with closing(sqlite3.connect(data / "catalog.db")) as db:
                    db.execute("UPDATE publications SET set_name=? WHERE content_key=?", ("fixture_by_YourEmojiBot", missing_key))
                    db.commit()
                result = run("--sync")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(archived.read_bytes(), original)
                self.assertFalse((data / "missing.png").exists())
                self.assertEqual((root / "archive/Fixture Pack/001_logo.png").read_bytes(), original)
                history = json.loads((root / "archive/Fixture Pack/_history.json").read_text(encoding="utf-8"))
                self.assertEqual(history["emoji"][1]["content_key"], key)
                self.assertEqual(run("--check").returncode, 0)
                result = run("--sync")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("0 file(s) moved", result.stdout)
                self.assertEqual(observed, ["getStickerSet"] * 3)
                # A stranded final-name partial copy must never replace a retained good source.
                shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
                with closing(sqlite3.connect(data / "catalog.db")) as db:
                    db.execute("UPDATE items SET file_path=? WHERE content_key=?", (str(source), key))
                    db.commit()
                archived.write_bytes(b"interrupted-copy")
                result = run("--sync")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(source.read_bytes(), original)
                with closing(sqlite3.connect(data / "catalog.db")) as db:
                    self.assertEqual(db.execute("SELECT file_path FROM items WHERE content_key=?", (key,)).fetchone()[0], str(source))
                self.assertFalse(list(archived.parent.glob(".archive-*.tmp")))
                for variable, arguments in (("COLLECTION_PACK_BASE", ("--check",)), ("EMOJI_ARCHIVE_DIR", ("--check",)),
                                            ("GENERAL_BOT_TOKEN", ("--sync",))):
                    prior = environment[variable]
                    environment[variable] = ""
                    self.assertEqual(run(*arguments).returncode, 1)
                    environment[variable] = prior
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "archive fixture leaked a thread")
