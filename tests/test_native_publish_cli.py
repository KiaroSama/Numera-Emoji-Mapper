"""Native collection dry-run exercises real catalog plans, with no network or frozen-plan write."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from tests.reference.catalog import Catalog
from unittest import mock
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativePublishCommand(unittest.TestCase):
    def test_corrupt_resume_refuses_before_catalog_migration(self):
        from contextlib import closing
        import sqlite3
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            data = Path(folder).resolve()
            database = data / "catalog.db"
            with closing(sqlite3.connect(database)) as db:
                db.execute("CREATE TABLE untouched(value TEXT)")
                db.commit()
            state = data / "publish_fixture.json"
            state.write_text("{truncated", encoding="utf-8")
            before = database.read_bytes()
            result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                "--title", "Fixture", "--mixed", "--no-brand-logo", "--dry-run",
                "--data-dir", str(data)], cwd=ROOT, env=os.environ.copy(), stdin=subprocess.DEVNULL,
                capture_output=True, encoding="utf-8", timeout=5,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(database.read_bytes(), before, "corrupt state allowed catalog migration")
            self.assertEqual(result.returncode, 4, result.stderr)
            self.assertEqual(state.read_text(encoding="utf-8"), "{truncated")

    def test_preflight_reports_missing_file_without_publishing_or_saving_plan(self):
        import contextlib
        import io
        import logging
        import threading
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        from tests.backend_source_oracle import source_namespace
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            key = "s:missing"
            with Catalog(data / "catalog.db") as catalog:
                catalog.add(content_key=key, fmt="static", file_path=folder / "missing.png")
            observed = []
            answer = {"ok": True, "result": {"file_id": "fixture-upload"}}
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
                    body = json.dumps({"ok": True, "result": {"username": "YourEmojiBot"}}
                                      if method == "getMe" else answer).encode("utf-8")
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
                result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                    "--title", "Fixture", "--mixed", "--no-brand-logo", "--user-id", "111111111",
                    "--data-dir", str(data), "--preflight"], env=environment, cwd=folder,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                source = source_namespace("collection_preflight.py", ("_report",),
                    {"log": logging.getLogger("preflight-oracle"), "EXIT_FAILED": 4, "EXIT_PARTIAL": 3, "EXIT_OK": 0})
                with contextlib.redirect_stdout(io.StringIO()) as capture:
                    expected_code = source["_report"](0, [], [(key, "missing.png")], [])
                self.assertEqual(result.returncode, expected_code, result.stderr)
                self.assertEqual(result.stdout, capture.getvalue())
                self.assertEqual(observed, ["getMe"])
                self.assertFalse((data / "publish_fixture.json").exists())
                self.assertFalse((data / "publish_plan_fixture.json").exists())
                with Catalog(data / "catalog.db") as catalog:
                    catalog.set_inclusion({key})
                observed.clear()
                result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                    "--title", "Fixture", "--mixed", "--no-brand-logo", "--user-id", "111111111",
                    "--data-dir", str(data), "--preflight"], env=environment, cwd=folder,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                with contextlib.redirect_stdout(io.StringIO()) as capture:
                    expected_code = source["_report"](0, [], [], [])
                self.assertEqual(result.returncode, expected_code, result.stderr)
                self.assertEqual(result.stdout, capture.getvalue())
                self.assertEqual(observed, ["getMe"])
                self.assertFalse((data / "publish_plan_fixture.json").exists())
                from shutil import copy2
                copy2(ROOT / "assets/numera-emoji-mapper-logo.png", folder / "missing.png")
                with Catalog(data / "catalog.db") as catalog:
                    catalog.set_inclusion(set())
                observed.clear()
                result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                    "--title", "Fixture", "--mixed", "--no-brand-logo", "--user-id", "111111111",
                    "--data-dir", str(data), "--preflight"], env=environment, cwd=folder,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                with contextlib.redirect_stdout(io.StringIO()) as capture:
                    expected_code = source["_report"](1, [], [], [])
                self.assertEqual(result.returncode, expected_code, result.stderr)
                self.assertEqual(result.stdout, capture.getvalue())
                self.assertEqual(observed, ["getMe", "uploadStickerFile"])
                self.assertFalse((data / "publish_plan_fixture.json").exists())
                answer.update({"ok": False, "description": "Bad Request: wrong file type"})
                observed.clear()
                result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                    "--title", "Fixture", "--mixed", "--no-brand-logo", "--user-id", "111111111",
                    "--data-dir", str(data), "--preflight"], env=environment, cwd=folder,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                with contextlib.redirect_stdout(io.StringIO()) as capture:
                    expected_code = source["_report"](0, [(key, "missing.png", "uploadStickerFile failed: Bad Request: wrong file type")], [], [])
                self.assertEqual(result.returncode, expected_code, result.stderr)
                self.assertEqual(result.stdout, capture.getvalue())
                self.assertEqual(observed, ["getMe", "uploadStickerFile"])
                self.assertFalse((data / "publish_plan_fixture.json").exists())
                observed.clear()
                answer = {"ok": True}  # Missing result is UNKNOWN, never a rejected file.
                result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                    "--title", "Fixture", "--mixed", "--no-brand-logo", "--user-id", "111111111",
                    "--data-dir", str(data), "--preflight"], env=environment, cwd=folder,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                with contextlib.redirect_stdout(io.StringIO()) as capture:
                    expected_code = source["_report"](0, [], [], [(key, "missing.png", "Bot API success has no result")])
                self.assertEqual(result.returncode, expected_code, result.stderr)
                self.assertEqual(result.stdout, capture.getvalue())
                self.assertEqual(observed, ["getMe", "uploadStickerFile"])
                self.assertFalse((data / "publish_fixture.json").exists())
                self.assertFalse((data / "publish_plan_fixture.json").exists())
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                self.assertFalse(thread.is_alive())

    def test_dry_run_preserves_plan_and_uses_current_inclusion_and_family(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            with Catalog(data / "catalog.db") as catalog:
                with mock.patch("tests.reference.catalog.catalog_identity",
                                side_effect=lambda cat, key, fmt, path, phash: (key, True)):
                    for key in ("a", "b", "c"):
                        catalog.add(content_key=key, fmt="static", file_path=folder / f"{key}.png")
                catalog.set_inclusion({"b"})
                catalog.mark_uploaded("a", "111111111", base="fixture", set_name="fixture1")
            path = data / "publish_plan_fixture.json"
            saved = '{"mixed":["a","b"]}\n'
            path.write_text(saved, encoding="utf-8")
            environment = {**os.environ, "PYO3_PYTHON": sys.executable}
            result = subprocess.run([str(BINARY), "build-collection", "--base", "fixture",
                "--title", "Fixture", "--mixed", "--no-brand-logo", "--dry-run",
                "--data-dir", str(data)], env=environment, cwd=folder,
                stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("mixed: 1 emoji -> 1 set(s) of up to 200", result.stdout)
            self.assertEqual(path.read_text(encoding="utf-8"), saved)
            self.assertFalse((data / "publish_fixture.json").exists())
            with Catalog(data / "catalog.db") as catalog:
                self.assertTrue(catalog.is_published("fixture", "a"))
                self.assertFalse(catalog.is_published("fixture", "c"))
            self.assertEqual(json.loads(saved), {"mixed": ["a", "b"]})
