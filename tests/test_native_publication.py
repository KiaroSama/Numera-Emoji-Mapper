"""Actual native publish flow against loopback, with real source art and no live Telegram."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from email import policy
from email.parser import BytesParser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs

from emojikit import identity, media
from tests.reference.catalog import Catalog
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativePublicationFlow(unittest.TestCase):
    def test_create_confirm_record_manifest_and_repeat_without_upload(self):
        self._publication(False)

    def test_configured_logo_leads_pack_and_full_milestone_announces_once(self):
        self._publication(True)

    def test_applied_create_with_lost_response_is_verified_not_retried(self):
        self._publication(False, lost_response=True)

    def test_restart_rejects_foreign_unrecorded_create_without_attributing_it(self):
        self._publication(False, foreign_create=True)

    def test_explicit_into_pack_appends_without_recreating_the_family(self):
        self._publication(False, target_mode="into")

    def test_explicit_new_set_preserves_the_previous_pack(self):
        self._publication(False, target_mode="new")

    def _publication(self, branded, lost_response=False, foreign_create=False, target_mode=None):
        self.assertTrue(BINARY.is_file(), "build native executable before publication parity")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            image = media.to_static_png(ROOT / "assets/numera-emoji-mapper-logo.png", folder / "source.png")
            payload = image.read_bytes()
            logo_source = ROOT / "assets/numera-emoji-mapper-logo-circle.png"
            logo_image = media.to_static_png(logo_source, folder / "logo.png")
            logo_payload = logo_image.read_bytes()
            key, phash = identity.fingerprint(image, "static")
            with Catalog(data / "catalog.db") as catalog:
                catalog.add(content_key=key, fmt="static", file_path=image,
                            emojis=["😀"], keywords=["public | artwork"], source="fixture", phash=phash)
            observed = []
            live = []
            if foreign_create:
                other_key, other_hash = identity.fingerprint(logo_image, "static")
                with Catalog(data / "catalog.db") as catalog:
                    catalog.add(content_key=other_key, fmt="static", file_path=logo_image,
                                emojis=["⭐"], keywords=["foreign"], source="fixture", phash=other_hash)
                live.append({"file_id": "foreign-file", "file_unique_id": "foreign-unique",
                             "custom_emoji_id": "222222222"})
                (data / "publish_fixture.json").write_text(json.dumps({"base": "fixture", "sets": [],
                    "sent": [], "sent_full": [], "skipped": [], "in_flight": {"key": key,
                    "operation": "create", "set_name": "fixture1_by_YourEmojiBot", "set_index": 1,
                    "title": "Fixture 1"}}), encoding="utf-8")
            if target_mode:
                old_key, old_hash = identity.fingerprint(logo_image, "static")
                old_name = "fixture1_by_YourEmojiBot"
                with Catalog(data / "catalog.db") as catalog:
                    catalog.add(content_key=old_key, fmt="static", file_path=logo_image,
                                emojis=["⭐"], phash=old_hash, file_unique_id="old-unique")
                    catalog.mark_uploaded(old_key, "222222222", base="fixture", set_name=old_name)
                live.append({"file_id": "old-file", "file_unique_id": "old-unique",
                             "custom_emoji_id": "222222222"})
                (data / "publish_fixture.json").write_text(json.dumps({"base": "fixture",
                    "sets": [{"name": old_name, "fmt": "mixed", "index": 1, "keys": [old_key],
                              "live": 1, "logo": False}], "sent": [old_name], "sent_full": [],
                    "skipped": [], "in_flight": None}), encoding="utf-8")
                if target_mode == "into":
                    later_image = media.to_static_png(ROOT / "assets/numera-coin-emoji-mapper-logo.png", folder / "later.png")
                    later_key, later_hash = identity.fingerprint(later_image, "static")
                    self.assertNotIn(later_key, (old_key, key))
                    later_name = "fixture2_by_YourEmojiBot"
                    with Catalog(data / "catalog.db") as catalog:
                        catalog.add(content_key=later_key, fmt="static", file_path=later_image,
                                    emojis=["🪙"], phash=later_hash, file_unique_id="later-unique")
                        catalog.mark_uploaded(later_key, "333333333", base="fixture", set_name=later_name)
                    later_live = [{"file_id": "later-file", "file_unique_id": "later-unique",
                                   "custom_emoji_id": "333333333"}]
                    later_set = {"name": later_name, "title": "Fixture 2", "fmt": "mixed", "index": 2, "keys": [later_key],
                                 "live": 1, "logo": False}
                    state_path = data / "publish_fixture.json"
                    seeded = json.loads(state_path.read_text(encoding="utf-8"))
                    seeded["sets"].append(later_set)
                    seeded["sent"].append(later_name)
                    state_path.write_text(json.dumps(seeded), encoding="utf-8")
            class Handler(BaseHTTPRequestHandler):
                def setup(self):
                    super().setup()
                    self.connection.settimeout(5)
                def log_message(self, *args):
                    pass
                def do_POST(self):
                    size = int(self.headers["Content-Length"])
                    body = self.rfile.read(size)
                    method = self.path.rsplit("/", 1)[-1]
                    observed.append(method)
                    if method == "getMe":
                        result = {"username": "YourEmojiBot"}
                    elif method == "getStickerSet":
                        if not live:
                            self.reply({"ok": False, "description": "STICKERSET_INVALID"})
                            return
                        form = parse_qs(body.decode("utf-8"))
                        name = form["name"][0]
                        if target_mode == "into":
                            self.server.test.assertIn(name, (old_name, later_name))
                            selected = live if name == old_name else later_live
                        elif target_mode == "new" and name == "fixture2_by_YourEmojiBot":
                            selected = live[1:]
                            if not selected:
                                self.reply({"ok": False, "description": "STICKERSET_INVALID"})
                                return
                        else:
                            selected = live[:1] if target_mode == "new" else live
                        result = {"name": name, "stickers": selected}
                    elif method == "createNewStickerSet":
                        self.server.test.assertEqual(self.multipart_name(body),
                            "fixture2_by_YourEmojiBot" if target_mode == "new" else "fixture1_by_YourEmojiBot")
                        self.server.test.assertIn(b"custom_emoji", body)
                        self.server.test.assertIn(logo_payload if branded else payload, body)
                        live.append({"file_id": "new-file", "file_unique_id": "new-unique",
                                     "custom_emoji_id": "222222222" if branded else "111111111"})
                        if lost_response:
                            import socket
                            self.connection.shutdown(socket.SHUT_RDWR)
                            self.close_connection = True
                            return
                        result = True
                    elif method == "addStickerToSet":
                        self.server.test.assertEqual(self.multipart_name(body), "fixture1_by_YourEmojiBot")
                        self.server.test.assertTrue(branded or target_mode == "into")
                        self.server.test.assertIn(payload, body)
                        live.append({"file_id": "item-file", "file_unique_id": "item-unique",
                                     "custom_emoji_id": "111111111"})
                        result = True
                    elif method == "getFile":
                        form = parse_qs(body.decode("utf-8"))
                        result = {"file_path": "logo.png" if branded and form["file_id"] == ["new-file"] else "fixture.png"}
                    elif method == "sendMessage":
                        form = parse_qs(body.decode("utf-8"))
                        self.server.test.assertIn("Fixture 2" if target_mode == "new" else "Fixture 1", form["text"][0])
                        result = {"message_id": 1}
                    else:
                        self.send_error(400)
                        return
                    self.reply({"ok": True, "result": result})
                def multipart_name(self, body):
                    header = f"Content-Type: {self.headers['Content-Type']}\r\n\r\n".encode("utf-8")
                    message = BytesParser(policy=policy.default).parsebytes(header + body)
                    names = [part.get_payload(decode=True).decode("utf-8") for part in message.iter_parts()
                             if part.get_param("name", header="content-disposition") == "name"]
                    self.server.test.assertEqual(len(names), 1, "mutation must carry exactly one target name")
                    return names[0]
                def do_GET(self):
                    observed.append("download")
                    self.reply(logo_payload if foreign_create or self.path.endswith("logo.png") else payload)
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
                environment = {**os.environ, "GENERAL_BOT_TOKEN": "test-only-token",
                    "PYO3_PYTHON": sys.executable,
                    "NUMERA_TEST_API_BASE": f"http://127.0.0.1:{server.server_port}"}
                command = [str(BINARY), "build-collection", "--base", "fixture", "--title", "Fixture",
                           "--mixed", "--no-brand-logo", "--user-id", "111111111", "--data-dir", str(data)]
                if target_mode:
                    command += ["--into-pack", "1"] if target_mode == "into" else ["--new-set"]
                if branded:
                    command.remove("--no-brand-logo")
                    command += ["--per-set", "2"]
                    environment.update({"BRAND_LOGO_BOTS": "YourEmojiBot",
                                        "BRAND_LOGO_PATH": str(logo_source)})
                if foreign_create:
                    result = subprocess.run(command, cwd=folder, env=environment, encoding="utf-8",
                        stdin=subprocess.DEVNULL, capture_output=True, timeout=30,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("not proven", result.stderr)
                    state = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                    self.assertEqual(state["sets"], [])
                    self.assertEqual(state["in_flight"]["key"], key)
                    with Catalog(data / "catalog.db") as catalog:
                        self.assertFalse(catalog.is_published("fixture", other_key))
                        self.assertFalse(catalog.is_published("fixture", key))
                    self.assertNotIn("createNewStickerSet", observed)
                    self.assertNotIn("sendMessage", observed)
                    return
                if target_mode:
                    result = subprocess.run(command, cwd=folder, env=environment, encoding="utf-8",
                        stdin=subprocess.DEVNULL, capture_output=True, timeout=30,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    state = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                    self.assertIsNone(state["in_flight"])
                    self.assertEqual(state["sets"][0]["keys"], [old_key, key] if target_mode == "into" else [old_key])
                    self.assertEqual(state["sets"][0]["live"], 2 if target_mode == "into" else 1)
                    if target_mode == "new":
                        self.assertEqual(state["sets"][1]["keys"], [key])
                        self.assertEqual(state["sets"][1]["name"], "fixture2_by_YourEmojiBot")
                    else:
                        self.assertEqual(len(state["sets"]), 2)
                        self.assertEqual(state["sets"][1], later_set, "explicit earlier target changed the later pack")
                        self.assertEqual(later_live, [{"file_id": "later-file", "file_unique_id": "later-unique",
                                                      "custom_emoji_id": "333333333"}])
                    self.assertEqual(observed.count("createNewStickerSet"), int(target_mode == "new"))
                    self.assertEqual(observed.count("addStickerToSet"), int(target_mode == "into"))
                    self.assertEqual(live[0]["custom_emoji_id"], "222222222", "old prefix identity changed")
                    with Catalog(data / "catalog.db") as catalog:
                        self.assertEqual(catalog.custom_emoji_id_for("fixture", old_key), "222222222")
                        self.assertEqual(catalog.custom_emoji_id_for("fixture", key), "111111111")
                        if target_mode == "into":
                            self.assertEqual(catalog.custom_emoji_id_for("fixture", later_key), "333333333")
                            self.assertEqual(catalog.published_set_names()[later_key], later_name)
                    return
                for _ in range(2):
                    result = subprocess.run(command, cwd=folder, env=environment, encoding="utf-8",
                        stdin=subprocess.DEVNULL, capture_output=True, timeout=30,
                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn("\nDONE.\n", result.stdout)
                    self.assertTrue(result.stdout.endswith("  https://t.me/addemoji/fixture1_by_YourEmojiBot  [mixed]\n"), result.stdout)
                self.assertEqual(observed.count("createNewStickerSet"), 1)
                self.assertEqual(observed.count("addStickerToSet"), int(branded))
                self.assertEqual(observed.count("sendMessage"), 2 if branded else 1)
                state = json.loads((data / "publish_fixture.json").read_text(encoding="utf-8"))
                self.assertEqual(state["sets"][0]["keys"], [key])
                self.assertEqual(state["sets"][0]["live"], 2 if branded else 1)
                self.assertEqual(state["sets"][0]["logo"], branded)
                self.assertEqual(state["sent_full"], ["fixture1_by_YourEmojiBot"] if branded else [])
                self.assertIsNone(state["in_flight"])
                self.assertEqual(state["sent"], ["fixture1_by_YourEmojiBot"])
                self.assertIn("public \\| artwork", (data / "manifests/fixture1_by_YourEmojiBot.md").read_text(encoding="utf-8"))
                with Catalog(data / "catalog.db") as catalog:
                    self.assertEqual(catalog.custom_emoji_id_for("fixture", key), "111111111")
                self.assertEqual(list((data / "tmp/native-codec").iterdir()), [])
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive(), "publication fixture leaked a thread")
