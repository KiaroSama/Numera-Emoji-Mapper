"""The one renderer for "the emoji in this pack" Markdown tables.

Four writers -- the publisher's manifest, the archive's `_manifest.md`, the
coin manifests and the `packs/` roster -- each carried their own copy of this
table and drifted: one cut the keywords to two, one did not escape `|` (so a
keyword containing it broke the table), one was never tested. Each writer still
chooses its own columns; only the rendering is shared.
"""

from __future__ import annotations

EMPTY = "—"


def _cell(value) -> str:
    text = "" if value is None else str(value)
    # A newline ends the row and `|` starts a new cell: either one turns a
    # label taken from a downloaded pack into a broken table.
    text = " ".join(text.splitlines()).replace("|", "\\|").strip()
    return text or EMPTY


def markdown_table(rows: list[dict], columns: list[tuple]) -> str:
    """Rows as a Markdown table. ``columns`` is ``[(key, title[, "right"]), ...]``."""
    head = "| " + " | ".join(c[1] for c in columns) + " |"
    rule = "|" + "|".join(("-" * max(len(c[1]), 1) + ":") if len(c) > 2 and c[2] == "right"
                          else "-" * (len(c[1]) + 2) for c in columns) + "|"
    body = ["| " + " | ".join(_cell(r.get(c[0])) for c in columns) + " |" for r in rows]
    return "\n".join([head, rule, *body])


def keyword_cell(keywords: list[str]) -> str:
    """Every keyword, comma-separated -- never a truncated subset."""
    return ", ".join(k for k in keywords or [] if k)
