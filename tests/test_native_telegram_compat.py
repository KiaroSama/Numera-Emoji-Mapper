"""Native transport and upload proofs behind the retained coin Python contract."""
from email import policy
from email.parser import BytesParser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import socket
import tempfile
import threading
import unittest
from unittest import mock
from urllib.parse import parse_qs

from emojikit import media, telegram_api as api
from emojikit.errors import FloodWaitTooLong

ROOT = Path(__file__).resolve().parents[1]
RUNS_ON_NATIVE_WINDOWS = True


class NativeCoinTelegramContract(unittest.TestCase):
    def test_foreign_unknown_and_wrong_create_shape_do_not_attribute_by_count(self):
        for mode in ("foreign", "unknown", "wrong-create"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
                folder = Path(folder)
                source = media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png", folder / "source.png")
                other = media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo-circle.png", folder / "foreign.png")
                payloads = {"ours": source.read_bytes(), "foreign": other.read_bytes()}
                live = [{"file_id": "foreign", "file_unique_id": "old", "custom_emoji_id": "111111111"}]
                mutations = []
                class Handler(BaseHTTPRequestHandler):
                    def setup(self):
                        super().setup()
                        self.connection.settimeout(3)
                    def log_message(self, *args):
                        pass
                    def reply(self, value):
                        body = value if isinstance(value, bytes) else json.dumps(value).encode("utf-8")
                        self.send_response(200)
                        self.send_header("Content-Length", str(len(body)))
                        self.end_headers()
                        self.wfile.write(body)
                    def do_POST(self, mode=mode, mutations=mutations, live=live):
                        body = self.rfile.read(int(self.headers["Content-Length"]))
                        method = self.path.rsplit("/", 1)[-1]
                        if method == "getStickerSet":
                            self.reply([] if mode == "unknown" and mutations else {"ok": True, "result": {"stickers": live}})
                            return
                        if method == "getFile":
                            self.reply({"ok": True, "result": {"file_path": parse_qs(body.decode("utf-8"))["file_id"][0]}})
                            return
                        mutations.append(method)
                        if mode == "foreign":
                            live.append({"file_id": "foreign" if len(mutations) == 1 else "ours",
                                         "file_unique_id": "foreign-new" if len(mutations) == 1 else "ours-new",
                                         "custom_emoji_id": "222222222" if len(mutations) == 1 else "333333333"})
                            if len(mutations) == 2:
                                self.reply({"ok": True, "result": True})
                                return
                        elif mode == "wrong-create":
                            live[:] = [{"file_id": "ours", "file_unique_id": str(n), "custom_emoji_id": str(n)} for n in (1, 2)]
                        self.connection.shutdown(socket.SHUT_RDWR)
                        self.close_connection = True
                    def do_GET(self, payloads=payloads):
                        self.reply(payloads[self.path.rsplit("/", 1)[-1]])
                server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
                server.daemon_threads = True
                thread = threading.Thread(target=server.serve_forever)
                thread.start()
                try:
                    with mock.patch.dict("os.environ", {"TELEGRAM_API_BASE": f"http://127.0.0.1:{server.server_port}"}):
                        tg = api.Telegram("test-only-token")
                        if mode == "foreign":
                            self.assertEqual(tg.add_sticker(1, "fixture", source, "😀", "public", expected_before=1), 3)
                            self.assertEqual(mutations, ["addStickerToSet", "addStickerToSet"])
                        else:
                            with self.assertRaises(api.AmbiguousUploadError):
                                if mode == "wrong-create":
                                    tg.create_set(1, "fixture", "Fixture", source, "😀", "public")
                                else:
                                    tg.add_sticker(1, "fixture", source, "😀", "public", expected_before=1)
                            self.assertEqual(len(mutations), 1, "unprovable count/shape must never authorize another mutation")
                finally:
                    server.shutdown()
                    server.server_close()
                    thread.join(timeout=5)
                    self.assertFalse(thread.is_alive())

    def test_native_mutation_proof_return_classes_callback_and_bytes(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            source = media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png", Path(folder) / "source.png")
            payload = source.read_bytes()
            live = [{"file_id": "old", "file_unique_id": "old-unique", "custom_emoji_id": "111111111"}]
            observed = []
            lost = False

            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(3)

                def log_message(self, *args):
                    pass

                def reply(self, value):
                    body = value if isinstance(value, bytes) else json.dumps(value).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)

                def do_POST(self):
                    nonlocal lost
                    body = self.rfile.read(int(self.headers["Content-Length"]))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    if "multipart/form-data" in self.headers["Content-Type"]:
                        message = BytesParser(policy=policy.default).parsebytes(
                            f"Content-Type: {self.headers['Content-Type']}\r\n\r\n".encode("utf-8") + body)
                        fields = {part.get_param("name", header="content-disposition"): part.get_payload(decode=True)
                                  for part in message.iter_parts()}
                        self.server.test.assertEqual(fields["file0"], payload)
                        self.server.test.assertEqual(fields["name"], b"fixture")
                    else:
                        fields = parse_qs(body.decode("utf-8"))
                    if method == "getStickerSet":
                        name = fields["name"][0]
                        if name == "missing":
                            self.reply({"ok": False, "description": "STICKERSET_INVALID"})
                            return
                        if name == "unknown":
                            self.reply([])
                            return
                        result = {"name": "fixture", "stickers": live}
                    elif method == "getFile":
                        result = {"file_path": "source.png"}
                    elif method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "addStickerToSet":
                        live.append({"file_id": "new", "file_unique_id": "new-unique", "custom_emoji_id": "222222222"})
                        if not lost:
                            lost = True
                            self.connection.shutdown(socket.SHUT_RDWR)
                            self.close_connection = True
                            return
                        result = True
                    elif method == "createNewStickerSet":
                        live[:] = [{"file_id": "created", "file_unique_id": "created-unique", "custom_emoji_id": "333333333"}]
                        result = True
                    elif method == "deleteStickerSet":
                        self.reply({"ok": False, "description": "definite rejection"})
                        return
                    elif method == "sendMessage":
                        self.reply({"ok": False, "description": "retry after 301", "parameters": {"retry_after": 301}})
                        return
                    elif method == "replaceStickerInSet":
                        self.connection.shutdown(socket.SHUT_RDWR)
                        self.close_connection = True
                        return
                    else:
                        result = True
                    self.reply({"ok": True, "result": result})

                def do_GET(self):
                    self.reply(payload)

            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            server.test = self
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                with mock.patch.dict("os.environ", {"TELEGRAM_API_BASE": f"http://127.0.0.1:{server.server_port}"}):
                    tg = api.Telegram("test-only-token")
                    self.assertEqual(tg.get_me()["username"], "YourEmojiBot")
                    self.assertEqual(tg.probe_set_state("missing"), (api.SetState.MISSING, None))
                    self.assertEqual(tg.probe_set_state("unknown"), (api.SetState.UNKNOWN, None))
                    self.assertEqual(tg.live_count_strict("missing"), 0)
                    with self.assertRaises(api.LiveStateUnknown):
                        tg.live_count_strict("unknown")
                    self.assertEqual(tg.set_fuids("fixture"), {"old-unique"})
                    self.assertEqual(tg.download_bytes("old"), payload)
                    self.assertEqual(tg.add_sticker(1, "fixture", source, "😀", "public", expected_before=1), 2)
                    self.assertEqual(observed.count("addStickerToSet"), 1, "lost applied response must not duplicate the sticker")
                    self.assertIsNone(tg.add_sticker(1, "fixture", source, "😀", "public", expected_before=2))
                    self.assertEqual(observed.count("addStickerToSet"), 2)
                    self.assertIsNone(tg.create_set(1, "fixture", "Fixture", source, "😀", "public"))
                    self.assertEqual(observed.count("createNewStickerSet"), 1)
                    with self.assertRaises(api.BotApiError):
                        tg.call("deleteStickerSet", data={"name": "fixture"}, retries=1)
                    tg.max_flood_wait = 300
                    with self.assertRaises(FloodWaitTooLong) as caught:
                        tg.send_message(1, "fixture")
                    self.assertEqual((caught.exception.method, caught.exception.seconds), ("sendMessage", 301))
                    with self.assertRaises(api.AmbiguousUploadError):
                        tg.call("replaceStickerInSet", data={}, retries=1, applied_check=lambda: None)
                    failure = LookupError("caller callback failure")
                    def broken():
                        raise failure
                    with self.assertRaises(LookupError) as caught:
                        tg.call("replaceStickerInSet", data={}, retries=1, applied_check=broken)
                    self.assertIs(caught.exception, failure, "native binding must preserve the original callback exception")
                    self.assertNotIn(tg.token, tg.safe(RuntimeError("url/bot" + tg.token)))
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "native compatibility API fixture leaked")
