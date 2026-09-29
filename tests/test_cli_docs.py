"""GUIDE §12, the CLI reference, is checked against each tool's own --help.

About 900 hand-written lines whose only guard was a checklist: remap_ids'
`--cache` and verify_logos' `--top` were in no section at all. For every
section that names its tool, every flag the tool accepts must be documented
there, and every flag documented there must exist.

Only tools with an argparse parser are run: a tool without one does its real
work when handed `--help` (emoji_bot starts polling, alias_map rewrites the map).
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

from tests._cli_fixtures import cli_help  # noqa: E402

GUIDE = ROOT / "docs" / "GUIDE.md"
_FLAG = re.compile(r"(?<![\w-])--[a-z][a-z0-9-]*")

# Flags a section mentions that are not its tool's own, each with the reason.
MENTIONED_NOT_OWNED = {
    # The panel section explains which build_collection options the panel's
    # own buttons map to.
    "12.6": {"--new-set", "--per-set", "--source"},
    # The status command runs pack_manifest's and pack_archive's --check.
    "12.5e": {"--check"},
    # --export is refused while the roster is stale and says to run its --refresh.
    "12.5d": {"--refresh"},
}


def sections() -> dict[str, tuple[list[Path], str]]:
    """§12 subsection number -> (the tool files it documents, its text)."""
    text = GUIDE.read_text(encoding="utf-8")
    part = text[text.index("## 12. Full CLI reference"):text.index("\n## 13.")]
    pieces = re.split(r"(?m)^### (12\.\w+) ", part)[1:]
    out = {}
    for num, body in zip(pieces[::2], pieces[1::2], strict=True):
        title = body.splitlines()[0]
        files = [ROOT / "emojikit" / f"{m}.py" for m in re.findall(r"emojikit/(\w+)\.py", title)]
        if "`coins/`" in title:
            files = [ROOT / "coins" / f"{m}.py"
                     for m in sorted(set(re.findall(r"coins[\\/](\w+)\.py", body)))]
        out[num] = (files, body)
    return out


def _has_parser(path: Path) -> bool:
    return path.is_file() and "ArgumentParser(" in path.read_text(encoding="utf-8")


def _target(path: Path) -> str:
    return (f"emojikit.{path.stem}" if path.parent.name == "emojikit" else str(path))


class TheCliReferenceMatchesTheTools(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        cls.sections = sections()

    def test_the_scan_finds_the_sections(self):
        """A parser that matches nothing passes forever and proves nothing."""
        self.assertIn("12.2", self.sections)
        self.assertEqual(self.sections["12.2"][0], [ROOT / "emojikit" / "build_pack.py"])
        self.assertGreaterEqual(len(self.sections["12.8"][0]), 5, "the coin tools")

    def test_every_flag_is_documented_and_every_documented_flag_exists(self):
        for num, (files, body) in sorted(self.sections.items()):
            tools = [f for f in files if _has_parser(f)]
            if not tools:
                continue
            with self.subTest(section=num):
                real: set[str] = set()
                for tool in tools:
                    real |= set(_FLAG.findall(cli_help(_target(tool), ROOT)))
                real.discard("--help")
                documented = set(_FLAG.findall(body))
                self.assertEqual(sorted(real - documented), [],
                                 f"§{num}: flags the tool accepts but the GUIDE never names")
                stale = documented - real - MENTIONED_NOT_OWNED.get(num, set())
                self.assertEqual(sorted(stale), [],
                                 f"§{num}: the GUIDE names flags the tool does not have")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_cli_docs -v")
