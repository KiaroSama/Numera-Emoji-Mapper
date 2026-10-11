"""Prebuilt environment paths stay physically in their owning package."""
from pathlib import Path
import os
import tempfile
import subprocess
import unittest
from unittest.mock import patch

from scripts.local_environment import confined_directory, environment

ROOT = Path(__file__).resolve().parents[1]


class PrebuiltEnvironment(unittest.TestCase):
    def test_existing_installation_refuses_before_writing(self):
        from scripts import install_prebuilt
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder)
            (root / ".venv").mkdir()
            prior = root / ".venv/pyvenv.cfg"
            prior.write_bytes(b"previous working environment")
            with patch.object(install_prebuilt, "ROOT", root), patch.object(install_prebuilt, "verify", return_value={}):
                with self.assertRaisesRegex(ValueError, "Existing installation preserved"):
                    install_prebuilt.install()
            self.assertEqual(prior.read_bytes(), b"previous working environment")
            self.assertEqual(sorted(path.name for path in root.iterdir()), [".venv"])

    def test_owned_temp_and_caches_override_external_values(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve()
            before = {"TEMP": "/outside", "PIP_CACHE_DIR": "/outside", "KEEP": "unchanged"}
            actual = environment(root, before, build=True)
            self.assertEqual(before["TEMP"], "/outside", "input environment was mutated")
            self.assertEqual(actual["KEEP"], "unchanged")
            for key in ("TEMP", "TMP", "TMPDIR", "PIP_CACHE_DIR", "XDG_CACHE_HOME", "PYTHONPYCACHEPREFIX",
                        "CARGO_HOME", "CARGO_TARGET_DIR", "RUSTUP_HOME", "HOMEBREW_CACHE", "HOMEBREW_TEMP"):
                with self.subTest(key=key):
                    self.assertTrue(Path(actual[key]).resolve().is_relative_to(root))
                    self.assertTrue(Path(actual[key]).is_dir())

    def test_linked_cache_refuses_before_using_outside_directory(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "package"
            root.mkdir()
            outside = Path(folder) / "outside"
            outside.mkdir()
            linked = root / ".venv"
            if os.name == "nt":
                subprocess.run(["cmd.exe", "/c", "mklink", "/J", str(linked), str(outside)],
                               check=True, capture_output=True, timeout=5)
            else:
                linked.symlink_to(outside, target_is_directory=True)
            try:
                with self.assertRaisesRegex(ValueError, "Linked project-local storage"):
                    confined_directory(root, ".venv", create=False)
                self.assertEqual(list(outside.iterdir()), [])
            finally:
                if os.name == "nt":
                    linked.rmdir()
                else:
                    linked.unlink()
