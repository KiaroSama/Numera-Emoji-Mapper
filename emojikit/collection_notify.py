"""Announcing a published pack, and its per-pack manifest.

Split out of `build_collection.py` (file-size limit); it re-exports these names.
"""

from __future__ import annotations

import logging
from pathlib import Path

from emojikit.announce import announce_packs
from emojikit.catalog import Catalog
from emojikit.collection_state import _state_path, save_json
from emojikit.logsetup import redact
from emojikit.telegram_api import Telegram
from emojikit.pack_rows import keyword_cell, markdown_table

log = logging.getLogger("build_collection")


def notify(tg: Telegram, user_id: int, state: dict, data_dir: Path, base: str,
           name: str, title: str, *, full: bool = False) -> None:
    """Post a pack's add-link, at most once per milestone.

    TWO milestones, not one: the pack first going up, and the pack FILLING.
    ``sent`` alone conflated them, so a set announced while it was still
    being built stayed silent when it reached ``per_set`` -- and "the pack is
    finished" is the message the channel is actually waiting for. Pack 2 was
    announced at 96 emoji and said nothing at 200.
    """
    seen = state.setdefault("sent_full" if full else "sent", [])
    if name in seen:
        return
    # With a Worker deployed, the BOT posts the announcement and this process
    # never talks to the channel. `state["sent"]` still guards it, so which path
    # sent it does not change whether a re-run announces twice.
    try:
        dest = announce_packs(tg, user_id, [{"name": name, "title": title}],
                              bot="general")
        seen.append(name)
        save_json(_state_path(data_dir, base), state)
        log.info("sent link for %s to %s", name, dest)
    except Exception as exc:  # noqa: BLE001
        log.warning("notify failed for %s: %s", name, redact(str(exc)))


def write_manifest(data_dir: Path, cat: Catalog, s: dict, base: str) -> None:
    """Write a per-pack manifest: emoji name (keywords) + custom_emoji_id."""
    keys = s.get("keys") or []
    if not keys:
        return
    md = data_dir / "manifests"
    md.mkdir(parents=True, exist_ok=True)
    # The brand logo is the set's first sticker but not a catalog item, so
    # len(keys) is one short of what is actually in the pack. Reporting the
    # short number here made the manifest say 199 for a 200-emoji pack, which
    # is the internal row count and not what anyone opening this file wants.
    logo = 1 if s.get("logo") else 0
    rows = [{"n": 1, "name": "brand logo", "cid": ""}] if logo else []
    for i, key in enumerate(keys, 1 + logo):
        it = cat.get(key)
        # Every keyword: the list used to stop at two, so the manifest
        # disagreed with the other pack lists about the same emoji.
        name = keyword_cell(it.keywords) if it and it.keywords else (
            it.sources[0] if it and it.sources else key)
        cid = (cat.custom_emoji_id_for(base, key) if it else "") or ""
        rows.append({"n": i, "name": name, "cid": cid})
    head = [f"# {s.get('title', s['name'])}", "",
            f"Pack: https://t.me/addemoji/{s['name']}  |  format: {s['fmt']}"
            f"  |  {len(keys) + logo} emoji", ""]
    table = markdown_table(rows, [("n", "#"), ("name", "Name"), ("cid", "Emoji ID")])
    (md / f"{s['name']}.md").write_text("\n".join(head) + "\n" + table + "\n",
                                        encoding="utf-8")
    log.info("manifest written: manifests/%s.md (%d emoji)",
             s["name"], len(keys) + logo)
