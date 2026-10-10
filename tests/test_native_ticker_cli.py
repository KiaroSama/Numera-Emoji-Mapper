"""Native prepared-PNG builder owns upload intents and repeats without another mutation."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeTickerCLI(unittest.TestCase):
    def test_dry_run_text_matches_source_without_network_or_state(self):
        import contextlib
        import io
        from unittest import mock
        from tests.reference import build_pack

        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            source = folder / "source"
            source.mkdir()
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source / "fixture.png")
            keywords = folder / "keywords.csv"
            keywords.write_text('ticker,keywords\nfixture,"fixture, public art"\n', encoding="utf-8")
            state = folder / "resume.json"
            args = ["--base", "fixture", "--title", "Fixture", "--source-dir", str(source),
                    "--token-env", "GENERAL_BOT_TOKEN", "--user-id", "111111111",
                    "--keywords", str(keywords), "--state", str(state), "--dry-run"]
            environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token"}
            with mock.patch.dict(os.environ, environment), mock.patch.object(sys, "argv", ["build_pack", *args]), mock.patch.object(build_pack, "setup_logging"), contextlib.redirect_stdout(io.StringIO()) as capture:
                expected_code = build_pack.main()
            result = subprocess.run([str(BINARY), "build-pack", *args], cwd=folder, env=environment,
                stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=5,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(result.returncode, expected_code, result.stderr)
            self.assertEqual(result.stdout, capture.getvalue())
            self.assertFalse(state.exists())

    def test_unbranded_create_resume_and_repeat(self):
        self._build(False)

    def test_configured_logo_first_and_restart_without_duplicate(self):
        self._build(True)

    def _build(self, branded):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            root = Path(directory).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            shutil.copytree(ROOT / "emojikit", root / "emojikit", ignore=shutil.ignore_patterns("__pycache__"))
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            source = root / "source"
            source.mkdir()
            from emojikit import media
            media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png", source / "fixture.png")
            payload = (source / "fixture.png").read_bytes()
            logo_source = ROOT / "assets/numera-emoji-mapper-logo-circle.png"
            media.to_static_png(logo_source, root / "logo.png")
            logo_payload = (root / "logo.png").read_bytes()
            observed = []
            live = []
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    body = self.rfile.read(int(self.headers["Content-Length"]))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    if method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "getStickerSet":
                        if not live:
                            self.reply({"ok": False, "description": "STICKERSET_INVALID"})
                            return
                        result = {"stickers": live}
                    elif method == "createNewStickerSet":
                        self.server.test.assertIn(logo_payload if branded else payload, body)
                        self.server.test.assertIn(b"custom_emoji", body)
                        intent = json.loads((root / "state_fixture.json").read_text(encoding="utf-8"))["in_flight"]
                        self.server.test.assertEqual(intent["operation"], "create")
                        self.server.test.assertEqual(intent["key"], "__brand_logo__" if branded else "fixture")
                        live.append({"file_id": "new-file", "file_unique_id": "new-unique", "custom_emoji_id": "111111111"})
                        result = True
                    elif method == "addStickerToSet":
                        self.server.test.assertTrue(branded)
                        self.server.test.assertIn(payload, body)
                        intent = json.loads((root / "state_fixture.json").read_text(encoding="utf-8"))["in_flight"]
                        self.server.test.assertEqual(intent["operation"], "add")
                        self.server.test.assertEqual(intent["expected_before"], 1)
                        live.append({"file_id": "item-file", "file_unique_id": "item-unique", "custom_emoji_id": "222222222"})
                        result = True
                    elif method == "getFile":
                        result = {"file_path": "logo.png" if branded and b"new-file" in body else "fixture.png"}
                    elif method == "sendMessage":
                        result = {"message_id": 1}
                    else:
                        self.send_error(400)
                        return
                    self.reply({"ok": True, "result": result})
                def do_GET(self):
                    self.reply(logo_payload if self.path.endswith("logo.png") else payload)
                def reply(self, value):
                    body = value if isinstance(value, bytes) else json.dumps(value).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            server.test = self
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token", "BRAND_LOGO_BOTS": "YourEmojiBot" if branded else "",
                               "BRAND_LOGO_PATH": str(logo_source),
                               "PYO3_PYTHON": sys.executable, "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                command = [str(executable), "build-pack", "--base", "fixture", "--title", "Fixture", "--source-dir", str(source),
                           "--token-env", "GENERAL_BOT_TOKEN", "--user-id", "111111111"]
                for _ in range(2):
                    result = subprocess.run(command, cwd=directory, env=environment, stdin=subprocess.DEVNULL,
                        capture_output=True, encoding="utf-8", timeout=30,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn("\nDONE. 1 total emojis across 1 set(s).", result.stdout)
                    self.assertTrue(result.stdout.endswith(f"  https://t.me/addemoji/fixture1_by_YourEmojiBot  ({2 if branded else 1})\n"))
                self.assertEqual(observed.count("createNewStickerSet"), 1)
                self.assertEqual(observed.count("addStickerToSet"), 1 if branded else 0)
                self.assertEqual(observed.count("sendMessage"), 1)
                state = json.loads((root / "state_fixture.json").read_text(encoding="utf-8"))
                self.assertEqual(state["done"], ["fixture"])
                self.assertIsNone(state["in_flight"])
                self.assertEqual(state["sets"][0]["count"], 2 if branded else 1)
                self.assertEqual(state["sent"], ["fixture1_by_YourEmojiBot"])
                self.assertFalse((root / "collection").exists())
                self.assertFalse(list((root / "logs").glob("ticker-codec-*")))
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "ticker fixture leaked a thread")
