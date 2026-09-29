# Design: applying the panel's pack plan

Status: **built 2026-09-29** as `python -m emojikit.plan_apply` (usage: GUIDE, "The move
plan"). Decided the same day — the owner answered the three questions (see
"Owner decisions" at the end). The applier is built on this design, dry run by
default; `--apply` is the only path that changes a live pack.

## The gap

The Curate panel is an intent editor. Save writes `<data-dir>/pack_plan.json`:
`targets` (authoritative `[content_key, pack]`), `moves`
(`{key, label, from_pack, to_pack}`), `held` (`{key, label, from_pack}`),
`counts`, `logo_slots`, `over_capacity`. The GUIDE promises that the real
rearrangement is "a separate, deliberate step" read from that file. Nothing
reads it: `build_collection` fills packs in panel order (plan 023) and
`sync_order` reorders inside one set, but no code removes a sticker from a set
or moves one between sets.

## Why a move is expensive

The Bot API has no move-between-sets call. Moving an emoji is
`deleteStickerFromSet` on the old set and `addStickerToSet` on the new one, and
the add mints a **new `custom_emoji_id`**. Every message, inventory or external
map that used the old id shows a placeholder from then on. `setStickerPositionInSet`
(reordering inside a set) keeps the id; only cross-pack moves and removals
retire one. The roster already records retired ids (`by_previous_id` in
`packs/index.json`), so a stale reference can still be resolved afterwards.

## What the report shows today

`plan_status` reads the plan, the publisher state (`publish_<base>.json`) and,
read-only, the catalog. Per pack: target count plus the logo slot against the
cap, emoji moving in and out, held emoji still live in the pack (they would have
to be removed), and never-published candidates aimed at it; then every
`custom_emoji_id` a move or removal would retire. `--live` confirms those ids
with `getStickerSet`. Exit 3 when anything is pending, so it can feed
`emojikit.status` later.

## Proposed applier (for review)

### Order of operations: make room first

A full pack (200, logo included) cannot take an arrival, and Telegram refuses
the add rather than queueing it. So per run:

1. **Removals** — the `from` half of each move. Each frees a slot and retires
   an id. A held emoji that is still live is NOT removed: it stays in its pack
   until the owner places it somewhere (owner decision 2).
2. **Adds** — the `to` half of each move, then candidates, in the panel's
   current order (the same `_by_position` rule the publisher uses).
3. **Reorder** — `sync_order` per touched pack, so positions match the panel.
   Ids survive this step.

Adding first would need spare capacity that full packs do not have, and a run
interrupted between an add and its delete would leave the same emoji live
twice.

### Records per item

The publisher's resume state and the catalog's `publications` must describe the
live packs after every single mutation, not at the end:

- removal: drop the key from `sets[i].keys`, decrement `live`, delete the
  `publications` row for `(base, key)`; record the retired id in the roster's
  history on the next `pack_manifest --refresh`.
- add: through the existing verified publisher path — `reconcile_set`, the
  per-item committed `uploaded` flag, the recorded upload order, the
  verified-retry path. Never a second add mechanism.

### A journal, reusing the migration pattern

`emojikit/collection_migrate.py` already has the shape: a journal written before
each stage (`backup` → `database` → `state` → `files` → `verified`), every stage
idempotent, a WAL-safe backup, and the pack-family lock held for the run. The
applier journals each planned mutation with its intent (`remove`/`add`, key,
set, expected live count) before sending it, exactly like the in-flight intent
the publisher already keeps. An ambiguous outcome (network drop after send) is
settled by reading the live set by content, never by retrying blindly.

### Flood-wait budget

Telegram rate-limits set edits (`retry_after`). The client already honours a
flood wait; the applier adds a per-run cap on mutations so one run stays within
a predictable time, and stops cleanly (journal intact, exit 3) when the cap or a
long flood wait is reached. The next run resumes from the journal.

### Idempotence and resume

- A removal whose sticker is already gone is done.
- An add is decided by content against the live set (`reconcile_set`), so a
  resumed run neither duplicates nor loses it.
- The plan is re-read on every run; an item whose live pack already matches its
  target is skipped. Running the applier twice changes nothing the second time.

### Refusals

- `over_capacity` non-empty: refuse before any mutation.
- The plan names a key the catalog no longer has: refuse (the panel drops these
  on save; an old plan file may still carry them).
- Unknown live state for any touched set: stop, as every writer here does.

## Owner decisions (2026-09-29)

1. **A cross-pack move is allowed** even though the emoji gets a new id and the
   old id stops working in messages that used it. The roster keeps the retired
   id (`by_previous_id`) so an old reference can still be traced.
2. **A held emoji that is already live stays live** until it is placed in a
   pack; the applier never removes it on its own.
3. **At most 20 Telegram changes per run** (removals and adds together;
   reordering inside a pack keeps ids and is not counted). A run that reaches
   the cap stops cleanly with exit 3 and the next run continues from the
   journal.
