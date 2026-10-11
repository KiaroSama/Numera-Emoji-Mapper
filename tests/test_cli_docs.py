"""GUIDE §12, the CLI reference, is checked against each tool's own --help.

About 900 hand-written lines whose only guard was a checklist: remap_ids'
`--cache` and verify_logos' `--top` were in no section at all. For every
section that names its tool, every flag the tool accepts must be documented
there, and every flag documented there must exist.

Only tools with an argparse parser are run: a tool without one does its real
work when handed `--help` (emoji_bot starts polling, alias_map rewrites the map).
"""

from __future__ import annotations

import os
import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

from tests._cli_fixtures import cli_help  # noqa: E402

GUIDES = (ROOT / "docs/guide-cli.md", ROOT / "docs/guide-cli-panel-coin.md")
NATIVE = {
    "make-emoji-pngs", "build-pack", "fetch-pack", "fetch-emoji-ids", "add-media",
    "build-collection", "sync-order", "pack-manifest", "pack-archive", "status", "panel",
}
from tests.test_native_local_cli import BINARY  # noqa: E402

RUNS_ON_NATIVE_WINDOWS = True
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
    part = "\n".join(path.read_text(encoding="utf-8") for path in GUIDES)
    pieces = re.split(r"(?m)^### (12\.\w+) ", part)[1:]
    out = {}
    for num, body in zip(pieces[::2], pieces[1::2], strict=True):
        title = body.splitlines()[0]
        files = [name for name in re.findall(r"`([a-z][a-z-]+)`", title) if name in NATIVE]
        if "`coins/`" in title:
            files = [ROOT / "coins" / f"{m}.py"
                     for m in sorted(set(re.findall(r"coins[\\/](\w+)\.py", body)))]
        out[num] = (files, body)
    return out


def _has_parser(path: Path) -> bool:
    return path.is_file() and "ArgumentParser(" in path.read_text(encoding="utf-8")


def _target(path: Path) -> str:
    return (f"tests.reference.{path.stem}" if path.parent.name == "emojikit" else str(path))


class TheCliReferenceMatchesTheTools(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        cls.sections = sections()

    def test_the_scan_finds_the_sections(self):
        """A parser that matches nothing passes forever and proves nothing."""
        self.assertIn("12.2", self.sections)
        self.assertEqual(self.sections["12.2"][0], ["build-pack"])
        self.assertGreaterEqual(len(self.sections["12.8"][0]), 5, "the coin tools")

    def test_every_flag_is_documented_and_every_documented_flag_exists(self):
        for num, (files, body) in sorted(self.sections.items()):
            tools = [f for f in files if isinstance(f, str) or _has_parser(f)]
            if not tools:
                continue
            with self.subTest(section=num):
                real: set[str] = set()
                for tool in tools:
                    if isinstance(tool, str):
                        result = subprocess.run([str(BINARY), tool, "--help"], cwd=ROOT,
                            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=5,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                        self.assertEqual(result.returncode, 0, result.stderr)
                        help_text = result.stdout
                    else:
                        help_text = cli_help(_target(tool), ROOT)
                    real |= set(_FLAG.findall(help_text))
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
