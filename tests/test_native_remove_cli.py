"""A real native removal commits journal/state only after fresh live identity proof."""
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


class NativePlanRemoval(unittest.TestCase):
    def test_budgeted_removal_retires_id_without_starting_an_add(self):
        self._removal(False)

    def test_unknown_live_state_stops_with_runtime_exit_and_retains_intent(self):
        self._removal(True)

    def test_flood_wait_keeps_pending_exit_and_planning_refusal_keeps_usage_exit(self):
        self._removal("flood")

    def test_cross_pack_move_add_reorder_and_repeat_are_one_native_command(self):
        from email import policy
        from email.parser import BytesParser
        import sys
        from emojikit import identity, media

        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            names = [f"fixture{n}_by_YourEmojiBot" for n in (1, 2, 3)]
            arts = ("numera-emoji-mapper-logo.png", "numera-emoji-mapper-logo-circle.png", "numera-coin-emoji-mapper-logo.png")
            keys, paths = [], []
            old_ids = ["111111111", "222222222", "333333333"]
            with Catalog(data / "catalog.db") as catalog:
                for index, art in enumerate(arts):
                    path = media.to_static_png(ROOT / "assets" / art, folder / f"source-{index}.png")
                    key, phash = identity.fingerprint(path, "static")
                    keys.append(key)
                    paths.append(path)
                    catalog.add(content_key=key, fmt="static", file_path=path, phash=phash,
                                emojis=["😀"], file_unique_id=f"unique-{index}")
                    catalog.mark_uploaded(key, old_ids[index], base="fixture", set_name=names[index])
                self.assertEqual(len(set(keys)), 3, "distinct source art is necessary for attribution")
                catalog.set_order(keys)
            state = {"base": "fixture", "sets": [
                {"name": name, "title": f"Fixture {index + 1}", "fmt": "mixed", "index": index + 1,
                 "keys": [keys[index]], "live": 1, "logo": False} for index, name in enumerate(names)],
                "sent": names.copy(), "sent_full": [], "skipped": []}
            sentinel = json.loads(json.dumps(state["sets"][2]))
            plan = {"version": 1, "targets": [[keys[0], 2], [keys[1], 2], [keys[2], 3]],
                    "known": keys, "excluded": [], "moves": [], "held": [], "per_set": 200}
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            (data / "pack_plan.json").write_text(json.dumps(plan), encoding="utf-8")
            live = {name: [{"custom_emoji_id": old_ids[index], "file_id": f"file-{index}",
                            "file_unique_id": f"unique-{index}"}] for index, name in enumerate(names)}
            observed = []
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def reply(self, value):
                    body = value if isinstance(value, bytes) else json.dumps(value).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                def do_POST(self):
                    body = self.rfile.read(int(self.headers["Content-Length"]))
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    if "multipart/form-data" in self.headers["Content-Type"]:
                        message = BytesParser(policy=policy.default).parsebytes(
                            f"Content-Type: {self.headers['Content-Type']}\r\n\r\n".encode("utf-8") + body)
                        parts = {part.get_param("name", header="content-disposition"): part.get_payload(decode=True)
                                 for part in message.iter_parts()}
                        self.server.test.assertEqual(method, "addStickerToSet")
                        self.server.test.assertEqual(parts["name"].decode("utf-8"), names[1])
                        self.server.test.assertEqual(parts["file0"], paths[0].read_bytes())
                        live[names[1]].append({"custom_emoji_id": "444444444", "file_id": "moved-file",
                                               "file_unique_id": "moved-unique"})
                        result = True
                    else:
                        form = parse_qs(body.decode("utf-8"))
                        if method == "getMe":
                            result = {"username": "YourEmojiBot"}
                        elif method == "getStickerSet":
                            name = form["name"][0]
                            self.server.test.assertIn(name, live)
                            result = {"name": name, "stickers": live[name]}
                        elif method == "deleteStickerFromSet":
                            self.server.test.assertEqual(form["sticker"], ["file-0"])
                            self.server.test.assertIsNotNone(json.loads((data / "plan_apply_fixture.json").read_text(encoding="utf-8"))["intent"])
                            live[names[0]].clear()
                            result = True
                        elif method == "getFile":
                            self.server.test.assertEqual(form["file_id"], ["moved-file"])
                            result = {"file_path": "moved.png"}
                        elif method == "setStickerPositionInSet":
                            self.server.test.assertEqual(form["sticker"], ["moved-file"])
                            self.server.test.assertEqual(form["position"], ["0"])
                            live[names[1]].insert(0, live[names[1]].pop())
                            result = True
                        else:
                            raise AssertionError(f"unexpected mutation or method: {method}")
                    self.reply({"ok": True, "result": result})
                def do_GET(self):
                    self.reply(paths[0].read_bytes())
            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            server.test = self
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token", "BRAND_LOGO_BOTS": "",
                               "PYO3_PYTHON": sys.executable,
                               "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                command = [str(BINARY), "plan-apply", "--base", "fixture", "--data-dir", str(data),
                           "--apply", "--user-id", "111111111", "--max-changes", "2"]
                for run in range(2):
                    before_mutations = [method for method in observed if method in
                                        ("deleteStickerFromSet", "addStickerToSet", "setStickerPositionInSet")]
                    result = subprocess.run(command, cwd=folder, env=environment, stdin=subprocess.DEVNULL,
                        capture_output=True, encoding="utf-8", timeout=30,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    mutations = [method for method in observed if method in
                                 ("deleteStickerFromSet", "addStickerToSet", "setStickerPositionInSet")]
                    self.assertEqual(mutations, ["deleteStickerFromSet", "addStickerToSet", "setStickerPositionInSet"])
                    if run:
                        self.assertEqual(mutations, before_mutations, "restart repeated a completed mutation")
                    saved = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                    self.assertEqual(saved["sets"][0]["keys"], [])
                    self.assertEqual(saved["sets"][1]["keys"], keys[:2])
                    self.assertEqual(saved["sets"][2], sentinel)
                    journal = json.loads((data / "plan_apply_fixture.json").read_text(encoding="utf-8"))
                    self.assertIsNone(journal["intent"])
                    self.assertEqual([row["cid"] for row in journal["retired"]], [old_ids[0]])
                    self.assertEqual([st["custom_emoji_id"] for st in live[names[1]]], ["444444444", old_ids[1]])
                    self.assertEqual(live[names[2]][0]["custom_emoji_id"], old_ids[2])
                    with Catalog(data / "catalog.db") as catalog:
                        self.assertEqual(catalog.custom_emoji_id_for("fixture", keys[0]), "444444444")
                        self.assertEqual(catalog.published_set_names()[keys[0]], names[1])
                        self.assertEqual(catalog.custom_emoji_id_for("fixture", keys[2]), old_ids[2])
                    self.assertEqual(list((data / "tmp/native-codec").iterdir()), [])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "move fixture leaked a thread")

    def _removal(self, unknown):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            with Catalog(data / "catalog.db") as catalog:
                with mock.patch("tests.reference.catalog.catalog_identity",
                                side_effect=lambda cat, key, fmt, path, phash: (key, True)):
                    catalog.add(content_key="a", fmt="static", file_path=folder / "unused.png")
                catalog.mark_uploaded("a", "111111111", base="fixture", set_name="fixture1_by_YourEmojiBot")
            state = {"base": "fixture", "sets": [
                {"name": "fixture1_by_YourEmojiBot", "title": "Fixture 1", "fmt": "mixed", "index": 1,
                 "keys": ["a"], "live": 1, "logo": False},
                {"name": "fixture2_by_YourEmojiBot", "title": "Fixture 2", "fmt": "mixed", "index": 2,
                 "keys": [], "live": 0, "logo": False}], "sent": [], "sent_full": [], "skipped": []}
            plan = {"version": 1, "targets": [["a", 2]], "known": ["a"], "excluded": [],
                    "moves": [], "held": [], "per_set": 200}
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            (data / "pack_plan.json").write_text(json.dumps(plan), encoding="utf-8")
            if unknown:
                (data / "plan_apply_fixture.json").write_text(json.dumps({"version": 1, "base": "fixture",
                    "intent": {"op": "remove", "key": "a", "set": "fixture1_by_YourEmojiBot", "cid": "111111111"},
                    "retired": []}), encoding="utf-8")
            live = [{"custom_emoji_id": "111111111", "file_id": "fixture-file", "file_unique_id": "fixture-unique"}]
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
                    if method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "getStickerSet":
                        result = {"stickers": live}
                    elif method == "deleteStickerFromSet":
                        self.server.test.assertEqual(form["sticker"], ["fixture-file"])
                        live.clear()
                        result = True
                    else:
                        self.send_error(400)
                        return
                    payload = {"ok": True, "result": [] if unknown and method == "getStickerSet" else result}
                    if unknown == "flood":
                        payload = {"ok": False, "error_code": 429, "description": "Too Many Requests: retry after 301",
                                   "parameters": {"retry_after": 301}}
                    body = json.dumps(payload).encode("utf-8")
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
                environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token", "BRAND_LOGO_BOTS": "",
                    "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                result = subprocess.run([str(BINARY), "plan-apply", "--base", "fixture", "--data-dir", str(data),
                    "--apply", "--user-id", "111111111", "--max-changes", "1"],
                    cwd=folder, env=environment, stdin=subprocess.DEVNULL, capture_output=True,
                    encoding="utf-8", timeout=15,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                if unknown == "flood":
                    self.assertEqual(result.returncode, 3, result.stderr)
                    self.assertIn("FLOOD_WAIT", result.stderr)
                    self.assertEqual(observed, ["getMe"])
                    refused = subprocess.run([str(BINARY), "plan-apply", "--max-changes", "0"],
                        cwd=folder, env=environment, stdin=subprocess.DEVNULL, capture_output=True,
                        encoding="utf-8", timeout=5)
                    self.assertEqual(refused.returncode, 2)
                    self.assertEqual(observed, ["getMe"])
                    return
                if unknown:
                    self.assertEqual(result.returncode, 4, result.stderr)
                    self.assertNotIn("deleteStickerFromSet", observed)
                    journal = json.loads((data / "plan_apply_fixture.json").read_text(encoding="utf-8"))
                    self.assertEqual(journal["intent"]["cid"], "111111111")
                    self.assertEqual(json.loads((data / "publish_fixture.json").read_text(encoding="utf-8")), state)
                    return
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertEqual(observed.count("deleteStickerFromSet"), 1)
                state = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                self.assertEqual(state["sets"][0]["keys"], [])
                self.assertEqual(state["sets"][0]["live"], 0)
                journal = json.loads((data / "plan_apply_fixture.json").read_text(encoding="utf-8"))
                self.assertIsNone(journal["intent"])
                self.assertEqual(journal["retired"][0]["cid"], "111111111")
                with Catalog(data / "catalog.db") as catalog:
                    self.assertFalse(catalog.is_published("fixture", "a"))
                self.assertNotIn("createNewStickerSet", observed)
                self.assertNotIn("addStickerToSet", observed)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "removal fixture leaked a thread")
