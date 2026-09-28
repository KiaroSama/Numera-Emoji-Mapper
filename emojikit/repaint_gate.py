"""The question asked before ingesting emoji that Telegram repaints.

Split out of `build_pack.py` (file-size limit); `build_pack` re-exports it.
"""

from __future__ import annotations

import logging
import sys

log = logging.getLogger("build_pack")


# What to do about emoji Telegram repaints (see media.is_repaintable).
REPAINT_MODES = ("ask", "skip", "keep")
_REPAINT_SAMPLE = 8
_NO_ANSWER = ("  nobody answered, so skipping them. Re-run with "
              "--repaintable keep to ingest them.")


def repaintable_gate(labels: list[str], *, mode: str = "ask",
                     prompt=None) -> bool:
    """Should these repaintable emoji be ingested? True = keep them.

    ``ask`` prompts, but only when there is someone to answer: with no tty the
    answer is SKIP, because skipping is the reversible half. A skipped emoji is
    one re-run away with ``--repaintable keep``; one already published into a
    live set has to be replaced sticker by sticker -- the exact round trip this
    gate exists to prevent.
    """
    if not labels:
        return True
    n = len(labels)
    shown = ", ".join(labels[:_REPAINT_SAMPLE])
    if n > _REPAINT_SAMPLE:
        shown += f", ... (+{n - _REPAINT_SAMPLE} more)"
    warning = "\n".join((
        f"WARNING: {n} of these emoji are REPAINTABLE: {shown}",
        "  Telegram OVERRIDES their colours with the text/accent colour, so"
        " what you see in the source pack is not the stored art.",
        "  In a pack without the flag they show that stored art instead --"
        " sometimes flat black, sometimes full colour, so LOOK before deciding.",
        "  The flag is set once per set at creation; it cannot be added later.",
    ))
    log.warning("%d repaintable emoji in this batch: %s", n, shown)
    print(warning, file=sys.stderr)

    if mode == "keep":
        print("  --repaintable keep: ingesting them anyway.", file=sys.stderr)
        return True
    if mode == "skip":
        print("  --repaintable skip: leaving them out.", file=sys.stderr)
        return False

    if prompt is None:
        if not sys.stdin.isatty():
            print(_NO_ANSWER, file=sys.stderr)
            return False
        prompt = input
    try:
        answer = prompt("  Ingest them anyway? [y/N] ")
    except (EOFError, KeyboardInterrupt):
        # isatty() is NOT enough: under Git Bash `... < /dev/null` still reports
        # a tty, and input() then raises EOFError and kills the whole ingest.
        # A question nobody answered is a no, never a crash.
        print("", file=sys.stderr)
        print(_NO_ANSWER, file=sys.stderr)
        return False
    return answer.strip().lower() in ("y", "yes")
