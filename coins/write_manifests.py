"""Write a per-pack manifest (.md) for every coin emoji pack.

For each of the live sets, lists every sticker in order with the ticker(s) that
map to it and its custom_emoji_id. One Markdown file per pack plus a combined
index, written under ``<out-dir>/manifests/``.

Usage:
  python coins/write_manifests.py --out-dir "<your coin archive folder>"
"""

from __future__ import annotations

import os as _os
import sys as _sys
_sys.path.insert(0, _os.path.dirname(_os.path.dirname(_os.path.abspath(__file__))))

import argparse
import json
import logging
import os
from collections import defaultdict
from pathlib import Path

from emojikit.build_pack import (load_env)
from emojikit.telegram_api import (Telegram)
from emojikit.logsetup import register_secret, setup_logging
from emojikit.pack_rows import markdown_table
from coins._env import require_token

ROOT = Path(__file__).resolve().parent
log = logging.getLogger("write_manifests")


def render_pack_md(s: dict, sticks: list[dict], cid_to_tickers: dict) -> str:
    """One coin pack's manifest: position, the ticker(s) on each id, the id."""
    rows = []
    for pos, st in enumerate(sticks, 1):
        cid = str(st.get("custom_emoji_id"))
        rows.append({"n": pos, "cid": cid,
                     "tickers": ", ".join(sorted(cid_to_tickers.get(cid, []))) or "(unmapped)"})
    head = [f"# {s.get('title', s['name'])}", "",
            f"Pack: https://t.me/addemoji/{s['name']}  |  {len(sticks)} stickers", ""]
    table = markdown_table(rows, [("n", "#"), ("tickers", "Ticker(s)"), ("cid", "Emoji ID")])
    return "\n".join(head) + "\n" + table + "\n"


def main() -> int:
    load_env()
    setup_logging("write_manifests")
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", default=str(ROOT), help="Where to create manifests/.")
    ap.add_argument("--token-env", default="TELEGRAM_BOT_TOKEN")
    ap.add_argument("--state", default=str(ROOT / "rebuild_dedup_state.json"))
    ap.add_argument("--map", default=str(ROOT / "ticker_to_id.json"))
    args = ap.parse_args()
    # --token-env can name any variable, not only the ones logsetup masks by name.
    register_secret(os.environ.get(args.token_env))

    tg = Telegram(require_token(args.token_env))
    sets = sorted(json.loads(Path(args.state).read_text(encoding="utf-8"))["sets"],
                  key=lambda s: s["index"])
    t2id = json.loads(Path(args.map).read_text(encoding="utf-8"))
    cid_to_tickers: dict[str, list[str]] = defaultdict(list)
    for t, c in t2id.items():
        cid_to_tickers[str(c)].append(t)

    md_dir = Path(args.out_dir) / "manifests"
    md_dir.mkdir(parents=True, exist_ok=True)
    index = ["# Coin Emoji Packs - Manifest Index", "",
             "| Pack | Stickers | Link |", "|------|----------|------|"]
    grand = 0
    for s in sets:
        sticks = tg.get_sticker_set(s["name"]).get("stickers", [])
        grand += len(sticks)
        (md_dir / f"{s['name']}.md").write_text(render_pack_md(s, sticks, cid_to_tickers),
                                                encoding="utf-8")
        index.append(f"| {s['name']} | {len(sticks)} | https://t.me/addemoji/{s['name']} |")
        log.info("wrote manifests/%s.md (%d stickers)", s["name"], len(sticks))
    index.append("")
    index.append(f"Total: {len(sets)} packs, {grand} stickers.")
    (md_dir / "INDEX.md").write_text("\n".join(index) + "\n", encoding="utf-8")
    log.info("wrote manifests/INDEX.md | %d packs, %d stickers", len(sets), grand)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
