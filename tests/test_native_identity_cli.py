"""Read-only native identity report proves real static identity and retains obsolete history."""
from contextlib import closing
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeIdentityCLI(unittest.TestCase):
    def test_report_preserves_database_and_refuses_missing_media(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            root = Path(directory).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            shutil.copytree(ROOT / "emojikit", root / "emojikit", ignore=shutil.ignore_patterns("__pycache__"))
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            data = root / "collection"
            data.mkdir()
            source = data / "source.png"
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
            from emojikit import identity
            key, phash = identity.fingerprint(source, "static")
            database = data / "catalog.db"
            with closing(sqlite3.connect(database)) as db:
                db.executescript("CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,phash INTEGER);"
                                 "CREATE TABLE publications(content_key TEXT);CREATE TABLE seen_files(content_key TEXT);")
                signed = phash if phash < 1 << 63 else phash - (1 << 64)
                db.execute("INSERT INTO items VALUES(?,?,?,?)", (key, "./source.png", "static", signed))
                db.commit()
            plan = {"static": [key, "s:obsolete"]}
            (data / "publish_plan_fixture.json").write_text(json.dumps(plan), encoding="utf-8")
            before = database.read_bytes()
            environment = {**os.environ, "PYO3_PYTHON": sys.executable}
            def run(*args):
                return subprocess.run([str(executable), "identity-repair", *args], cwd=directory, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=20,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            from emojikit.maintenance import maintenance
            with maintenance(data):
                blocked = run("report")
                self.assertEqual(blocked.returncode, 4, blocked.stderr)
                self.assertFalse((data / "tmp/identity-codec").exists(),
                                 "report must acquire canonical ownership before decoding")
            result = run("report")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Clean: identities, media", result.stdout)
            self.assertIn("s:obsolete", result.stdout)
            import contextlib
            import io
            from unittest.mock import patch
            from tests.reference import identity_repair
            with patch("emojikit.media_paths.resolve.__defaults__", (root,)), contextlib.redirect_stdout(io.StringIO()) as output:
                expected_code = identity_repair.report(data)
            self.assertEqual(result.returncode, expected_code)
            self.assertEqual(result.stdout, output.getvalue())
            self.assertEqual(database.read_bytes(), before)
            (data / "publish_plan_fixture.json").unlink()
            preview = run("migrate-video-keys")
            with patch("emojikit.media_paths.resolve.__defaults__", (root,)), patch("emojikit.packstate.LOCK_DIR", root / ".locks"), contextlib.redirect_stdout(io.StringIO()) as output:
                expected_code = identity_repair.migrate(data, apply=False)
            self.assertEqual(preview.returncode, expected_code)
            self.assertEqual(preview.stdout, output.getvalue())
            (data / "publish_plan_fixture.json").write_text(json.dumps(plan), encoding="utf-8")
            journal = data / "identity-migration.journal.json"
            for corrupt in ("{truncated", "{}", "[]"):
                journal.write_text(corrupt, encoding="utf-8")
                refused = run("report")
                self.assertEqual(refused.returncode, 4, refused.stderr)
                self.assertEqual(database.read_bytes(), before)
                self.assertEqual(journal.read_text(encoding="utf-8"), corrupt)
            journal.unlink()
            result = run("report", "--data-dir", "missing-catalog")
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertFalse((root / "missing-catalog").exists())
            for options in ((), ("--apply",)):
                refused = run("migrate-video-keys", "--from-backup", "missing.db", *options)
                self.assertEqual(refused.returncode, 2, refused.stderr)
                self.assertFalse((data / "missing.db").exists())
            plan_path = data / "pack_plan.json"
            plan_path.write_text(json.dumps({"version": 1, "moves": {}, "held": []}), encoding="utf-8")
            refused = run("report")
            self.assertEqual(refused.returncode, 4, refused.stderr)
            self.assertEqual(database.read_bytes(), before)
            plan_path.unlink()
            source.unlink()
            result = run("report")
            self.assertEqual(result.returncode, 4, result.stderr)
            self.assertIn("missing catalog media", result.stdout)
            self.assertEqual(database.read_bytes(), before)
            (data / "publish_fixture.json").write_text(json.dumps({"sets": [{"keys": ["s:missing"]}]}), encoding="utf-8")
            self.assertEqual(run("report").returncode, 3)
            self.assertEqual(database.read_bytes(), before)
