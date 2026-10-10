"""Top-level native help advertises the developed command surface without invoking it."""
import os
import subprocess
import unittest

from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeCommandHelp(unittest.TestCase):
    def test_top_level_help_names_existing_backend_commands(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        result = subprocess.run([str(BINARY), "--help"], cwd=ROOT,
            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=5,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        self.assertEqual(result.returncode, 0, result.stderr)
        for command in ("panel", "add-media", "fetch-pack", "fetch-emoji-ids", "build-collection",
                        "build-pack", "emoji-bot", "sync-order", "plan-apply", "make-emoji-pngs",
                        "plan-status", "pack-manifest", "status", "pack-archive", "identity-repair"):
            with self.subTest(command=command):
                self.assertIn(command, result.stdout.split(), "supported command missing from help")
        self.assertEqual(result.stderr, "")

    def test_extra_identity_command_is_a_usage_error_before_catalog_access(self):
        result = subprocess.run([str(BINARY), "identity-repair", "report", "restore"], cwd=ROOT,
            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=5,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("only one identity-repair command", result.stderr)
        self.assertNotIn("no catalog", result.stdout)
