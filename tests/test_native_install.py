"""Native executable installs independently of Cargo cache and preserves prior bytes on failure."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import build_native
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeInstall(unittest.TestCase):
    def test_installed_binary_runs_after_source_removed_and_failed_replace_preserves_it(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "root with spaces"
            (root / "native/target/release").mkdir(parents=True)
            source = root / "native/target/release" / BINARY.name
            shutil.copy2(BINARY, source)
            with mock.patch.object(build_native, "ROOT", root):
                installed = build_native.install_executable(source)
                self.assertEqual(installed.parent, root / "native/runtime")
                self.assertEqual(installed.read_bytes(), source.read_bytes())
                source.unlink()
                result = subprocess.run([str(installed), "--help"], cwd=folder,
                    capture_output=True, encoding="utf-8", stdin=subprocess.DEVNULL, timeout=5,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("identity-repair", result.stdout)
                (root / "assets").mkdir()
                shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
                shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
                result = subprocess.run([str(installed), "status"], cwd=folder,
                    capture_output=True, encoding="utf-8", stdin=subprocess.DEVNULL, timeout=5,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertIn("STALE roster", result.stdout)
                self.assertFalse((root / "collection/catalog.db").exists())
                logs = list((root / "logs").glob("status_*_UTC_*.log"))
                self.assertEqual(len(logs), 1)
                text = logs[0].read_text(encoding="utf-8")
                self.assertIn("[runtime] native backend started", text)
                self.assertIn("exit=3", text)
                before = installed.read_bytes()
                source.write_bytes(b"replacement bytes")
                with mock.patch.object(build_native.os, "replace", side_effect=PermissionError("fixture occupied")):
                    with self.assertRaises(PermissionError):
                        build_native.install_executable(source)
                self.assertEqual(installed.read_bytes(), before)
                self.assertEqual(list(installed.parent.iterdir()), [installed])
