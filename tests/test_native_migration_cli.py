"""Native version2 bundle is readable by retained source and refuses late catalog changes."""
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

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "native/target/debug" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")


class NativeMigrationCLI(unittest.TestCase):
    def test_version_two_apply_restore_and_late_edit_refusal(self):
        self._migration(False)

    def test_real_operator_video_copy_migrates_and_source_can_restore_bundle(self):
        self._migration(True)

    def test_generated_video_bundle_replays_between_backends(self):
        self._migration("generated")

    def _migration(self, real_video):
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
            from emojikit import identity
            fmt = "video" if real_video else "static"
            if real_video:
                source = data / "001_video_aaaaaaaaaaaa.webm"
                if real_video == "generated":
                    from tests._video_fixtures import BLUE, RED, encode
                    generated = encode(data, "migration-clip", [RED, BLUE, RED])
                    generated.rename(source)
                else:
                    source_path = os.environ.get("NUMERA_REAL_VIDEO_FIXTURE")
                    if not source_path:
                        self.skipTest("private real-video replay requires NUMERA_REAL_VIDEO_FIXTURE; generated CI coverage runs separately")
                    shutil.copy2(Path(source_path), source)
                key = "v:" + "a" * 32
                _, phash = identity.fingerprint(source, "video")
            else:
                source = data / "s_fixture.png"
                shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
                key, phash = identity.fingerprint(source, "static")
            original = source.read_bytes()
            with closing(sqlite3.connect(data / "catalog.db")) as db:
                db.executescript("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);"
                                 "CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);")
                signed = phash if phash < 1 << 63 else phash - (1 << 64)
                db.execute("INSERT INTO items VALUES(?,?,?,?)", (key, "collection/" + source.name, fmt, signed))
                db.execute("INSERT INTO publications VALUES(?,?)", (key, "111111111"))
                db.execute("INSERT INTO seen_files VALUES(?,?)", (key, "fixture-unique"))
                db.commit()
            state = {"sets": [{"keys": [key]}], "skipped": []}
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            environment = {**os.environ, "PYO3_PYTHON": sys.executable}
            def run(*args):
                return subprocess.run([str(executable), "identity-repair", *args], cwd=directory, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=30,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            before = (data / "catalog.db").read_bytes()
            result = run("migrate-video-keys", "--apply")
            self.assertEqual(result.returncode, 0, result.stderr)
            if not real_video:
                self.assertEqual((data / "catalog.db").read_bytes(), before)
                self.assertFalse(list(data.glob("*.rollback.json")))
                self.assertEqual(source.read_bytes(), original)
                from unittest.mock import patch
                from emojikit import packstate
                family_state = data / "publish_x.y.json"
                family_state.write_text(json.dumps(state), encoding="utf-8")
                with patch.object(packstate, "LOCK_DIR", root / ".locks"):
                    with packstate.exclusive_lock(packstate.pack_family_lock_path("x.y")):
                        refused = run("migrate-video-keys", "--apply")
                        self.assertEqual(refused.returncode, 4, refused.stderr)
                        self.assertEqual((data / "catalog.db").read_bytes(), before)
                family_state.unlink()
                # An obsolete frozen plan may be stale, but unreadable evidence
                # cannot certify a clean catalog or retire recovery ownership.
                frozen = data / "publish_plan_fixture.json"
                frozen.write_text("{truncated", encoding="utf-8")
                refused = run("migrate-video-keys", "--apply")
                self.assertEqual(refused.returncode, 4, refused.stderr)
                self.assertEqual((data / "catalog.db").read_bytes(), before)
                self.assertFalse(list(data.glob("*.rollback.json")))
                self.assertEqual(frozen.read_text(encoding="utf-8"), "{truncated")
                return
            bundles = list(data.glob("*.rollback.json"))
            self.assertEqual(len(bundles), 1)
            document = json.loads(bundles[0].read_text(encoding="utf-8"))
            self.assertEqual(document["version"], 2)
            self.assertFalse((data / "identity-migration.journal.json").exists())
            from tests.reference import migration_bundle
            from unittest.mock import patch
            from emojikit import media_paths
            with patch.object(media_paths.resolve, "__defaults__", (root,)):
                self.assertEqual(migration_bundle.verify_current(data, document), document["database_signature"])
            self.assertIn("phash", document)
            self.assertEqual(document["direction"], "forward")
            migrated = Path(document["files"][0]["destination"])
            self.assertEqual(migrated.read_bytes(), original)
            self.assertFalse(source.exists())
            self.assertEqual(run("restore", "--bundle", str(bundles[0])).returncode, 3)
            self.assertFalse(source.exists())
            result = run("restore", "--bundle", str(bundles[0]), "--apply")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(migration_bundle.signature(data / "catalog.db"), document["before_signature"])
            self.assertEqual(json.loads((data / "publish_fixture.json").read_text(encoding="utf-8")), state)
            # Resume an older partially applied SQL migration using immutable IDs,
            # not its old path or slot, then prove retained source can restore it.
            new_key = document["key_map"][key]
            with closing(sqlite3.connect(data / "catalog.db")) as db:
                for table in ("items", "publications", "seen_files"):
                    db.execute(f"UPDATE {table} SET content_key=? WHERE content_key=?", (new_key, key))
                db.commit()
            partial_bytes = (data / "catalog.db").read_bytes()
            preview = run("migrate-video-keys", "--from-backup", document["backup"])
            self.assertEqual(preview.returncode, 3, preview.stderr)
            self.assertIn("recovered 1 old->new pair(s)", preview.stdout)
            self.assertEqual((data / "catalog.db").read_bytes(), partial_bytes)
            self.assertFalse((data / "identity-migration.journal.json").exists())
            result = run("migrate-video-keys", "--apply", "--from-backup", document["backup"])
            self.assertEqual(result.returncode, 0, result.stderr)
            legacy_result = json.loads(result.stdout.strip())
            legacy_bundle = Path(legacy_result["bundle"])
            from tests.reference import collection_migrate
            with patch.object(media_paths.resolve, "__defaults__", (root,)), patch("emojikit.packstate.LOCK_DIR", root / ".locks"):
                collection_migrate.restore_migration(data, legacy_bundle, apply=True)
            self.assertEqual(source.read_bytes(), original)
            if real_video == "generated":
                # Legacy recovery's backup intentionally preserves the earlier
                # partial SQL state, not the initial pre-migration snapshot.
                result = run("restore", "--bundle", str(bundles[0]), "--apply")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(migration_bundle.signature(data / "catalog.db"), document["before_signature"])
                self._replay(root, data, source, original, state, key, phash, run)
            with closing(sqlite3.connect(data / "catalog.db")) as db:
                db.execute("UPDATE publications SET custom_emoji_id='222222222'")
                db.commit()
            changed = (data / "catalog.db").read_bytes()
            result = run("restore", "--bundle", str(bundles[0]), "--apply")
            self.assertEqual(result.returncode, 4, result.stderr)
            self.assertIn("catalog changed outside migration", result.stderr)
            self.assertEqual((data / "catalog.db").read_bytes(), changed)
            self.assertFalse((data / "identity-migration.journal.json").exists())

    def _replay(self, root, data, source, original, state, old_key, phash, run):
        from unittest.mock import patch
        from emojikit import identity, media_paths
        from tests.reference import collection_migrate as cm, migration_bundle as bundle
        from emojikit.packstate import write_json_atomic

        new_key, _ = identity.fingerprint(source, "video")
        signed = phash if phash < 1 << 63 else phash - (1 << 64)
        mapping = {old_key: new_key}
        journal = data / "identity-migration.journal.json"
        with patch.object(media_paths.resolve, "__defaults__", (root,)), patch("emojikit.packstate.LOCK_DIR", root / ".locks"):
            backup = cm.backup_catalog(data / "catalog.db")
            files = bundle.plan_files(data / "catalog.db", mapping)
            after = cm._remap(state, mapping)
            doc = bundle.make_bundle(data, backup, mapping, {new_key: signed}, files,
                                     {"publish_fixture.json": {"before": state, "after": after}})
            manifest = backup.with_suffix(".rollback.json")
            doc["bundle"] = str(manifest)
            write_json_atomic(manifest, doc)
            original_database = (data / "catalog.db").read_bytes()
            protected = root / "assets/panel.html"
            protected_bytes = protected.read_bytes()
            for field in ("phash", "bundle"):
                tampered = json.loads(json.dumps(doc))
                if field == "phash":
                    tampered["phash"][new_key] = signed ^ 1
                else:
                    tampered["bundle"] = str(protected)
                write_json_atomic(journal, tampered)
                refused = run("migrate-video-keys", "--apply")
                self.assertEqual(refused.returncode, 4, refused.stderr)
                self.assertEqual((data / "catalog.db").read_bytes(), original_database,
                                 "inconsistent intent must refuse BEFORE SQL changes")
                self.assertEqual(json.loads(journal.read_text(encoding="utf-8")), tampered)
                self.assertEqual(source.read_bytes(), original)
                self.assertEqual(protected.read_bytes(), protected_bytes)
            write_json_atomic(journal, doc)
            # SQL committed before its journal stage, then a file moved before
            # its path commit: both are deterministic crash-state simulations.
            cm._apply_database(data / "catalog.db", mapping, {new_key: phash})
            intent = files[0]
            bundle.move_file(Path(intent["source"]), Path(intent["destination"]), intent["sha256"])
            expected = cm.apply_migration(data)
            cm.restore_migration(data, manifest, apply=True)
            write_json_atomic(manifest, doc)
            write_json_atomic(journal, doc)
            cm._apply_database(data / "catalog.db", mapping, {new_key: phash})
            bundle.move_file(source, Path(intent["destination"]), intent["sha256"])
            result = run("migrate-video-keys", "--apply")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout.strip()), expected)
            self.assertFalse(journal.exists())
            applied = json.loads(manifest.read_text(encoding="utf-8"))
            bundle.verify_applied(data, applied)
            self.assertEqual(Path(intent["destination"]).read_bytes(), original)
            self.assertEqual(json.loads((data / "publish_fixture.json").read_text(encoding="utf-8")), after)
            # A different requested bundle must not replace this reverse intent.
            applied["direction"] = "restore"
            applied["stage"] = "restore"
            write_json_atomic(journal, applied)
            alternate = data / "other.rollback.json"
            write_json_atomic(alternate, applied)
            before_refusal = (data / "catalog.db").read_bytes(), journal.read_bytes()
            refused = run("restore", "--bundle", str(alternate), "--apply")
            self.assertEqual(refused.returncode, 4, refused.stderr)
            self.assertIn("requested bundle differs", refused.stderr)
            self.assertEqual(((data / "catalog.db").read_bytes(), journal.read_bytes()), before_refusal)
            bundle.move_file(Path(intent["destination"]), source, intent["sha256"],
                             keep_source=intent["destination_existed"])
            write_json_atomic(data / "publish_fixture.json", state)
            result = run("restore", "--bundle", str(manifest), "--apply")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(bundle.signature(data / "catalog.db"), doc["before_signature"])
            self.assertEqual(source.read_bytes(), original)
            self.assertFalse(journal.exists())
            # The retained source can finish a native-origin forward journal.
            result = run("migrate-video-keys", "--apply")
            self.assertEqual(result.returncode, 0, result.stderr)
            native_manifest = Path(json.loads(result.stdout.strip())["bundle"])
            native_doc = json.loads(native_manifest.read_text(encoding="utf-8"))
            cm.restore_migration(data, native_manifest, apply=True)
            native_doc["direction"], native_doc["stage"] = "forward", "backup"
            write_json_atomic(native_manifest, native_doc)
            write_json_atomic(journal, native_doc)
            cm._apply_database(data / "catalog.db", native_doc["key_map"], native_doc["phash"])
            cm.apply_migration(data)
            bundle.verify_applied(data, json.loads(native_manifest.read_text(encoding="utf-8")))
            self.assertFalse(journal.exists())
            cm.restore_migration(data, native_manifest, apply=True)
            self.assertEqual(source.read_bytes(), original)
            self.assertEqual(bundle.signature(data / "catalog.db"), doc["before_signature"])
