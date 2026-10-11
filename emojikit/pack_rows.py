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
    import json
    from emojikit import _native
    # Convert cells at the language boundary; Rust owns escaping and table structure.
    keys = {column[0] for column in columns}
    values = [{key: "" if row.get(key) is None else str(row.get(key)) for key in keys} for row in rows]
    return _native.markdown_table_json(json.dumps(values, ensure_ascii=False),
        [(column[0], column[1], len(column) > 2 and column[2] == "right") for column in columns])


def keyword_cell(keywords: list[str]) -> str:
    """Every keyword, comma-separated -- never a truncated subset."""
    return ", ".join(k for k in keywords or [] if k)
