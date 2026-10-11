[Complete guide index](GUIDE.md)

## 20. The Curate panel internals (`panel`)

A `ThreadingHTTPServer` on `127.0.0.1`. Routes:

| Route | Response |
|-------|----------|
| `GET /` | The page (`assets/panel.html`: markup, CSS, items embedded as JSON, the per-run values). |
| `GET /img/<key>` | The media bytes (webp/png/webm) with correct MIME. |
| `GET /preview/<key>?fps=N&size=S` | Cached animated WebP for TGS; `still=1` returns a still for static/video/TGS. Sizes: 52, 72 or 104; rates: 1–30, bounded by `--preview-fps`. |
| `GET /static/<file>` | Static assets (logo, favicon, and the panel scripts, `emojikit.panel.SCRIPT_FILES`), traversal-guarded. The scripts are requested as `panel-grid.js?v=<hash>` — the hash is the scripts' content (`emojikit.panel.ASSET_VER`), because the route is immutable-cached and an edited script would otherwise be served stale. The logo is requested the same way, as `logo-128.png?v=<hash>` of its own bytes (`emojikit.panel.ICON_VER`), so a replaced logo is not served stale either. The query is stripped before the file lookup. |
| `GET /api/ping` | `{"ok":true}`. The page polls it to tell you when the panel process is gone; it touches neither the lock nor the catalog. |
| `GET /api/session` | Which catalog and view this panel serves (a hash of the catalog path, `all`, `packs`). A second launch on the same port reads it to reopen this panel instead of starting another. |
| `POST /api/save` | Body `{"excluded":[keys], "known":[keys], "packs":[[key, pack], …]}` → `catalog.set_inclusion(...)`, **restricted to `known`**; `packs` (optional) is the intended pack per emoji, written to `pack_plan.json`. |
| `POST /api/order` | Body `{"order":[keys]}` → `catalog.set_order(...)` (drag-to-reorder = publish order). |
| `POST /api/client-log` | Up to 32 whitelisted events, 16KiB per batch; counts/revisions/status/error type and location only. No token, label, media ID or arbitrary message fields. |

**Transport.** HTTP/1.1 keep-alive (the old HTTP/1.0 opened a connection and a
handler thread per thumbnail, and a browser allows six per host), a listen
backlog of 64 (the stdlib's 5 refused the page's own script loads when a
screenful of thumbnails arrived at once), and a 30 s idle timeout so a
kept-alive connection cannot hold its thread forever. Every response carries an
accurate `Content-Length`, except a 204, which carries none (RFC 9110).
Every response also carries `X-Content-Type-Options: nosniff` and
`Referrer-Policy: no-referrer`; the page adds `X-Frame-Options: DENY` and
`Content-Security-Policy: frame-ancestors 'none'`, because a page on another
origin could frame this loopback page and its requests would still pass the
Host check.

All POST routes are guarded: loopback-only `Host`/
`Origin`, a per-run token sent as `X-Panel-Token`, an exact-permutation check on
the order, a content-type check and a body cap (`tests/test_panel.py`).

**`known` is the save's scope, and it is mandatory.** A save carries the FULL
selection — every key it does not name becomes *included* — so a request with no
notion of scope speaks for the whole catalog. The panel re-reads the catalog on
every page load, so a tab opened before `fetch-emoji-ids` added an emoji, or
before the owner deselected one in a second tab, held a stale snapshot; saving
from it silently re-included rows it had never seen and answered `{"ok": true}`.
The request now states which keys it was showing, and the decision is applied
only inside that scope (intersected with what the catalog currently holds, so a
key that has since vanished is harmless). A page too old to say gets **409** and
a "reload the page and save again" message: one reload costs a second, while a
silent re-inclusion is invisible until a publish ships the wrong pack.
`tests/test_panel_save_scope.py` drives all of it through the real handler.

Ordering: `emojikit.panel_view.order_by_similarity` groups items by format (static, then
video, then animated) and within each runs a greedy nearest-neighbour walk on
the perceptual hash so look-alikes are adjacent. Items without a hash (animated)
keep content order. The view model lives in `emojikit/panel_view.py` — `build_view`,
`order_by_similarity`, `packs_named`, `copy_id_for` — with `panel` left
holding the server. Pure functions on one side, sockets on the other.

Front-end:

- Dark OLED theme, neon-blue accent (`#22d3ee`), Inter font, big 108-px cards
  with a label and a format badge. Thumbnails sit on a **dark-slate contrast
  checkerboard** (`#828c9a`/`#464e5a`, matching the dark theme) so black,
  hollow-center, and faint/low-opacity emoji are all visible by
  default, plus a **Backdrop switch** (Checker → Light → Dark → Gray, persisted
  in `localStorage`) to inspect tricky emoji on any background.
- All selected by default. Click toggles; **Shift+click** toggles a range.
  Header buttons: Undo / Redo / Reset all / Import draft / ↑ Top / ↓ Bottom /
  zoom / Select all / Deselect all / Invert / Backdrop / Animation /
  All visible / Real video / Selection mode / → Hold / Save selection.
  Select all / Deselect all / Invert change inclusion in one pass over the list
  (`setIncludedMany`), and holding a picked group computes every origin in one
  pass (`originsFor`), instead of recomputing the pack runs per item (for
  Deselect all on a thousand cards, ~60 ms became a few). The two jump buttons scroll the
  **document**, not `scrollIntoView`: that aligns an element with the top of
  the viewport, which sits behind the sticky header, so Top stopped a
  header-height short — hiding the Pack 1 marker — and Bottom stopped short for
  the same reason. Select all / Deselect all / Invert are mode-dependent: while
  selection mode is on they drive `picked` (`pickAll`/`clearPicked`/
  `invertPicked`), otherwise `included` (`setAll`) — before this they only ever
  touched `included`, so picking several emoji looked broken because nothing
  on screen responded to them.
- **Selection and history** (`assets/panel-holding.js`). Selection mode hides
  the inclusion tick and shows only the pick control. Shift-click picks the
  inclusive range from the last anchor, skipping logos and held cards.
  Snapshots include order, inclusion, picks, hold origins and view settings.
  One drag or pick stroke creates one undo entry; cancelled drags restore the
  original snapshot. **Reset all** restores the last successful explicit Save
  in this page and is itself undoable. A Save captures its checkpoint before
  sending, so a later edit is never folded into an older acknowledgement.
- **Holding area.** Held cards stay in the ordering model but occupy no grid
  slots. **Unhold** restores the original slot; **Unhold all** checks every
  destination before changing any card. A full original pack refuses with a
  capacity message. The grid drop commits a tray card's inclusion before
  recording history; `dragend.dropEffect` is not used as proof of a successful
  drop. Tray thumbnails are still images and are reused across updates.
  The tray is **windowed** like the grid: only held cards near its visible
  stretch are mounted, between two spacers, at a 70 px card pitch (64 px wide
  plus the 6 px gap).
  The tray joins **selection mode**: each held card carries the same pick box a
  grid card does, click plus shift-click takes a run of them, dragging one
  picked card carries every picked held emoji, and **Unhold** on a picked card
  returns the whole picked set. Without it a held emoji was pickable nowhere —
  the pick box lives on a grid card and a held emoji has none — so the tray was
  the one place selection mode could not reach.
- **An emoji dropped into another pack joins it.** The panel states the
  INTENDED layout; it is not a mirror of what is live on Telegram. Drag an
  emoji into the pack you want it to end up in, save, and the emoji is
  re-stamped into that pack — the grid redraws with it inside the destination
  run. Nothing moves on Telegram at that moment and nothing can: the Bot API
  has no move-between-sets call, so a real move is a delete plus a re-add that
  mints a new `custom_emoji_id`. **Save writes the decision to
  `<data-dir>/pack_plan.json`** (§ "The move plan" below) and the real
  rearrangement is a separate, deliberate step performed from that file.
  The re-stamp is the load-bearing half: without it `packStarts()` re-groups
  the card by the pack number it arrived carrying, it becomes a one-card run
  still labelled with the pack it came from, and the drag reads as having
  snapped back — which is exactly how this was first reported.
  **The pack comes from the card the pointer was aimed at**, recorded while the
  drag is live, never from whatever ends up above the card once it lands. Those
  two answers differ at every pack boundary, and the difference was measurable:
  a held emoji dropped on pack 4's FIRST card landed stamped pack 3. Every drop
  re-stamps, from the tray and from inside the grid alike — a grid-to-grid drag
  used to keep its old pack number and cut the destination pack in two. A drop
  that never passed over any card stamps nothing: an unknown destination is not
  a destination. A drop into a run that has no pack number yet leaves the emoji
  without one, so it continues that run instead of starting a false one.
- **A full pack takes nothing, and the holding tray is how you make room.**
  Packs are fixed 200-emoji buckets, logo included. A drop into a pack already
  at 200 is refused with `Pack N is full (200/200). Hold one of its emoji
  first, then bring this one in.` — park one of that pack's own emoji in the
  tray, then bring the replacement in. Capacity is asked of the DESTINATION,
  not of the emoji's own pack; asking the wrong one refused moves OUT of a full
  pack and allowed moves INTO one.
- **Pack boundaries and numbers** count included cards and the brand logos.
  Holding four entries from a full pack changes `#1–#200` to `#1–#196`, including
  when that pack is followed by other published packs. The same visible-index
  projection controls virtual rows, card positions and separator ranges.
- **Save queues** (`assets/panel-save.js`). Orders auto-save; inclusion
  waits for explicit Save. Each queue permits one request at a time and keeps
  its newest body. A 400/409 rejection blocks only that body; a newer body can
  progress and still retry a transient failure. Debounce, retry and heartbeat
  share the same eligibility deadline. Dirty state and unload protection remain
  until the corresponding state is acknowledged; the warning offers a local
  JSON draft export for conflict recovery.
- **UI logs** (`emojikit/panel_logging.py`). Browser events are batched at most
  once per second, with a three-second delivery deadline and a 32-event buffer.
  The authenticated endpoint validates the complete batch before logging and
  caps request frequency. UI actions and error locations go to the panel's
  normal UTF-8 UTC log; arbitrary exception text and private content do not.
  Logging failure does not enter or block either save queue.
- **The panel scripts** load in this order, as classic scripts sharing one
  scope (`emojikit.panel.SCRIPT_FILES`): `panel-grid.js`, `panel-motion.js`,
  `panel-save.js`, `panel-drag.js`, `panel-actions.js`, `panel-holding.js`,
  `panel-draft.js`. `assets/jsconfig.json` and `assets/panel-globals.d.ts` let
  `tsc` type-check them (`checkJs`); the CI `worker` job runs it.
- **The grid is virtual** (`assets/panel-grid.js`). `ITEMS` is the order and
  the selection; the DOM holds only the rows within half a screen or three rows
  of the viewport, whichever is smaller, between two spacers that carry the height of everything above and
  below. Every card is the same fixed height and every row offset is integer
  arithmetic on numbers JavaScript computes and hands to CSS as variables
  (`--cols`, `--cardH`, `--sepH`, `--gap`, `--z`), written only when a value
  changes — each write restyled every card, even an unchanged one — so a render
  is a binary search plus ~100 node moves, whatever the catalog holds. Measured on the
  1 063-card catalog in headless Chromium: a drag step cost 24–46 ms per
  pointer move on the old page (every one re-laid-out 1 062 cards; 40 % of
  frames over 32 ms) and 16–24 ms here with none over 32 ms; a cold scroll
  sweep spent 802 ms in long tasks before and 113 ms after, and the 1.8 s hang
  the old page showed on the way back up is gone. 11 743 DOM nodes became 714.
- **Zoom** scales the card: everything inside a card is sized in `em` off one
  `font-size: calc(12px * var(--z))`, and the column count follows from the
  container width, so `--z` changes both how big a tile is and how many fit.
  Below `0.75` the body gets `compact` and a card carries only its header and
  thumbnail; the text rows are not built at all. The
  item at the top of the screen is re-scrolled to the top after the relayout.
  Range 0.4–2.4, steps of ×1.15, persisted as `panelZoom`. Ctrl+wheel and
  Ctrl+plus/minus/0 are intercepted (`passive:false`) so the browser's own page
  zoom does not fire as well. `zoomReset` is a typeable `<input>`, not a
  reset-only button: Enter parses and applies a percentage, double-click still
  resets to 100 %. `paintZoom()` writes `.value`, not `.textContent` — the
  latter is silently inert on an `<input>`, which is exactly why the click
  handler that used to live there moved to `panel-holding.js` instead of
  growing in place.
- **Drag edits the model as you drag** (`assets/panel-drag.js`). Each
  `dragover` that changes the target moves the carried items inside `ITEMS`
  and re-projects the grid, so the translucent tile IS where they land; the
  drop only records the snapshot taken at `dragstart` as history and saves; a
  cancel restores that snapshot. There is no index arithmetic at drop time.
  A carried card whose row scrolls out of the window is **parked** in a hidden
  holder, never removed: the browser delivers `dragend` to the source node,
  and a removed source leaves the gesture stuck.
- **Static, animated and (by default) video cards are all plain `<img>`** —
  the browser owns decoding and compositing. Not `loading="lazy"`: a card only
  exists once its row is near the viewport, so the window is the lazy loading.
  A video card shows an animated WebP preview rendered from the video; creating
  or tearing down a media player was the 60–140 ms frame left after the grid
  went virtual. With **Real video** on, a video card is
  `<video preload="metadata">` (muted, looping, `playsinline`) whose **source is
  attached only while animation is on and the card is in the viewport** and
  released when it leaves (`videoIO`). Players are never created or torn down
  mid-scroll: a release requested during a scroll is queued and done once it
  settles. Changing the switch rebuilds the mounted video cards, but never
  under a drag.
- **One `IntersectionObserver` (no margin) reports what is on screen, and
  `allocate()` decides what moves** (`assets/panel-motion.js`). At most
  `ANIM_BUDGET` = 24 cards animate, the ones nearest the centre of the screen
  below the sticky header; **All visible** lifts the cap. At low zoom the old
  page decoded 128 animations per frame the moment Animation was switched on.
  The observer swaps an animated card's `src` between the still (`?still=1`)
  and the animated WebP, and plays/pauses a real `<video>`. A grid of frozen
  stills cannot be curated, which is why hover-only was rejected. A scroll
  freezes only the cards that were playing and thaws them once it settles
  (180 ms). While the page scrolls or a card is dragged, `body.moving` pauses
  the pick ring's animation too. The checkerboard backdrop is drawn with one
  `repeating-conic-gradient`.
- `prefers-reduced-motion` is respected: nothing plays by itself, and hover
  becomes the only way to play a video, which is what those handlers are for.

The panel ships no animation library: animated emoji are rasterised to WebP by
`rlottie-python` on the server, so the page needs nothing from a CDN and works
offline.

### The move plan (`<data-dir>/pack_plan.json`)

`.\native\runtime\numera-emoji.exe plan-status [--live]` shows, read-only, what applying the
saved plan would change — per pack: target count and logo slot against the cap,
emoji moving in and out, held emoji still live, never-published candidates — and
every `custom_emoji_id` a move would retire. Exit 3 when the plan asks for work.

`.\native\runtime\numera-emoji.exe plan-apply` applies it (design: `docs/design/plan-applier.md`):

```powershell
.\native\runtime\numera-emoji.exe plan-apply                  # dry run: what would change, writes nothing
.\native\runtime\numera-emoji.exe plan-apply --apply          # change the live packs, 20 changes per run
```

| Option | Default | Meaning |
|---|---|---|
| `--data-dir` | `collection` | catalog, plan, publisher state and apply journal |
| `--base` | `COLLECTION_PACK_BASE` | the pack family; only `--mixed` families are supported |
| `--token-env` | `GENERAL_BOT_TOKEN` | variable holding the bot token |
| `--user-id` | `PACK_OWNER_USER_ID` | the owner id Telegram requires for adds |
| `--apply` | off | without it nothing is written anywhere |
| `--max-changes` | 20 | counted changes per run, 1..20 (the owner's cap) |
| `--max-wait` | 300 | longest Telegram flood wait (seconds) one run sleeps out |

Per run, in this order: **removals** (the outgoing half of each move), then
**adds** in the panel's order (moves arriving, then never-published emoji the
plan puts in a pack) through the publisher's own verified path, then a
**reorder** of every pack whose order differs from the panel's. The Bot API has
no move call, so a move is a delete plus a re-add and the emoji gets a **new
`custom_emoji_id`**; the retired id is kept in `retired` of
`<data-dir>/plan_apply_<base>.json`. Owner decisions of 2026-09-29:

- a cross-pack move is allowed even though the id changes;
- a **held** emoji that is still live is never removed — it stays until placed;
- at most **20 counted changes** (removals and adds; creating the next pack with
  its logo is one more) per run. Reordering keeps ids and is not counted.
  Reaching the cap ends the run with exit 3; the next run continues.

Safety: the run holds the same catalog lease and pack-family lock as the
publisher. Each delete is journalled (`intent`) before it is sent, the sticker
is found by its recorded `custom_emoji_id` at its recorded position, and the
delete is confirmed by re-reading the set; a run that died after sending one is
settled from the live set on the next start, never by re-sending. Refused
before any change (exit 2): a plan over a pack's cap (held emoji keep their
slot), a key the catalog no longer holds or has excluded, a per-format family, a
target pack that is not an existing one or the next new one, an unreadable
journal, a held lock. A set whose live state cannot be read stops the run
(exit 1) and is reported as unknown. Exit codes: 0 nothing pending, 3 work
remains, 2 refused, 1 stopped mid-run (the records and journal say what was
done). A Telegram flood wait up to `--max-wait` seconds is slept out; a longer one ends the run with exit 3 (the refused call applied nothing, the state and journal are saved, a pending delete intent is cleared), and the next run continues. Run
`pack_manifest --refresh` afterwards so the roster records the retired ids.

The panel decides nothing on Telegram. It is where the owner says what the
layout **should** be; every save writes that decision to `pack_plan.json` beside
`catalog.db`, and rearranging the live packs is a separate, deliberate step read
from that file.

It has to be written explicitly rather than inferred from the order, because
with fixed 200-emoji buckets a pack boundary does not follow from position:
"which pack was this meant for" is a decision, and a decision that is guessed
later is a decision nobody made.

```json
{
  "version": 1,
  "written_utc": "2026-09-19T02:07:19Z",
  "per_set": 200,
  "targets": [["s:e8dc…", 3], ["s:5cb9…", 1]],
  "known":    ["s:5cb9…", "s:c0ff…", "s:e8dc…"],
  "excluded": ["s:c0ff…"],
  "counts": {"1": 199, "2": 197, "3": 199},
  "logo_slots": {"1": 1, "2": 1, "3": 1},
  "over_capacity": {},
  "moves": [{"key": "s:e8dc…", "label": "refx-nexus-3-round",
             "from_pack": 2, "to_pack": 3}],
  "held":  [{"key": "s:c0ff…", "label": "premium-id:5447…", "from_pack": 3}]
}
```

| Field | What it is |
|-------|------------|
| `targets` | **The authoritative intent**: every emoji the panel has an opinion about, paired with the pack it should end up in. The server reads this back on load, so a saved move survives a reload. |
| `moves` | The subset of `targets` whose intended pack differs from the one the emoji is live in — the work, derived for the reader's convenience. `targets` is the source of truth; `moves` is what it means today. |
| `held` | Every emoji parked in the tray, with the pack it came out of. Not a move — the owner took it out and has not said where it goes; parking one is how room is made for an arriving emoji. |
| `known` | Every key any save has spoken for. A page shows only part of the catalog, so this records the scope decisions were made in and lets a later partial save merge instead of overwrite. |
| `excluded` | Every key currently held out of a pack, across all saves — not only the ones the last page could see. |
| `counts` | Emoji per pack in the intended layout. |
| `logo_slots` | Packs whose brand logo consumes one of the 200 slots. The logo is emoji 0 of **each** pack, not once per family, so capacity has to count it per pack. |
| `over_capacity` | Packs whose `counts` plus `logo_slots` exceed `per_set`. Normally empty, because a drop over the cap is refused while curating — but a plan read back later must be able to say so rather than look healthy and fail at publish. |

A **partial save merges**: a page that can see only some of the catalog replaces
the decisions inside its own scope and keeps every other decision in the file.
That is why `targets`, `known` and `excluded` are whole-catalog while `moves`
and `held` read as a to-do list. A plan written by an older panel that carries
only `moves` is still understood — its moves are read as the intent.

The one thing a save does drop is a decision about an emoji the catalog no
longer holds. The keys are checked against the database under the same lease
that writes the plan, so a deleted emoji stops being counted and stops
reserving a slot; without that the file would only ever grow, and a pack could
report itself over capacity because of emoji that are gone. A pack nobody
targets any more loses its `logo_slots` entry for the same reason.

An unreadable `pack_plan.json` is never silently replaced with an empty one: the
panel refuses to load and names the file, because discarding saved intent to
keep the page opening is the worse failure. Repair or move the file, then
reload.

An emoji the page carried no pack for is **absent** from the plan entirely:
saying nothing and saying "leave it" are different claims, and only the first
is true. A candidate that was never published has no `from_pack`, so it is
counted but never a move — there is nothing to move it out of. The brand logo
is skipped: it is emoji 0 of every pack and does not travel.

The file is written atomically, because the step that reads it may start at any
moment and a half-written plan is a scrambled instruction set.

---
