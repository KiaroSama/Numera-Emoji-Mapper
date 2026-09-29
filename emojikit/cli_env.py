"""Helpers every command-line entry point shares: exit codes, environment, .env.

Split out of `build_pack.py`, which had grown past the project's file-size limit and
which twenty-odd modules imported only for these few names. `build_pack` re-exports
them, so existing imports keep working.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


# Exit codes shared by every CLI entry point, so a launcher or CI job can tell
# "done", "bad input", "retry me" and "stop" apart. Returning 0 after total
# failure made retry logic and menu actions treat a dead run as a success.
EXIT_OK = 0
EXIT_USAGE = 2       # invalid arguments or configuration
EXIT_PARTIAL = 3     # some items succeeded, some failed -- retryable
EXIT_FAILED = 4      # nothing succeeded, or an integrity stop


def ingest_exit_code(succeeded: int, failed: int) -> int:
    """Exit code for a batch that processed ``succeeded`` and ``failed`` items."""
    if not failed:
        return EXIT_OK
    return EXIT_PARTIAL if succeeded else EXIT_FAILED


def safe_int_env(name: str, default: int = 0, *, minimum: int | None = None,
                 maximum: int | None = None) -> int:
    """Parse a numeric env var without letting a typo kill the process.

    ``int(os.environ.get(...))`` at import time turns one bad character in .env
    into an unexplained crash before argparse can print anything useful.
    """
    raw = (os.environ.get(name) or "").strip()
    if not raw:
        return default
    try:
        value = int(raw)
    except ValueError:
        print(f"WARNING: {name} is not a whole number; using {default}.",
              file=sys.stderr)
        return default
    if minimum is not None and value < minimum:
        print(f"WARNING: {name}={value} below {minimum}; using {minimum}.",
              file=sys.stderr)
        return minimum
    if maximum is not None and value > maximum:
        print(f"WARNING: {name}={value} above {maximum}; using {maximum}.",
              file=sys.stderr)
        return maximum
    return value


def _under_a_test_runner() -> bool:
    """True when a test runner, not a tool, owns this process.

    The suite's own guard (tests/__init__.py) sets NUMERA_EMOJI_MAPPER_NO_DOTENV, but
    it protects only what is imported AFTER it, and it runs at all only when
    `tests` is imported as a package -- `unittest discover -s tests` without
    `-t .` loads the modules as top level and skips it. Either way the modules
    under test call load_env() at import time and put the real credentials back;
    the scrub that follows removes credential-SHAPED names, so anything else in
    .env survives in os.environ for the whole run.

    Deciding here makes the protection independent of how the suite was invoked
    and of which module imported first. The signal is exact -- it reads the spec
    of the process entry point, so an ordinary CLI run cannot trip it -- and a
    false positive would only mean .env is not auto-loaded, with explicit
    environment variables still working. It fails safe in both directions.
    A direct `python tests/test_x.py` run has no `__spec__`, and every test
    module imports `unittest` while no entry point does, so its presence is the
    reliable signal for that case.
    """
    spec = getattr(sys.modules.get("__main__"), "__spec__", None)
    entry = getattr(spec, "name", "") or ""
    return (entry.split(".")[0] in {"unittest", "pytest"}
            or "pytest" in sys.modules or "unittest" in sys.modules)


def load_env() -> None:
    # Several modules call load_env() at IMPORT time, which would put the real
    # credentials straight back into os.environ after the suite scrubbed them --
    # reopening the hole that once let a test reach live Telegram and replace a
    # sticker in a production pack.
    if os.environ.get("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" or _under_a_test_runner():
        return
    env = ROOT / ".env"
    if env.is_file():
        for k, v in _parse_env_file(env).items():
            os.environ.setdefault(k, v)


def _parse_env_file(path: Path) -> dict[str, str]:
    """KEY=VALUE lines, '#' comments, optional quotes.

    utf-8-sig, not utf-8: a .env saved with a byte order mark (Windows
    PowerShell 5.1 writes one) otherwise turns the first key into
    "﻿KEY", so that setting silently reads as unset -- while the Worker's
    put-secrets.ps1, which strips the mark, sees it fine.
    """
    out: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            k, v = line.split("=", 1)
            out.setdefault(k.strip(), v.strip().strip('"').strip("'"))
    return out
