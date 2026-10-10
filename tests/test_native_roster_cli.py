"""Actual native roster refresh preserves live identity, family history and fail-closed output."""
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
from urllib.parse import parse_qs
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeRosterCLI(unittest.TestCase):
    def test_live_refresh_family_preservation_and_read_failure(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            fixture = Path(directory).resolve() / "fixture-root"
            (fixture / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", fixture / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", fixture / "pyproject.toml")
            executable = fixture / BINARY.name
            shutil.copy2(BINARY, executable)
            (fixture / "collection").mkdir()
            (fixture / "coins").mkdir()
            general = "fixture_by_YourEmojiBot"
            coin = "coins_by_YourEmojiBot"
            (fixture / "collection/publish_fixture.json").write_text(json.dumps({"sets": [
                {"name": general, "title": "Fixture <&>", "index": 1, "logo": False}]}), encoding="utf-8")
            coin_state = fixture / "coins/rebuild_dedup_state.json"
            coin_state.write_text(json.dumps({"sets": [{"name": coin, "title": "Coins", "index": 1}]}), encoding="utf-8")
            (fixture / "coins/ticker_to_id.json").write_text(json.dumps({"bbb": "222222222", "aaa": "222222222"}), encoding="utf-8")
            database = fixture / "collection/catalog.db"
            with closing(sqlite3.connect(database)) as db:
                db.executescript("CREATE TABLE items(content_key TEXT, keywords TEXT, file_path TEXT);"
                                 "CREATE TABLE publications(content_key TEXT, custom_emoji_id TEXT);")
                db.execute("INSERT INTO items VALUES(?,?,?)", ("s:fixture", '["fixture","premium-id:444444444","premium-id:111111111"]', "missing.png"))
                db.execute("INSERT INTO publications VALUES(?,?)", ("s:fixture", "111111111"))
                db.commit()
            live = {general: [{"custom_emoji_id": "111111111", "emoji": "😀"}],
                    coin: [{"custom_emoji_id": "222222222", "emoji": "⭐"}]}
            failed = set()
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
                    observed.append(method)
                    if method != "getStickerSet":
                        self.send_error(400)
                        return
                    name = form["name"][0]
                    value = {"ok": False, "description": "STICKERSET_INVALID"} if name in failed else {
                        "ok": True, "result": {"title": "ignored-live-title", "stickers": live[name]}}
                    body = json.dumps(value).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-general-token", "TELEGRAM_BOT_TOKEN": "test-coin-token",
                           "COLLECTION_PACK_BASE": "fixture", "COIN_EMOJI_DIR": "", "BRAND_LOGO_PATH": "",
                           "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
            def run(*args):
                return subprocess.run([str(executable), "pack-manifest", *args], cwd=directory, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=15,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            try:
                result = run("--refresh")
                self.assertEqual(result.returncode, 0, result.stderr)
                out = fixture / "packs"
                index = json.loads((out / "index.json").read_text(encoding="utf-8"))
                self.assertEqual(index["pack_count"], 2)
                self.assertEqual(index["emoji_count"], 2)
                self.assertEqual(index["by_current_id"]["111111111"]["slot"], 1)
                coin_doc = json.loads((out / f"{coin}.json").read_text(encoding="utf-8"))
                self.assertEqual(coin_doc["emoji"][0]["name"], "aaa, bbb")
                general_doc = json.loads((out / f"{general}.json").read_text(encoding="utf-8"))
                self.assertEqual(general_doc["emoji"][0]["role"], "emoji")
                self.assertEqual(general_doc["title"], "Fixture <&>")
                html = (out / f"{general}.html").read_text(encoding="utf-8")
                self.assertIn('id="bg"', html)
                self.assertIn('id="animLabel"', html)
                self.assertIn('id="roster"', html)
                self.assertEqual(observed, ["getStickerSet", "getStickerSet"])
                self.assertEqual(run("--check").returncode, 0)
                coin_bytes = (out / f"{coin}.json").read_bytes()
                recorded = index["inputs"][coin_state.name]
                coin_state.write_text(coin_state.read_text(encoding="utf-8") + " ", encoding="utf-8")
                live[general] = [{"custom_emoji_id": "333333333", "emoji": "😀"}]
                with closing(sqlite3.connect(database)) as db:
                    db.execute("UPDATE publications SET custom_emoji_id=?", ("333333333",))
                    db.commit()
                result = run("--refresh", "--family", "general")
                self.assertEqual(result.returncode, 0, result.stderr)
                index = json.loads((out / "index.json").read_text(encoding="utf-8"))
                self.assertEqual((out / f"{coin}.json").read_bytes(), coin_bytes)
                self.assertEqual(index["inputs"][coin_state.name], recorded)
                self.assertEqual(index["by_current_id"]["222222222"]["set"], coin)
                self.assertEqual(index["by_previous_id"]["111111111"], "333333333")
                self.assertEqual(run("--check").returncode, 3)
                before = {p.name: p.read_bytes() for p in out.iterdir() if p.is_file()}
                failed.add(coin)
                result = run("--refresh")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual({p.name: p.read_bytes() for p in out.iterdir() if p.is_file()}, before)
                self.assertNotIn("addStickerToSet", observed)
                self.assertNotIn("createNewStickerSet", observed)
                failed.clear()
                environment["COLLECTION_PACK_BASE"] = ""
                (out / "index.json").write_text("{broken", encoding="utf-8")
                (fixture / "coins/ticker_to_id.json").write_text('{"aaa":222222222,"bbb":"222222222"}', encoding="utf-8")
                result = run("--refresh", "--family", "coins")
                self.assertEqual(result.returncode, 0, result.stderr)
                coin_doc = json.loads((out / f"{coin}.json").read_text(encoding="utf-8"))
                self.assertEqual(coin_doc["emoji"][0]["name"], "aaa, bbb")
                environment["COLLECTION_PACK_BASE"] = "fixture"
                (fixture / "coins/ticker_to_id.json").write_text("{broken", encoding="utf-8")
                result = run("--refresh", "--family", "general")
                self.assertEqual(result.returncode, 0, result.stderr)
                result = run("--refresh")
                self.assertEqual(result.returncode, 0, result.stderr)
                index = json.loads((out / "index.json").read_text(encoding="utf-8"))
                index["inputs"]["publish_old.json"] = [0, 0]
                (out / "index.json").write_text(json.dumps(index), encoding="utf-8")
                self.assertEqual(run("--check").returncode, 3)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "roster fixture leaked a thread")
