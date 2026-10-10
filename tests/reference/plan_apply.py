"""Apply the Curate panel's saved pack plan to the live packs of one --mixed family.

    python -m emojikit.plan_apply [--data-dir collection] [--base NAME]
        [--token-env GENERAL_BOT_TOKEN] [--user-id N] [--apply] [--max-changes 20]

Without ``--apply`` this is a dry run: it prints, pack by pack, what applying
the plan would remove, add and reorder, which custom_emoji_ids would be retired
and how many runs that takes, and writes nothing anywhere.

With ``--apply``, per run: removals first (the outgoing half of each move --
never a held emoji, which stays live until the owner places it), then adds in
the panel's order through the publisher's own verified path, then a reorder of
every pack whose order differs from the panel's. The Bot API has no move call,
so a move is a delete plus a re-add and the emoji gets a NEW custom_emoji_id;
the old one is kept in the journal's ``retired`` list. At most 20 counted
changes (removals and adds) per run -- owner decisions of 2026-09-29, see
docs/design/plan-applier.md. Reordering keeps ids and is not counted.

Exit codes: 0 nothing pending, 3 work remains (dry run with work, or the cap was
reached), 2 usage error or a refusal before any change, 1 failure mid-run.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import logging
import os
import re
import sqlite3
from dataclasses import dataclass, field
from pathlib import Path

from emojikit import operator_config
from . import build_collection, sync_order
from .build_pack import (EXIT_FAILED, EXIT_OK, EXIT_USAGE, load_env,
                                 safe_int_env)
from .catalog import Catalog
from .collection_names import check_name_length
from .collection_reconcile import _probe
from .collection_state import (DEFAULT_EMOJI, MIXED, PER_SET, BrandLogo,
                                       SetDrift, StateError, _lock_path, _state_path,
                                       load_json, load_state, save_json)
from emojikit.errors import FloodWaitTooLong, OperatorConfigMissing
from emojikit.logsetup import record_exit_code, redact, register_secret, setup_logging
from emojikit.maintenance import writer
from emojikit.packstate import LockBusy, exclusive_lock
from .panel_plan import PlanError, read_plan, target_map
from .plan_status import EXIT_PENDING
from emojikit.telegram_api import BotApiError, LiveStateUnknown, SetState, Telegram

ROOT = Path(__file__).resolve().parents[2]
MAX_CHANGES = 20          # owner decision 3: counted changes per run
MAX_WAIT = 300            # longest Telegram flood wait (s) one run sleeps out
log = logging.getLogger("plan_apply")


class Refusal(Exception):
    """The plan cannot be applied as it stands. Raised before any change."""


@dataclass
class Steps:
    """What one run would do; recomputed every run, never stored as truth."""
    removals: list[tuple[str, int]] = field(default_factory=list)   # (key, from pack)
    adds: dict[int, list[str]] = field(default_factory=dict)        # pack -> keys, panel order
    moved_in: set[str] = field(default_factory=set)                 # adds that are moves
    create: int | None = None                                       # the one new pack
    create_logo: bool = False
    reorder: list[int] = field(default_factory=list)
    held_live: list[tuple[str, int]] = field(default_factory=list)
    unpublishable: list[str] = field(default_factory=list)          # publisher skipped them

    @property
    def counted(self) -> int:
        return (len(self.removals) + sum(map(len, self.adds.values()))
                + (1 if self.create is not None and self.create_logo else 0))

    @property
    def pending(self) -> bool:
        return bool(self.counted or self.reorder)


def read_catalog(db: Path, base: str) -> tuple[dict[str, dict], dict[str, str]]:
    """(items, ids) read-only: key -> {included, pos}; key -> custom_emoji_id in ``base``.

    Read-only on purpose: opening a ``Catalog`` migrates and switches the
    journal mode, and a dry run must not write a byte.
    """
    try:
        con = sqlite3.connect(f"{db.resolve().as_uri()}?mode=ro", uri=True)
        try:
            items = {k: {"included": bool(inc), "pos": i} for i, (k, inc) in enumerate(
                con.execute("SELECT content_key, included FROM items "
                            "ORDER BY position, content_key"))}
            ids = {k: str(c) for k, c in con.execute(
                "SELECT content_key, custom_emoji_id FROM publications WHERE base=?",
                (base,)) if c}
        finally:
            con.close()
    except sqlite3.Error as exc:
        raise Refusal(f"cannot read {db}: {exc}") from exc
    return items, ids


def compute_steps(plan: dict, state: dict, items: dict[str, dict],
                  ids: dict[str, str]) -> Steps:
    """Diff the plan's targets against what the publisher recorded as live."""
    sets = sorted(state.get("sets") or [], key=lambda s: int(s["index"]))
    if any(s.get("fmt") != MIXED for s in sets):
        raise Refusal("this family was published per format; the applier only handles "
                      "--mixed families, where one pack number means one set")
    if plan.get("over_capacity"):
        raise Refusal(f"the plan overfills pack(s) {sorted(plan['over_capacity'])}; "
                      f"fix it in the panel and save again")
    per_set = min(int(plan.get("per_set") or PER_SET), PER_SET)
    by_index = {int(s["index"]): s for s in sets}
    live = {k: int(s["index"]) for s in sets for k in s.get("keys") or []}
    excluded = set(plan.get("excluded") or []) | {r["key"] for r in plan.get("held") or []}
    skipped = set(state.get("skipped") or [])
    pos = {k: v["pos"] for k, v in items.items()}
    steps = Steps()

    def order(keys):
        return sorted(keys, key=lambda k: pos.get(k, len(pos)))

    for key, n in target_map(plan).items():
        if key in excluded:
            continue
        if key not in items:
            raise Refusal(f"the plan names {key}, which the catalog no longer holds; "
                          f"save the plan again from the panel")
        if not items[key]["included"]:
            raise Refusal(f"the plan puts {key} in pack {n} but the catalog has it "
                          f"excluded; save the plan again from the panel")
        src = live.get(key)
        if src == n:
            continue
        if src is None and key in skipped:
            steps.unpublishable.append(key)      # the publisher refuses its file
            continue
        if src is not None:
            if not ids.get(key):
                raise Refusal(f"{key} is live in pack {src} but its custom_emoji_id is "
                              f"not recorded, so its sticker cannot be found by id")
            steps.removals.append((key, src))
            steps.moved_in.add(key)
        steps.adds.setdefault(int(n), []).append(key)

    top = max(by_index, default=0)
    new = sorted(n for n in steps.adds if n not in by_index)
    if new:
        if new != [top + 1]:
            raise Refusal(f"the plan targets pack(s) {new}, but only pack {top + 1} can be "
                          f"created next (packs are numbered without gaps)")
        if not sets:
            raise Refusal("this family has no pack yet; start it with "
                          "build_collection --mixed, then apply the plan")
        steps.create = top + 1
        slots = plan.get("logo_slots") or {}
        steps.create_logo = bool(slots.get(str(top + 1), sets[-1].get("logo")))

    steps.adds = {n: order(keys) for n, keys in sorted(steps.adds.items())}
    out = {k for k, _ in steps.removals}
    for n in sorted(set(by_index) | set(steps.adds)):
        rec = by_index.get(n)
        stay = [k for k in (rec.get("keys") or []) if k not in out] if rec else []
        logo = bool(rec.get("logo")) if rec else steps.create_logo
        final = stay + steps.adds.get(n, [])   # the publisher appends at the end
        # Held-but-live emoji are in `stay`: they keep their slot (owner decision 2).
        if logo + len(final) > per_set:
            raise Refusal(f"pack {n} would hold {logo + len(final)} (logo included), "
                          f"over the cap of {per_set}")
        if rec and final != order(final):
            steps.reorder.append(n)
    steps.removals.sort(key=lambda r: (r[1], pos.get(r[0], len(pos))))
    steps.held_live = sorted(((k, live[k]) for k in excluded if k in live),
                             key=lambda r: (r[1], pos.get(r[0], len(pos))))
    return steps


def render(steps: Steps, ids: dict[str, str], max_changes: int) -> str:
    """The dry-run listing (also printed before an --apply run)."""
    lines = []
    packs = sorted({n for _, n in steps.removals} | set(steps.adds) | set(steps.reorder)
                   | {n for _, n in steps.held_live})
    for n in packs:
        outs = [k for k, src in steps.removals if src == n]
        ins = steps.adds.get(n, [])
        moved = sum(1 for k in ins if k in steps.moved_in)
        head = f"pack {n}" + ("  (NEW: created by this plan)" if n == steps.create else "")
        lines.append(head)
        if outs:
            lines.append(f"  remove {len(outs)}: " + ", ".join(outs))
        if ins:
            lines.append(f"  add {len(ins)} ({moved} moved in, {len(ins) - moved} new): "
                         + ", ".join(ins))
        if n in steps.reorder:
            lines.append("  reorder to the panel's order (ids kept)")
        held = [k for k, src in steps.held_live if src == n]
        if held:
            lines.append(f"  held, stays live until placed: {', '.join(held)}")
    if steps.removals:
        lines += ["", "custom_emoji_ids these moves RETIRE (the re-add mints a new id):"]
        lines += [f"  {ids.get(k, '?')}  {k}" for k, _ in steps.removals]
    if steps.unpublishable:
        lines += ["", "the publisher refuses these files (state 'skipped'); not counted: "
                  + ", ".join(steps.unpublishable)]
    runs = -(-steps.counted // max_changes)
    lines += ["", f"{steps.counted} counted change(s): {runs} run(s) at {max_changes} per run"
              + (", then a reorder" if steps.reorder else "")
              if steps.pending else "nothing pending: the live packs match the plan"]
    return "\n".join(lines)


# --------------------------------------------------------------------------- #
# Removals: the only mutation this module makes itself.
# --------------------------------------------------------------------------- #
def _journal_path(data_dir: Path, base: str) -> Path:
    return data_dir / f"plan_apply_{base}.json"


def load_journal(data_dir: Path, base: str) -> dict:
    """Unreadable is a stop (StateError), never "no journal": it holds the intent."""
    j = load_json(_journal_path(data_dir, base),
                  {"version": 1, "base": base, "intent": None, "retired": []})
    if not isinstance(j, dict) or j.get("base") != base or not isinstance(j.get("retired"), list):
        raise StateError(f"{_journal_path(data_dir, base)} is not this family's apply journal")
    return j


def _now() -> str:
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _live_stickers(tg, name: str) -> list[dict]:
    st, sset = _probe(tg, name)
    if st is SetState.UNKNOWN:
        raise LiveStateUnknown(f"live state of {name} is unknown; not touching it")
    if st is SetState.MISSING:
        raise SetDrift(f"{name} no longer exists on Telegram")
    return sset.get("stickers", [])


def _finish_removal(cat: Catalog, state: dict, journal: dict, *, data_dir: Path,
                    base: str, key: str, cid: str, set_name: str) -> None:
    """Make the records say what Telegram already did. Idempotent (resume)."""
    rec = next(s for s in state["sets"] if s["name"] == set_name)
    if key in (rec.get("keys") or []):
        rec["keys"].remove(key)
        rec["live"] = max(0, int(rec.get("live", 0)) - 1)
    save_json(_state_path(data_dir, base), state)
    cat.unpublish(base, key)
    if not any(r.get("cid") == cid for r in journal["retired"]):
        journal["retired"].append({"key": key, "cid": cid, "from_set": set_name, "at": _now()})
    journal["intent"] = None
    save_json(_journal_path(data_dir, base), journal)
    log.info("removed %s (retired id %s) from %s", key, cid, set_name)


def settle_intent(tg, cat: Catalog, state: dict, journal: dict, *,
                  data_dir: Path, base: str) -> None:
    """A delete sent by a run that died before recording it: read, never re-send."""
    intent = journal.get("intent")
    if not intent:
        return
    if not any(s["name"] == intent["set"] for s in state["sets"]):
        raise SetDrift(f"the journal names {intent['set']}, which this family does not record")
    live = _live_stickers(tg, intent["set"])
    if any(str(st.get("custom_emoji_id")) == intent["cid"] for st in live):
        log.info("pending removal of %s was not applied; it will be planned again",
                 intent["key"])
        journal["intent"] = None
        save_json(_journal_path(data_dir, base), journal)
        return
    _finish_removal(cat, state, journal, data_dir=data_dir, base=base,
                    key=intent["key"], cid=intent["cid"], set_name=intent["set"])


def remove_one(tg, cat: Catalog, state: dict, journal: dict, *, data_dir: Path,
               base: str, key: str, pack: int) -> None:
    rec = next(s for s in state["sets"] if int(s["index"]) == pack)
    name, cid = rec["name"], cat.custom_emoji_id_for(base, key)
    live = _live_stickers(tg, name)
    at = next((i for i, st in enumerate(live) if str(st.get("custom_emoji_id")) == cid), None)
    keys = rec.get("keys") or []
    offset = 1 if rec.get("logo") else 0
    # By id, and the recorded position must agree: records that disagree with
    # the live set are drift, and deleting on drift is how the wrong emoji goes.
    if at is None or key not in keys or at != offset + keys.index(key):
        raise SetDrift(f"{name} does not hold {key} ({cid}) where this family recorded "
                       f"it; reconcile the pack before applying the plan")
    journal["intent"] = {"op": "remove", "key": key, "set": name, "cid": cid, "at": _now()}
    save_json(_journal_path(data_dir, base), journal)

    def gone() -> bool | None:     # the verified-retry probe: read, never guess
        st, sset = _probe(tg, name)
        if st is not SetState.EXISTS:
            return None
        return all(str(s.get("custom_emoji_id")) != cid for s in sset.get("stickers", []))

    try:
        tg.call("deleteStickerFromSet", data={"sticker": live[at]["file_id"]},
                applied_check=gone)
    except (BotApiError, FloodWaitTooLong):
        journal["intent"] = None          # a definite refusal: nothing happened
        save_json(_journal_path(data_dir, base), journal)
        raise
    if gone() is not True:
        raise LiveStateUnknown(f"deleted {key} from {name}, but a re-read does not "
                               f"confirm it; the next run settles it from the live set")
    _finish_removal(cat, state, journal, data_dir=data_dir, base=base,
                    key=key, cid=cid, set_name=name)


# --------------------------------------------------------------------------- #
# One --apply run
# --------------------------------------------------------------------------- #
def apply_run(tg, cat: Catalog, *, plan: dict, state: dict, data_dir: Path, base: str,
              user_id: int, bot: str, logo_bots: frozenset[str], logo_path: str | None,
              max_changes: int) -> int:
    """Removals, adds, reorder, within the cap. Returns the exit code."""
    try:
        return _run(tg, cat, plan=plan, state=state, data_dir=data_dir, base=base,
                    user_id=user_id, bot=bot, logo_bots=logo_bots, logo_path=logo_path,
                    max_changes=max_changes)
    except FloodWaitTooLong as exc:
        # Refused, so nothing half-applied: every change before it is already
        # recorded item by item. Saved here because the publisher writes its
        # state only every 20 items and at the end, which it never reached.
        save_json(_state_path(data_dir, base), state)
        print(f"{exc}; stopped cleanly. Run again after that.")
        log.warning("stopped on a flood wait: %s", exc)
        return EXIT_PENDING


def _run(tg, cat: Catalog, *, plan: dict, state: dict, data_dir: Path, base: str,
         user_id: int, bot: str, logo_bots: frozenset[str], logo_path: str | None,
         max_changes: int) -> int:
    journal = load_journal(data_dir, base)
    settle_intent(tg, cat, state, journal, data_dir=data_dir, base=base)
    items, ids = read_catalog(cat.path, base)
    steps = compute_steps(plan, state, items, ids)
    per_set = min(int(plan.get("per_set") or PER_SET), PER_SET)
    budget = max_changes
    for key, pack in steps.removals[:budget]:
        remove_one(tg, cat, state, journal, data_dir=data_dir, base=base, key=key, pack=pack)
        budget -= 1
    # Budget left means every removal is done -- which is what guarantees the room.
    if budget:
        for pack, keys in steps.adds.items():
            creating = pack == steps.create
            logo = None
            if creating and bot.lower() in logo_bots:
                logo = BrandLogo(logo_path or "", data_dir)
                logo.static_png()         # prepared before the set exists, as the publisher does
            room = budget - (1 if creating and logo else 0)
            if room < 1:
                break
            if creating:
                check_name_length(base, bot, "")
            title = re.sub(r"\s+\d+$", "", state["sets"][0]["title"])
            done, failed = build_collection.publish_format(
                tg, cat, fmt=MIXED, plan_keys=keys[:room], base=base, title=title,
                user_id=user_id, default_emoji=DEFAULT_EMOJI, per_set=per_set,
                data_dir=data_dir, state=state, bot=bot, logo=logo,
                new_set=creating, into_pack=None if creating else pack)
            budget -= done + (1 if creating and logo else 0)
            if failed:
                log.error("%d add(s) into pack %d did not land; see the log", failed, pack)
                return EXIT_FAILED
            if not budget:
                break
    items, ids = read_catalog(cat.path, base)
    left = compute_steps(plan, state, items, ids)
    if left.counted:
        print(f"cap reached: {left.counted} counted change(s) remain; run again.")
        return EXIT_PENDING
    for rec in state["sets"]:
        if int(rec["index"]) in left.reorder:
            if sync_order.sync_set(tg, cat, rec, apply=True) < 0:
                save_json(_state_path(data_dir, base), state)
                return EXIT_FAILED
    save_json(_state_path(data_dir, base), state)
    print("done: the live packs match the plan.")
    return EXIT_OK


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--data-dir", default="collection", help="Catalog/plan directory.")
    ap.add_argument("--base", default="", help="Pack family (default COLLECTION_PACK_BASE).")
    ap.add_argument("--token-env", default="GENERAL_BOT_TOKEN")
    ap.add_argument("--user-id", type=int,
                    default=safe_int_env("PACK_OWNER_USER_ID", 0, minimum=0))
    ap.add_argument("--apply", action="store_true",
                    help="Change the live packs. Without it nothing is written anywhere.")
    ap.add_argument("--max-changes", type=int, default=MAX_CHANGES,
                    help=f"Counted changes (removals + adds) per run, 1..{MAX_CHANGES}.")
    ap.add_argument("--max-wait", type=int, default=MAX_WAIT,
                    help="Longest Telegram flood wait (s) to sleep out; a longer one ends "
                         "the run with exit 3 and the next run continues.")
    args = ap.parse_args(argv)
    load_env()
    setup_logging("plan_apply")
    register_secret(os.environ.get(args.token_env))
    if not 1 <= args.max_changes <= MAX_CHANGES:
        print(f"ERROR: --max-changes must be 1..{MAX_CHANGES} (the owner's cap).")
        return EXIT_USAGE
    data_dir = Path(args.data_dir)
    if not data_dir.is_absolute():
        data_dir = ROOT / data_dir
    base = args.base or operator_config.value("COLLECTION_PACK_BASE")
    db = data_dir / "catalog.db"
    if not base or not db.is_file():
        print("ERROR: give --base (or set COLLECTION_PACK_BASE) and a data dir with catalog.db.")
        return EXIT_USAGE
    try:
        plan = read_plan(data_dir)
        if plan is None:
            print(f"no {data_dir / 'pack_plan.json'}: the panel has saved no plan.")
            return EXIT_OK
        if not args.apply:
            items, ids = read_catalog(db, base)
            steps = compute_steps(plan, load_state(data_dir, base), items, ids)
            print(render(steps, ids, args.max_changes))
            return EXIT_PENDING if steps.pending else EXIT_OK
        return _apply(args, plan=plan, base=base, data_dir=data_dir, db=db)
    except (PlanError, StateError, Refusal, OperatorConfigMissing, LockBusy) as exc:
        print(f"REFUSED: {exc}. Nothing was changed.")
        log.error("refused: %s", exc)
        return EXIT_USAGE


def _apply(args, *, plan: dict, base: str, data_dir: Path, db: Path) -> int:
    token = os.environ.get(args.token_env, "")
    if not token or not args.user_id:
        print(f"ERROR: --apply needs {args.token_env} and --user-id (or PACK_OWNER_USER_ID).")
        return EXIT_USAGE
    logo_bots, logo_path = operator_config.brand_logo(False, None)
    # Same locks as the publisher and sync_order: two writers on one family would
    # read the same state and both act on it.
    with writer(data_dir), exclusive_lock(_lock_path(data_dir, base)):
        state = load_state(data_dir, base)
        items, ids = read_catalog(db, base)
        print(render(compute_steps(plan, state, items, ids), ids, args.max_changes))
        tg = Telegram(token)
        tg.max_flood_wait = max(0, args.max_wait)
        with Catalog(db) as cat:
            try:
                return apply_run(tg, cat, plan=plan, state=state, data_dir=data_dir,
                                 base=base, user_id=args.user_id, bot=tg.get_me()["username"],
                                 logo_bots=logo_bots, logo_path=logo_path,
                                 max_changes=args.max_changes)
            except (LiveStateUnknown, SetDrift, BotApiError, RuntimeError) as exc:
                print(f"STOPPED: {redact(str(exc))}. The records and the journal "
                      f"describe exactly what was done; run again to continue.")
                log.error("stopped: %s", redact(str(exc)))
                return EXIT_FAILED


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
