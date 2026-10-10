"""Native status counts waiting publication without treating it as stale or changing SQLite."""
from contextlib import closing
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import unittest
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeStatusCLI(unittest.TestCase):
    def test_waiting_publication_is_info_and_stale_roster_is_exit_three(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            root = Path(directory).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            (root / "collection").mkdir()
            (root / "packs").mkdir()
            database = root / "collection/catalog.db"
            with closing(sqlite3.connect(database)) as db:
                db.executescript("CREATE TABLE items(content_key TEXT,included INTEGER);"
                                 "CREATE TABLE publications(base TEXT,content_key TEXT);"
                                 "INSERT INTO items VALUES('s:a',1),('s:b',1),('s:c',0);"
                                 "INSERT INTO publications VALUES('fixture','s:a');")
                db.commit()
            before = database.read_bytes()
            inputs = {"catalog.db": [database.stat().st_size, int(database.stat().st_mtime)],
                      "publish_fixture.json": None, "rebuild_dedup_state.json": None, "ticker_to_id.json": None}
            index = root / "packs/index.json"
            index.write_text(json.dumps({"inputs": inputs}), encoding="utf-8")
            environment = {**os.environ, "COLLECTION_PACK_BASE": "fixture", "EMOJI_ARCHIVE_DIR": ""}
            def run():
                return subprocess.run([str(executable), "status"], cwd=directory, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("1 included emoji not yet published to fixture", result.stdout)
            self.assertIn("skip  archive", result.stdout)
            self.assertNotIn("STALE", result.stdout)
            self.assertEqual(database.read_bytes(), before)
            index.unlink()
            result = run()
            self.assertEqual(result.returncode, 3, result.stderr)
            self.assertIn("numera-emoji pack-manifest --refresh", result.stdout)
            self.assertEqual(database.read_bytes(), before)
