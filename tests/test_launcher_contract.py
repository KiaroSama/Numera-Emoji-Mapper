"""run.ps1 only passes flags the tools still accept.

The launcher builds argument lists for seven CLIs and nothing read it: renaming
a flag in any tool broke the menu only at run time, mid-workflow. This reads
the launcher's `@('-m','emojikit.<name>', ...)` arrays and asks each module's
own --help whether every flag it is handed exists.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

from tests._cli_fixtures import cli_help  # noqa: E402

# The menu lives in run.ps1; its actions, which build the argument lists,
# in the script run.ps1 dot-sources.
LAUNCHER = (ROOT / "run.ps1", ROOT / "scripts" / "run-actions.ps1")

_MODULE_ARRAY = re.compile(r"@\(\s*'-m'\s*,\s*'emojikit\.(\w+)'([^)]*)\)")
_ASSIGN = re.compile(r"(\$\w+)\s*=\s*@\(\s*'-m'\s*,\s*'emojikit\.(\w+)'")
_APPEND = re.compile(r"(\$\w+)\s*\+=\s*@\(([^)]*)\)")
_ARRAY = re.compile(r"@\(([^)]*)\)")
_FLAG = re.compile(r"'(--[a-z][a-z0-9-]*)'")


def launcher_flags(text: str) -> dict[str, set[str]]:
    """module -> every '--flag' literal the launcher passes it."""
    out: dict[str, set[str]] = {}
    for m in _MODULE_ARRAY.finditer(text):
        flags = out.setdefault(m.group(1), set())
        flags.update(_FLAG.findall(m.group(2)))
        # `@('-m','emojikit.x') + $list + @('--flag', $v)`: later arrays in the
        # same expression, up to the end of that line, belong to it too.
        tail = text[m.end():text.find("\n", m.end())]
        for extra in _ARRAY.finditer(tail):
            flags.update(_FLAG.findall(extra.group(1)))
    # A variable belongs to its function: several actions reuse `$argv` for
    # different modules, so appends are resolved per function body.
    for chunk in re.split(r"\n(?=function )", text):
        variables = {v: mod for v, mod in _ASSIGN.findall(chunk)}
        for var, body in _APPEND.findall(chunk):
            if var in variables:
                out[variables[var]].update(_FLAG.findall(body))
    return out


class TheLauncherPassesOnlyRealFlags(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        cls.text = "\n".join(p.read_text(encoding="utf-8") for p in LAUNCHER)
        cls.flags = launcher_flags(cls.text)

    def test_the_scan_finds_the_launchers_modules(self):
        """A regex that matches nothing passes forever and proves nothing."""
        for module in ("build_pack", "build_collection", "panel", "fetch_pack",
                       "fetch_emoji_ids", "sync_order", "pack_manifest",
                       "pack_archive", "status"):
            self.assertIn(module, self.flags)
        self.assertIn("--with-pack", self.flags["panel"])
        self.assertIn("--token-env", self.flags["fetch_pack"])

    def test_every_flag_is_in_its_modules_help(self):
        for module, flags in sorted(self.flags.items()):
            if not flags:
                continue            # nothing passed, nothing to check (emoji_bot)
            with self.subTest(module=module):
                text = cli_help(f"emojikit.{module}", ROOT)
                missing = sorted(f for f in flags if f not in text)
                self.assertEqual(missing, [],
                                 f"run.ps1 passes {missing} to emojikit.{module}, "
                                 f"which no longer accepts it")

    def test_the_coin_rebuild_default_command_still_exists(self):
        """run.ps1 starts coins/rebuild_dedup.py with no argument: `all`."""
        self.assertIn("coins\\rebuild_dedup.py", self.text)
        self.assertIn("all", cli_help(str(ROOT / "coins" / "rebuild_dedup.py"), ROOT))


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_launcher_contract -v")
