"""The Curate panel's Save: validate it against the live catalog, then apply it.

Moved out of ``panel.py`` unchanged. A Save carries the tab's full inclusion
state, its scope (``known``) and, from newer pages, its pack targets; it is
checked and written under the panel's writer lock so a stale tab can never
speak for emoji it did not show.
"""

from __future__ import annotations

import json

from .catalog import Catalog
from .collection_state import PER_SET
from .panel_plan import merge_plan, read_plan, target_map, write_plan
from .panel_view import build_view


def replace_map(target: dict, fresh: dict) -> None:
    """Make ``target`` equal ``fresh`` without ever emptying it.

    Update first, prune second: a thumbnail request served between the two
    steps of clear()+update() found the map empty and answered 404, which the
    page showed as a broken image until the next reload.
    """
    target.update(fresh)
    for stale in [k for k in target if k not in fresh]:
        del target[stale]


def handle_save(payload: dict, *, lock, db_path, view: list, by_key: dict,
                hidden_now: list, bot_username: str, show_published: bool,
                keep_sets) -> tuple[int, bytes]:
    """Answer one ``/api/save`` as ``(status, body)``.

    ``PlanError``, ``sqlite3.Error``, ``LockBusy`` and ``OSError`` propagate:
    ``do_POST`` already maps each to its status.
    """
    if set(payload) - {"excluded", "known", "packs"}:
        return (400, b'{"error":"unknown keys"}')
    raw = payload.get("excluded", [])
    if not isinstance(raw, list) or not all(isinstance(k, str) for k in raw):
        return (400, b'{"error":"excluded must be a list of keys"}')
    # WHAT THE TAB COULD SEE. `excluded` is full-state -- every key
    # it does not name becomes included -- so without a scope a tab
    # speaks for emoji it has never heard of. A page opened before
    # `fetch_emoji_ids.py` added an item, or before the owner
    # deselected one in another tab, would silently re-include it
    # and answer {"ok": true}.
    # The panel's INTENDED pack per emoji. Optional: an older page
    # does not send it, and a save that carries no opinion must
    # still save the inclusion rather than be refused.
    targets_raw = payload.get("packs", [])
    if not isinstance(targets_raw, list) or not all(
            isinstance(p, list) and len(p) == 2
            and isinstance(p[0], str) and isinstance(p[1], int)
            and not isinstance(p[1], bool) and 1 <= p[1] <= 9007199254740991
            for p in targets_raw):
        return (400, b'{"error":"packs must be [key, pack] pairs"}')

    scope_raw = payload.get("known")
    if not isinstance(scope_raw, list) or not all(
            isinstance(k, str) for k in scope_raw):
        # An old page cannot tell us what it was showing, and there
        # is no safe way to guess. Refusing is recoverable with one
        # reload; guessing loses a selection nobody sees go.
        return (409, json.dumps({
            "error": "this page is from an older panel run and "
                     "cannot say which emoji it was showing; "
                     "reload the page and save again"}).encode())
    target_keys = [pair[0] for pair in targets_raw]
    if (len(target_keys) != len(set(target_keys))
            or not set(target_keys) <= set(scope_raw)):
        return (400, b'{"error":"pack targets must be unique and inside known scope"}')
    scope = set(scope_raw)
    excluded = set(raw)
    if not excluded <= scope:
        return (400, b'{"error":"excluded keys must be inside known scope"}')
    # Validate the WHOLE submitted decision under writer ownership.
    # Intersecting with a refreshed shared view can erase the very
    # key an old tab excluded, then falsely acknowledge its Save.
    with lock, Catalog(db_path) as cat:
        current = {it.content_key: it for it in cat.all_items()}
        fresh, fresh_by_key, fresh_hidden = build_view(
            cat, bot_username, show_published, keep_sets, seed_order=False)
        visible = {v["key"] for v in fresh if not v.get("isLogo")}
        if not scope <= current.keys() or not scope <= visible:
            return (409, b'{"error":"catalog identities or visibility changed; export the draft and reload"}')
        previous = read_plan(db_path.parent)
        staged = [dict(v, included=v["key"] not in excluded)
                  if v["key"] in scope else dict(v) for v in fresh]
        # Old clients may update inclusion, never erase pack intent.
        targets = (dict(targets_raw) if "packs" in payload else
                   {k: n for k, n in target_map(previous).items() if k in scope})
        # `current` comes from the database under this lease, so
        # the merge can drop decisions about emoji that are gone
        # instead of carrying them forever. The render view
        # cannot answer that: it hides published packs by design.
        plan = (merge_plan(previous, staged, targets, scope,
                           PER_SET, live=set(current))
                if "packs" in payload or previous is not None else None)
        # Outside the scope, carry the CURRENT state through.
        # set_inclusion re-includes every key it is not given,
        # so a grid that hides finished packs -- or a tab that
        # predates an ingest -- would otherwise re-include every
        # deselected item it cannot see.
        outside_excluded = {key for key, item in current.items()
                            if not item.included and key not in scope}
        inc, exc = cat.set_inclusion(excluded | outside_excluded)
        if plan is not None:
            # A failed write returns 503, never an acknowledgement.
            # The same scoped request is safe to retry under this lease.
            write_plan(db_path.parent, plan)
        view[:] = staged
        replace_map(by_key, fresh_by_key)
        hidden_now[0] = fresh_hidden
    body = {"ok": True, "included": inc, "excluded": exc}
    if plan is not None:
        body["moves"] = len(plan["moves"])
        body["held"] = len(plan["held"])
    return (200, json.dumps(body).encode())
