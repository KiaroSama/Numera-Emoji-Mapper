"""Native poll/dispatch/cursor behavior runs in a detached public-only fixture root."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs

from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeBotPolling(unittest.TestCase):
    def test_rejected_rich_fallback_cursor_and_conflict_exit(self):
        self._polling(False)

    def test_ambiguous_rich_send_never_sends_a_plain_duplicate(self):
        self._polling(True)

    def _polling(self, ambiguous):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            directory = Path(directory).resolve()
            fixture = directory / "fixture-root"
            (fixture / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", fixture / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", fixture / "pyproject.toml")
            executable = fixture / BINARY.name
            shutil.copy2(BINARY, executable)
            observed = []
            replies = []
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
                    if method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "setMyCommands":
                        result = True
                    elif method == "getUpdates":
                        if form["offset"] == ["0"] and (not ambiguous or sum(method == "getUpdates" for method, _ in observed) == 1):
                            result = [{"update_id": 7, "message": {"message_id": 1,
                                "chat": {"id": 111111111, "type": "private"},
                                "from": {"id": 111111111}, "entities": [
                                    {"type": "custom_emoji", "custom_emoji_id": "222222222"}]}}]
                        else:
                            self.reply({"ok": False, "description": "Conflict: another poller"})
                            return
                    elif method == "getCustomEmojiStickers":
                        result = [{"custom_emoji_id": "222222222", "emoji": "⭐"}]
                    elif method == "sendMessage":
                        replies.append(form["text"][0])
                        if len(replies) == 1:
                            if ambiguous:
                                import socket
                                self.connection.shutdown(socket.SHUT_RDWR)
                                self.close_connection = True
                                return
                            self.reply({"ok": False, "description": "invalid custom emoji"})
                            return
                        result = {"message_id": 2}
                    else:
                        self.send_error(400)
                        return
                    self.reply({"ok": True, "result": result})
                def reply(self, value):
                    body = json.dumps(value).encode("utf-8")
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
                    "PACK_OWNER_USER_ID": "111111111", "BOT_ALLOWED_USER_IDS": "111111111",
                    "BOT_ALLOWED_CHANNEL_IDS": "",
                    "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                result = subprocess.run([str(executable), "emoji-bot"], cwd=directory, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=20,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 4, result.stderr)
                if ambiguous:
                    self.assertEqual(json.loads((fixture / "state_emoji_bot.json").read_text(encoding="utf-8")),
                                     {"offset": 8}, "reported unresolved outcome did not persist its cursor")
                    self.assertIn("reply outcome unresolved; no fallback", result.stderr)
                    self.assertEqual(len(replies), 1, "ambiguous rich send produced a duplicate")
                else:
                    self.assertEqual(json.loads((fixture / "state_emoji_bot.json").read_text(encoding="utf-8")),
                                     {"offset": 8})
                    self.assertEqual(len(replies), 2)
                    self.assertNotIn("<tg-emoji", replies[1])
                self.assertIn("<tg-emoji", replies[0])
                polls = [form for method, form in observed if method == "getUpdates"]
                self.assertEqual([form["offset"] for form in polls], [["0"], ["8"]])
                self.assertEqual(json.loads(polls[0]["allowed_updates"][0]),
                                 ["message", "channel_post", "my_chat_member"])
                if ambiguous:
                    observed.clear()
                    restarted = subprocess.run([str(executable), "emoji-bot"], cwd=directory, env=environment,
                        stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(restarted.returncode, 4, restarted.stderr)
                    self.assertEqual(len(replies), 1, "restart resent an acknowledged unresolved reply")
                    self.assertEqual([form["offset"] for method, form in observed if method == "getUpdates"], [["8"]])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "poll fixture leaked a thread")
