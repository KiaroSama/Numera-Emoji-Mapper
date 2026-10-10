"""Production native artifacts never contain test-only source or protected local content."""
from pathlib import Path, PurePosixPath
import unittest
import zipfile

ROOT = Path(__file__).resolve().parent.parent

RUNS_ON_NATIVE_WINDOWS = True


class NativePackage(unittest.TestCase):
    def test_actual_built_wheel_excludes_reference_and_local_content(self):
        wheels = list((ROOT / "native/target/wheels").glob("*.whl"))
        self.assertTrue(wheels, "build the required wheel before packaging acceptance")
        wheel = max(wheels, key=lambda path: path.stat().st_mtime_ns)
        forbidden = {"tests", "private", ".ai", ".claude", ".kiro", ".codex", ".agents",
                     ".specify", "specs", "plans", "logs", "collection", "packs",
                     "graphify-out", ".codebase-memory"}
        with zipfile.ZipFile(wheel) as archive:
            names = archive.namelist()
            self.assertTrue(any(name.startswith("emojikit/_native") and name.endswith((".so", ".pyd"))
                                for name in names))
            for name in names:
                path = PurePosixPath(name)
                with self.subTest(member=name):
                    self.assertFalse(set(path.parts) & forbidden)
                    self.assertNotIn(path.name.lower(), {"secrets.md", "explain-ai.md", ".ignoreme"})
                    self.assertFalse(path.name.startswith(".env"))
            self.assertIsNone(archive.testzip())
