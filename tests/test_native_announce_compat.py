"""Native announcement rendering and Worker routing behind retained coin exports."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import socket
import threading
import unittest
from unittest import mock
from urllib.parse import parse_qs

from emojikit import announce
from emojikit.telegram_api import Telegram

RUNS_ON_NATIVE_WINDOWS = True


class NativeAnnouncementContract(unittest.TestCase):
    def test_direct_cards_list_worker_refusal_and_nondefault_timeout(self):
        observed = []
        worker_mode = "success"
        class Handler(BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *args):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                if self.path == "/publish":
                    observed.append(("worker", json.loads(body), self.headers.get("Authorization")))
                    if worker_mode == "disconnect":
                        self.connection.shutdown(socket.SHUT_RDWR)
                        self.close_connection = True
                        return
                    status = 503 if worker_mode == "refuse" else 200
                    response = b'{"ok":true}'
                else:
                    observed.append(("direct", parse_qs(body.decode("utf-8")), None))
                    status = 200
                    response = b'{"ok":true,"result":{"message_id":1}}'
                self.send_response(status)
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        server.daemon_threads = True
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            base = f"http://127.0.0.1:{server.server_port}"
            packs = [{"name": "fixture1_by_YourEmojiBot", "title": "Fixture 1", "count": 1},
                     {"name": "fixture2_by_YourEmojiBot", "title": "", "count": 2}]
            with mock.patch.dict("os.environ", {"TELEGRAM_API_BASE": base,
                    "WORKER_PUBLISH_URL": "", "WORKER_PUBLISH_SECRET": "", "PACK_LINKS_CHAT_ID": ""}):
                tg = Telegram("test-only-token")
                self.assertEqual(announce.announce_packs(tg, 111111111, packs, bot="coin", note="Note"), "111111111")
                direct = [entry[1] for entry in observed]
                self.assertEqual([entry["text"][0] for entry in direct],
                    ["Note", "✅ Fixture 1\nhttps://t.me/addemoji/fixture1_by_YourEmojiBot",
                     "✅ fixture2_by_YourEmojiBot\nhttps://t.me/addemoji/fixture2_by_YourEmojiBot"])
                self.assertTrue(all(entry["chat_id"] == ["111111111"] and entry["disable_web_page_preview"] == ["true"] for entry in direct))
                observed.clear()
                with mock.patch.dict("os.environ", {"PACK_LINKS_CHAT_ID": "@fixturechannel"}):
                    self.assertEqual(announce.announce_packs(tg, 111111111, packs, bot="coin", note="Note", style="list"), "@fixturechannel")
                self.assertEqual(len(observed), 1)
                self.assertEqual(observed[0][1]["text"], ["Note\n\nFixture 1. https://t.me/addemoji/fixture1_by_YourEmojiBot\nfixture2_by_YourEmojiBot. https://t.me/addemoji/fixture2_by_YourEmojiBot"])
                observed.clear()
                with mock.patch.dict("os.environ", {"WORKER_PUBLISH_URL": base + "/publish", "WORKER_PUBLISH_SECRET": "fixture-secret"}):
                    self.assertEqual(announce.announce_packs(tg, 111111111, packs, bot="coin", note="Note", style="list"), "the worker")
                    self.assertEqual(observed, [("worker", {"bot": "coin", "packs": packs, "style": "list", "note": "Note"}, "Bearer fixture-secret")])
                    observed.clear()
                    self.assertIsNone(announce.announce_via_worker(packs, timeout=2))
                    self.assertEqual(len(observed), 1)
                    self.assertNotIn("note", observed[0][1])
                    worker_mode = "refuse"
                    observed.clear()
                    with self.assertRaisesRegex(RuntimeError, "HTTP 503"):
                        announce.announce_packs(tg, 111111111, packs, bot="coin")
                    self.assertEqual(len(observed), 1, "Worker refusal must not retry or fall back direct")
                    worker_mode = "disconnect"
                    observed.clear()
                    with self.assertRaisesRegex(RuntimeError, "outcome unknown"):
                        announce.announce_packs(tg, 111111111, packs, bot="coin")
                    self.assertEqual(len(observed), 1, "unknown Worker outcome must not retry")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            self.assertFalse(thread.is_alive(), "announcement fixture leaked")
