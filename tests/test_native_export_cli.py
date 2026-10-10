"""Native ZIP export is complete, ordered and leaves every real media source untouched."""
from contextlib import closing
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import unittest
import zipfile
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeExportCLI(unittest.TestCase):
    def test_ordered_complete_export_and_missing_input_preserve_output(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            root = Path(directory).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            data = root / "collection"
            data.mkdir()
            packs = root / "packs"
            packs.mkdir()
            source = data / "source.png"
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
            original = source.read_bytes()
            key = "s:" + "a" * 32
            name = "fixture_by_YourEmojiBot"
            database = data / "catalog.db"
            with closing(sqlite3.connect(database)) as db:
                db.executescript("CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,keywords TEXT);"
                                 "CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);")
                db.execute("INSERT INTO items VALUES(?,?,?,?)", (key, str(source), "static", '["fixture|keyword"]'))
                db.execute("INSERT INTO publications VALUES(?,?,?,?)", ("fixture", key, name, "222222222"))
                db.commit()
            doc = {"set_name": name, "title": "Fixture", "family": "general", "pack_index": 6,
                   "emoji": [{"slot": 1, "role": "brand-logo", "custom_emoji_id": "111111111", "history_key": "logo:" + name},
                             {"slot": 2, "role": "emoji", "custom_emoji_id": "222222222", "history_key": "ck:" + key}]}
            (packs / f"{name}.json").write_text(json.dumps(doc), encoding="utf-8")
            inputs = {"catalog.db": [database.stat().st_size, int(database.stat().st_mtime)],
                      "publish_fixture.json": None, "rebuild_dedup_state.json": None, "ticker_to_id.json": None}
            index_path = packs / "index.json"
            index_path.write_text(json.dumps({"inputs": inputs}), encoding="utf-8")
            destination = root / "output.zip"
            environment = {**os.environ, "COLLECTION_PACK_BASE": "fixture",
                           "BRAND_LOGO_PATH": str(ROOT / "assets/numera-emoji-mapper-logo.png")}
            def run():
                return subprocess.run([str(executable), "pack-archive", "--export", "6", "--zip", str(destination)],
                    cwd=directory, env=environment, stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=15,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            with zipfile.ZipFile(destination) as archive:
                self.assertEqual(archive.namelist(), ["001_logo.png", "002_static_" + "a" * 12 + ".png", "_manifest.md"])
                self.assertIsNone(archive.testzip())
                self.assertEqual(archive.read("001_logo.png"), original)
                self.assertEqual(archive.read("002_static_" + "a" * 12 + ".png"), original)
                self.assertIn("fixture\\|keyword", archive.read("_manifest.md").decode("utf-8"))
            self.assertEqual(source.read_bytes(), original)
            complete = destination.read_bytes()
            source.unlink()
            result = run()
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("slot 2", result.stdout)
            self.assertEqual(destination.read_bytes(), complete)
            self.assertFalse(list(root.glob("*.tmp")))
            inputs["catalog.db"] = None
            index_path.write_text(json.dumps({"inputs": inputs}), encoding="utf-8")
            self.assertEqual(run().returncode, 3)
            self.assertEqual(destination.read_bytes(), complete)
