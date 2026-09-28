"""A test module run on its own must stay inside the hermetic guard.

`tests/__init__.py` scrubs credentials and blocks outbound sockets because a
test once reached live Telegram and replaced a sticker in a published pack. It
runs only when the `tests` package is imported. `python tests/test_x.py` (or an
editor's "Run file") skips it, so every module refuses a direct run, and
`load_env()` treats an imported `unittest` as "a test owns this process" so it
never reads the real `.env` there either.
"""

from __future__ import annotations

import ast
import os
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "tests"


def _main_blocks(tree: ast.Module):
    for node in ast.walk(tree):
        if (isinstance(node, ast.If) and isinstance(node.test, ast.Compare)
                and isinstance(node.test.left, ast.Name)
                and node.test.left.id == "__name__"):
            yield node


def _calls_unittest_main(block: ast.If) -> bool:
    for node in ast.walk(block):
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and node.func.attr == "main"
                and isinstance(node.func.value, ast.Name)
                and node.func.value.id == "unittest"):
            return True
    return False


class DirectRunsRefuse(unittest.TestCase):
    def test_no_test_module_starts_unittest_from_its_main_block(self):
        offenders = []
        for path in sorted(TESTS.glob("test_*.py")):
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
            if any(_calls_unittest_main(b) for b in _main_blocks(tree)):
                offenders.append(path.name)
        self.assertEqual(offenders, [],
                         "these run unittest.main() when started directly, which "
                         "skips tests/__init__.py; raise SystemExit instead")


class DotenvDetector(unittest.TestCase):
    def _probe(self, code: str) -> str:
        env = {"NUMERA_EMOJI_MAPPER_NO_DOTENV": "1"}
        for key in ("SYSTEMROOT", "PATH"):
            if key in os.environ:
                env[key] = os.environ[key]
        done = subprocess.run([sys.executable, "-c", code], cwd=ROOT, env=env,
                              stdin=subprocess.DEVNULL, capture_output=True,
                              text=True, encoding="utf-8", timeout=60)
        self.assertEqual(done.returncode, 0, done.stderr)
        return done.stdout.strip()

    def test_an_imported_unittest_counts_as_a_test_run_and_nothing_else_does(self):
        probe = "from emojikit.cli_env import _under_a_test_runner as f; print(f())"
        self.assertEqual(self._probe("import unittest; " + probe), "True")
        self.assertEqual(self._probe(probe), "False")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_suite_guard -v")
