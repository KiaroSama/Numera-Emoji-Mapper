"""Is everything current? One offline, read-only answer.

    python -m emojikit.status

Runs the checks that were built to run automatically -- the pack roster
(`pack_manifest --check`) and the pack archive (`pack_archive --check`) -- and
counts the catalog's included emoji not yet published, printing one line per
item and the command that fixes it. No network, no writes.

Exit 0 when nothing is stale, 3 when something is (the same code both checks use).
"""

from __future__ import annotations

import argparse
import sqlite3

from emojikit import operator_config, pack_archive, pack_manifest
from emojikit.build_pack import EXIT_OK, load_env
from emojikit.errors import OperatorConfigMissing
from emojikit.logsetup import record_exit_code, setup_logging

EXIT_STALE = 3


def _unpublished(base: str) -> int | None:
    """Included catalog items not yet published to ``base``; None if unreadable.

    Read-only on purpose: opening a Catalog object may migrate the file, and a
    status command must change nothing.
    """
    db = pack_manifest.CATALOG
    if not db.is_file():
        return None
    try:
        # as_uri(): a path with a space (this project lives in one) must be
        # percent-encoded in a file: URI.
        con = sqlite3.connect(f"{db.resolve().as_uri()}?mode=ro", uri=True)
        try:
            (n,) = con.execute(
                "SELECT COUNT(*) FROM items WHERE included=1 AND content_key NOT IN "
                "(SELECT content_key FROM publications WHERE base=?)", (base,)).fetchone()
        finally:
            con.close()
    except sqlite3.Error:
        return None
    return int(n)


def report() -> tuple[bool, list[str]]:
    """``(stale, lines)``. Every line says what to run when it is not current."""
    lines: list[str] = []
    stale = False

    roster_stale, why = pack_manifest.check_stale()
    if roster_stale:
        stale = True
        lines.append(f"STALE roster: {why}\n"
                     f"      fix: python -m emojikit.pack_manifest --refresh")
    else:
        lines.append(f"ok    roster: {why}")

    try:
        archive_stale, reasons = pack_archive.check()
    except OperatorConfigMissing:
        # Unset is unknown, not "current": say which it is.
        lines.append("skip  archive: EMOJI_ARCHIVE_DIR is not set, so it was not checked")
    else:
        if archive_stale:
            stale = True
            lines.append("STALE archive: " + "; ".join(reasons) +
                         "\n      fix: python -m emojikit.pack_archive --sync")
        else:
            lines.append("ok    archive: every full pack is archived")

    base = operator_config.value("COLLECTION_PACK_BASE")
    pending = _unpublished(base) if base else None
    if pending is None:
        lines.append("skip  catalog: no readable catalog or COLLECTION_PACK_BASE unset")
    elif pending:
        # Waiting to be published is a to-do, not staleness.
        lines.append(f"info  catalog: {pending} included emoji not yet published to "
                     f"{base}\n      publish: python -m emojikit.build_collection "
                     f"--base {base} --title ... --dry-run")
    else:
        lines.append(f"ok    catalog: every included emoji is published to {base}")
    return stale, lines


def main(argv: list[str] | None = None) -> int:
    argparse.ArgumentParser(description=__doc__.split("\n")[0]).parse_args(argv)
    load_env()
    setup_logging("status")
    stale, lines = report()
    print("\n".join(lines))
    return EXIT_STALE if stale else EXIT_OK


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
