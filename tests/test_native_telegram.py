"""Loopback-only Bot API transport evidence, without operator credentials."""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
import threading
import unittest
from urllib.parse import parse_qs

RUNS_ON_NATIVE_WINDOWS = True


class NativeTelegramTransport(unittest.TestCase):
    def test_native_transport_cannot_bypass_suite_network_guard(self):
        from emojikit import _native

        for base in ("https://api.telegram.org", "http://localhost", "http://example.com"):
            with self.subTest(base=base), self.assertRaisesRegex(RuntimeError, "numeric loopback"):
                _native.NativeTelegram("test-only-token", base)

    def test_probe_keeps_exists_missing_unknown_and_sends_form(self):
        from emojikit import _native

        observed = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                size = int(self.headers["Content-Length"])
                data = parse_qs(self.rfile.read(size).decode("utf-8"))
                observed.append((self.path, data))
                name = data.get("name", [""])[0]
                if name == "exists":
                    payload = {"ok": True, "result": {"stickers": [], "name": name}}
                elif name == "missing":
                    payload = {"ok": False, "description": "STICKERSET_INVALID"}
                else:
                    payload = []
                body = json.dumps(payload).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = HTTPServer(("127.0.0.1", 0), Handler)
        server.timeout = 3
        def serve_three():
            for _ in range(3):
                server.handle_request()
        thread = threading.Thread(target=serve_three)
        thread.start()
        try:
            native = _native.NativeTelegram("test-only-token", f"http://127.0.0.1:{server.server_port}")
            for name in ("exists", "missing", "unknown"):
                result = json.loads(native.probe(name))
                self.assertEqual(result[0], name)
                if name != "exists":
                    self.assertIsNone(result[1])
            thread.join(timeout=5)
            self.assertFalse(thread.is_alive())
            self.assertEqual([data["name"][0] for _, data in observed],
                             ["exists", "missing", "unknown"])
            self.assertTrue(all(path.endswith("/getStickerSet") for path, _ in observed))
        finally:
            server.server_close()
            thread.join(timeout=10)
            self.assertFalse(thread.is_alive(), "loopback fixture leaked a thread")
