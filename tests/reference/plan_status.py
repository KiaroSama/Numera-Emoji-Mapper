"""What applying the panel's saved pack plan would do. Read-only; changes nothing.

    python -m emojikit.plan_status [--data-dir collection] [--base NAME] [--live]

The Curate panel writes the owner's intended layout to `<data-dir>/pack_plan.json`
and nothing reads it to act: a move between packs is a delete plus a re-add
that mints a NEW custom_emoji_id, so the first step is to show exactly what
would change. Per pack: its target count and logo slot against the cap, emoji
moving in and out, held emoji still live in it (they would have to be removed),
and never-published candidates aimed at it. With `--live` each set is read
(getStickerSet, read-only) to confirm which ids a move would retire.

Exit 0 when nothing is pending, 3 when the plan asks for work. The design for
actually applying it is docs/design/plan-applier.md, and `python -m
emojikit.plan_apply` does it (dry run unless --apply).
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
from pathlib import Path

from emojikit import operator_config
from emojikit.cli_env import EXIT_OK, EXIT_USAGE, load_env
from emojikit.logsetup import record_exit_code, redact, setup_logging
from .panel_plan import PlanError, read_plan, target_map

ROOT = Path(__file__).resolve().parents[2]
EXIT_PENDING = 3


def _live_pack_of(state: dict) -> dict[str, int]:
    """content_key -> the pack number the publisher recorded it in."""
    return {key: int(s["index"]) for s in state.get("sets", []) for key in s.get("keys") or []}


def _catalog_ids(data_dir: Path, base: str) -> dict[str, str]:
    """content_key -> its custom_emoji_id in ``base``, read-only (no migrations)."""
    db = data_dir / "catalog.db"
    if not db.is_file():
        return {}
    try:
        con = sqlite3.connect(f"{db.resolve().as_uri()}?mode=ro", uri=True)
        try:
            return {k: str(c) for k, c in con.execute(
                "SELECT content_key, custom_emoji_id FROM publications "
                "WHERE base=? AND custom_emoji_id IS NOT NULL", (base,))}
        finally:
            con.close()
    except sqlite3.Error:
        return {}


def summarize(plan: dict, state: dict) -> dict:
    """Everything the report prints, as data (what the tests check)."""
    per_set = int(plan.get("per_set") or 200)
    live = _live_pack_of(state)
    targets = target_map(plan)
    logos = {int(n): int(v) for n, v in (plan.get("logo_slots") or {}).items()}
    counts = {int(n): int(v) for n, v in (plan.get("counts") or {}).items()}
    packs: dict[int, dict] = {}

    def pack(n: int) -> dict:
        return packs.setdefault(n, {"target": counts.get(n, 0), "logo": logos.get(n, 0),
                                    "cap": per_set, "move_in": [], "move_out": [],
                                    "held_live": [], "candidates": []})

    for row in plan.get("moves") or []:
        pack(int(row["to_pack"]))["move_in"].append(row["key"])
        pack(int(row["from_pack"]))["move_out"].append(row["key"])
    for row in plan.get("held") or []:
        if row["key"] in live:
            pack(live[row["key"]])["held_live"].append(row["key"])
    for key, n in targets.items():
        if key not in live and key not in set(plan.get("excluded") or []):
            pack(int(n))["candidates"].append(key)
    for n in counts:
        pack(n)
    pending = any(p["move_in"] or p["move_out"] or p["held_live"] or p["candidates"]
                  or p["target"] + p["logo"] > p["cap"] for p in packs.values())
    return {"packs": dict(sorted(packs.items())), "pending": pending}


def render(summary: dict, ids: dict[str, str], live_ids: set[str] | None) -> str:
    lines = ["pack  target+logo/cap  in  out  held-live  candidates"]
    retire: list[str] = []
    for n, p in summary["packs"].items():
        over = "  OVER CAPACITY" if p["target"] + p["logo"] > p["cap"] else ""
        lines.append(f"{n:>4}  {p['target']:>6}+{p['logo']}/{p['cap']:<5} {len(p['move_in']):>3} "
                     f"{len(p['move_out']):>4} {len(p['held_live']):>10} "
                     f"{len(p['candidates']):>11}{over}")
        retire += p["move_out"] + p["held_live"]
    if retire:
        lines.append("")
        lines.append("custom_emoji_ids a move or removal would RETIRE "
                     "(a re-add mints a new id; the old one stops working):")
        for key in retire:
            cid = ids.get(key, "?")
            note = "" if live_ids is None else ("" if cid in live_ids else "  (not live now)")
            lines.append(f"  {cid}  {key}{note}")
    lines.append("")
    lines.append("PENDING: the plan asks for work (see docs/design/plan-applier.md)"
                 if summary["pending"] else "nothing pending: the live packs match the plan")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--data-dir", default="collection", help="Catalog/plan directory.")
    ap.add_argument("--base", default="", help="Pack family (default COLLECTION_PACK_BASE).")
    ap.add_argument("--live", action="store_true",
                    help="also read each set (getStickerSet, read-only) to confirm the ids")
    args = ap.parse_args(argv)
    load_env()
    setup_logging("plan_status")
    data_dir = Path(args.data_dir)
    if not data_dir.is_absolute():
        data_dir = ROOT / data_dir
    base = args.base or operator_config.value("COLLECTION_PACK_BASE")
    if not base:
        print("ERROR: give --base or set COLLECTION_PACK_BASE.")
        return EXIT_USAGE
    try:
        plan = read_plan(data_dir)
    except PlanError as exc:
        print(f"ERROR: {exc}")
        return EXIT_USAGE
    if plan is None:
        print(f"no {data_dir / 'pack_plan.json'}: the panel has saved no plan; nothing pending.")
        return EXIT_OK
    state_file = data_dir / f"publish_{base}.json"
    state = (json.loads(state_file.read_text(encoding="utf-8"))
             if state_file.is_file() else {})
    summary = summarize(plan, state)
    live_ids = None
    if args.live:
        from emojikit.telegram_api import Telegram
        token = os.environ.get("GENERAL_BOT_TOKEN", "")
        if not token:
            print("ERROR: --live needs GENERAL_BOT_TOKEN.")
            return EXIT_USAGE
        tg = Telegram(token)
        live_ids = set()
        for s in state.get("sets", []):
            try:     # getStickerSet only: this command writes nothing, anywhere
                live_ids |= {str(st.get("custom_emoji_id"))
                             for st in tg.get_sticker_set(s["name"]).get("stickers", [])}
            except Exception as exc:  # noqa: BLE001 - reported; the offline report still stands
                print(f"warning: cannot read {s['name']}: {redact(str(exc))}")
    print(render(summary, _catalog_ids(data_dir, base), live_ids))
    return EXIT_PENDING if summary["pending"] else EXIT_OK


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
