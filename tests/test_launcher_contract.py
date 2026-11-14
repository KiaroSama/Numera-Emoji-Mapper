"""The primary launcher passes only supported native command flags.

The actual executable's help validates menu arguments; an empty scanner must
fail rather than silently certifying a launcher that invokes retired applications.
"""

from __future__ import annotations

import os
import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

from tests.test_native_local_cli import BINARY  # noqa: E402

RUNS_ON_NATIVE_WINDOWS = True

# The public script delegates to the native menu. Its compiled action builders
# remain the source of command/flag literals checked against executable help.
LAUNCHER = (ROOT / "run.ps1", ROOT / "native/src/tooling_menu_actions.rs")

_MODULE_ARRAY = re.compile(r"@\(\s*'([a-z][a-z-]*)'([^)]*)\)")
_ASSIGN = re.compile(r"(\$\w+)\s*=\s*@\(\s*'([a-z][a-z-]*)'")
_APPEND = re.compile(r"(\$\w+)\s*\+=\s*@\(([^)]*)\)")
_ARRAY = re.compile(r"@\(([^)]*)\)")
_FLAG = re.compile(r"'(--[a-z][a-z0-9-]*)'")


def launcher_flags(text: str) -> dict[str, set[str]]:
    """module -> every '--flag' literal the launcher passes it."""
    if "argv(&[" in text:
        return rust_launcher_flags(text)
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


def rust_launcher_flags(text: str) -> dict[str, set[str]]:
    """Read the native builders, including cloned vectors and conditional routes."""
    out: dict[str, set[str]] = {}
    literal = re.compile(r'"([^"\\]*(?:\\.[^"\\]*)*)"')
    for chunk in re.split(r"\n(?=    fn |pub\(crate\) fn )", text):
        variables: dict[str, set[str]] = {}
        conditional: dict[str, tuple[str, str, str]] = {}
        for match in re.finditer(
                r'let (?:mut )?(\w+) = (?:argv\(&\[)?if (\w+)\s*'
                r'\{\s*"([a-z][a-z-]*)"\s*\}\s*else\s*'
                r'\{\s*"([a-z][a-z-]*)"\s*\}', chunk):
            name, condition, yes, no = match.groups()
            variables[name] = {yes, no}
            conditional[name] = (condition, yes, no)
        for match in re.finditer(r'argv\(&\[(.*?)\]\)', chunk, re.S):
            body = match.group(1)
            first = body.split(',', 1)[0].strip()
            commands = {value for value in literal.findall(first)
                        if re.fullmatch(r"[a-z][a-z-]*", value)}
            commands.update(variables.get(first, set()))
            if not commands:
                continue
            for command in commands:
                flags = {value for value in literal.findall(body) if value.startswith('--')}
                if first in conditional:
                    condition, yes, _no = conditional[first]
                    for branch in re.finditer(r'if ' + re.escape(condition) + r'\s*'
                            r'\{\s*"(--[a-z-]+)"\s*\}\s*else\s*'
                            r'\{\s*"(--[a-z-]+)"\s*\}', body):
                        positive, negative = branch.groups()
                        flags.difference_update((positive, negative))
                        flags.add(positive if command == yes else negative)
                out.setdefault(command, set()).update(flags)
            assignment = re.search(r'let (?:mut )?(\w+) =\s*$', chunk[:match.start()])
            if assignment:
                variables[assignment.group(1)] = commands
        for match in re.finditer(r'let (?:mut )?(\w+) = (\w+)\.clone\(\)', chunk):
            if match.group(2) in variables:
                variables[match.group(1)] = variables[match.group(2)]
        for match in re.finditer(r'(\w+)\.(?:push|extend)\((.*?)\);', chunk, re.S):
            name, payload = match.groups()
            commands = variables.get(name, set())
            if name in conditional:
                condition, yes, no = conditional[name]
                # The collection builder emits ids only on its ids branch;
                # its token option is emitted only on the alternate branch.
                before = chunk[:match.start()]
                branch = re.search(r'if ' + re.escape(condition) + r'\s*\{[^{}]*$', before)
                if branch:
                    commands = {yes}
                elif re.search(r'if let Some\(token\) = token\s*\{[^{}]*$', before):
                    commands = {no}
            for command in commands:
                out[command].update(value for value in literal.findall(payload)
                                    if value.startswith('--'))
    return out


class TheLauncherPassesOnlyRealFlags(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        cls.text = "\n".join(p.read_text(encoding="utf-8") for p in LAUNCHER)
        cls.flags = launcher_flags(cls.text)

    def test_the_scan_finds_the_launchers_modules(self):
        """A regex that matches nothing passes forever and proves nothing."""
        for command in ("build-pack", "build-collection", "panel", "fetch-pack",
                        "fetch-emoji-ids", "sync-order", "pack-manifest",
                        "pack-archive", "status", "make-emoji-pngs", "add-media", "emoji-bot"):
            self.assertIn(command, self.flags)
        self.assertIn("--with-pack", self.flags["panel"])
        self.assertIn("--token-env", self.flags["fetch-pack"])
        self.assertNotIn("'emojikit.", self.text)
        script = (ROOT / "run.ps1").read_text(encoding="utf-8")
        self.assertIn("@('menu')", script)
        self.assertIn("@('doctor')", script)
        self.assertNotIn(".venv", script)
        self.assertNotIn("Invoke-Py", script)

    def test_every_flag_is_in_its_modules_help(self):
        for module, flags in sorted(self.flags.items()):
            if not flags:
                continue            # nothing passed, nothing to check (emoji_bot)
            with self.subTest(module=module):
                result = subprocess.run([str(BINARY), module, "--help"], cwd=ROOT,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=5,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                missing = sorted(f for f in flags if f not in result.stdout)
                self.assertEqual(missing, [], f"run.ps1 passes unsupported {missing} to {module}")

    def test_the_coin_rebuild_default_command_still_exists(self):
        """The native menu still starts the default complete coin rebuild."""
        self.assertIn('argv(&["coin-rebuild"])', self.text)
        result = subprocess.run([str(BINARY), "coin-rebuild", "--help"], cwd=ROOT,
            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=5,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("all", result.stdout)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_launcher_contract -v")
