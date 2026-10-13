[Complete guide index](GUIDE.md)

### 12.6 `panel` — curate web panel

| Flag | Default | Meaning |
|------|---------|---------|
| `--data-dir` | `collection` | Catalog/media directory. |
| `--all` | off | Also show emoji already live in a pack. |
| `--with-pack N` | off | Also show the emoji already live in pack **N**. Repeatable. |
| `--port` | `9450` | Local port. |
| `--preview-fps` | `15` | Maximum frame rate for animated previews. The grid uses at most 15, and 10 below 75 % zoom (`panel_preview.page_tiers`). |
| `--no-open` | off | Don't auto-open the browser. Otherwise it opens straight after the port is bound; the bot name (which only decides whether the brand-logo card shows) is looked up in the background, so an unreachable Telegram never holds the start-up. |
| `--bot-username` | — | Branding without a Telegram lookup (see "Brand logo preview"). |

Interactions: **click** a card to toggle include/exclude, **click the
`premium-id:` label** to copy that id to the clipboard (it stops there and does
not toggle the card), **drag** a card to reorder (this is the publish order),
**zoom** with `−` / `100%` / `+`, Ctrl+wheel or Ctrl+plus/minus (Ctrl+0 resets;
the level is remembered). Zooming out packs more emoji per screen and, below
75 %, drops the text rows under the thumbnails; the emoji at the top of the
screen stays at the top of the screen. Each card shows its **grid position** at
the top; the numbers are written from the order every time the grid is
projected, never stored on the card. **The brand logo is
numbered and counted**, because it is the first emoji of every set it leads and
costs one of the 200 (`capacity = per_set - 1` in `build_collection`). Leaving
it out made the panel disagree with what ships — the owner read "200" and the
pack was 201. When the total passes the per-set cap the header says so, with how
many packs it will actually become, rather than letting a second set be a
surprise.

Colour carries the format on the **badge only**: static cyan, animated violet,
video emerald, brand logo amber. The card border is the same for every card —
per-format borders turned the grid into stripes on a dark background — and the
include tick is green, because it answers a different question from the badge
and must not read as the same axis. The header controls each have their own
accent so the row is scannable. Dragging to the top or
bottom edge of the window scrolls the page, so an item can be carried across
the whole catalog in one motion. Releasing anywhere that is not a card cancels
— it used to mean "move to the end".

**A refresh shows the current catalog, and only one panel may hold a port.**
Two separate reasons a reload used to appear to do nothing:

* the page served a snapshot of the catalog taken at start-up, so an emoji
  added by `fetch-emoji-ids` afterwards was invisible until a restart. The
  view is now re-read from the database on every page load (200 rows, a few
  milliseconds), replacing the shared list **in place** — rebinding it would
  leave every route closed over the old object;
* `socketserver` sets `SO_REUSEADDR` by default, and **on Windows that lets a
  second bind succeed on a port that already has a live listener**. Two panels
  then ran, both logging `Panel at …`, the browser reached whichever socket the
  OS picked, and the older process kept serving its own start-up snapshot —
  which is why only closing the launcher (killing every instance) made a change
  appear. `allow_reuse_address` stays off. B4 identifies an existing panel and
  reopens its session without replacing it; an unrelated port holder is refused.

If a refresh cannot open the catalog — a publish holding it, say — the page
keeps the last view it had and says so beside the counts: "catalog busy (a
publish may be running) — showing the view from HH:MM UTC". A busy catalog used
to look exactly like a catalog with no news.

Editing `panel` itself still needs the process restarted — a refresh asks the
running server for a page, and that server holds the old code.

**Losing the panel process is never silent.** The page polls `GET /api/ping`
every 5 s. If the panel is gone — or a save is refused — a red banner appears
*and stays* (a toast fades in 2.6 s, which is how an afternoon of reordering
was once done against a dead server and never noticed). The unsaved order is
kept in the page and flushed automatically the moment the panel answers again,
including across a **restart**: the mutation token is per run, so the page
re-reads it from `/` after a 403 and retries. Closing the tab with unsaved work
triggers the browser's "leave site?" prompt.

**Never point automated UI checks at `collection/`.** Reordering is what this
panel does, so a synthetic drag event *is* a write — there is no careful way to
test it against real data. `scripts/panel_sandbox.py` clones the catalog to a
temp directory, serves it on the real panel's port + 1 (imported from
the shared `emojikit.cli_env.PANEL_PORT`, never typed again), and deletes the clone
on exit.

It isolates four things, because cloning the data turned out to be only one of
them.

**The arguments are an allowlist.** `--source`, `--port`, `--all`,
`--with-pack N` and `--bot-username` are accepted; everything else is refused,
including abbreviations (`allow_abbrev=False`). Nothing is forwarded: the
panel's argument list is built inside the wrapper. It used to forward unknown
options straight through, appended AFTER its own `--data-dir`, so
`panel_sandbox.py --data-dir collection` served the live catalog while the
wrapper printed that the live catalog was not served.

**Making it never writes to the source.** The source catalog is opened
READ-ONLY under the ordinary writer lease -- never through `Catalog`, whose
constructor sets WAL mode, creates tables and commits, so cloning an older
catalog used to upgrade it in passing, even when the clone was then refused.
The copy is an online snapshot through `emojikit.sqlite_snapshot`, so committed
rows still sitting in the WAL come too. (A read-only open of a WAL database may
leave an empty `-wal`/`-shm` beside it: that is SQLite's own bookkeeping, not a
change to the catalog.) The CLONE may upgrade its own schema when the panel
first opens it.

**The clone shares no bytes, and they are the right bytes.** Media is COPIED,
never hard-linked, into `media/<hex>.<ext>`, where `<hex>` is the content key's
UTF-8 bytes in hexadecimal: injective, so two keys can never share a file (the
old colon-to-underscore swap mapped `s:a_b` and `s_a:b` to one name), and free
of the colon that opens an alternate data stream on NTFS. Each `file_path` says
which rule it follows (`emojikit.media_paths.resolve`): `./...` is the source data
folder, any other relative path is the PROJECT ROOT (the shape older code wrote) --
and an absolute one (the owner's archive) stays absolute;
searching beside the source first used to let a stray file of the same name win.
Each file's SHA-256 is compared before copying, on the copy, and on the source
again afterwards, so a same-length corruption or a file changed mid-copy
refuses the whole clone; comparing sizes let both through. Every `file_path` is
rewritten, and `pack_plan.json` travels with it.

**It refuses rather than half-succeeds.** Another writer, an interrupted
migration, an occupied or overlapping destination, or a media file it cannot
read: each aborts and removes only what that attempt created. A sandbox finished
with one row still pointing at production is worse than no sandbox.

**One sandbox never destroys another.** Each is OWNED from the first
directory it creates to the last file its clean-up removes, by a lifetime lease
held outside it, in `.panel-sandbox-leases/` beside the sandboxes. The lease is
taken before anything is built -- the ownership marker used to be published
first, and a sweep in that gap deleted a finished clone before it was served --
and outlives the directory, because removing a lock file is what lets two
processes each lock a different inode at the same name. Each lease file is a
small permanent tombstone by design. The start-up sweep reclaims a directory
only when its `.sandbox-owner.json` (protocol version 2) names that very
directory AND the lease is free, and it checks the marker again after taking
the lease. Unmarked, foreign, symlinked, still-served and version-1 directories
are left alone: a version-1 owner may still be running under the old scheme,
and an age or a PID is not proof it has stopped. The first sweep matched the
name prefix and deleted every hit, so starting a second sandbox removed the
first one's catalog.

**And it isolates the account.** The wrapper launches the installed native panel
with `NUMERA_EMOJI_MAPPER_NO_DOTENV=1` and every key `.env.example` names, plus
anything credential-shaped, stripped from its environment. An already-exported
token otherwise needs no dotenv file and could reach live Telegram during bot
detection. The selected Python interpreter remains only the exact-media codec
adapter, not the panel application.

The wrapper owns the child's process tree and waits for it before deleting its
clone. Windows uses a private kill-on-close Job. Linux passes the same locked
lifetime file description to the server; the server monitors the wrapper's PID
and start time and keeps that lease through shutdown and codec finalization.
Killing the wrapper therefore cannot make a still-served clone reclaimable.
A fresh clone's session identity never authorizes adoption of another listener.

This is cooperative test-data isolation, not an OS boundary against hostile code
running as you:

```powershell
.venv\Scripts\python.exe scripts\panel_sandbox.py
```
The sandbox copies the source's preview cache into the clone (best effort: a
failed copy is logged and those previews render on demand), so a sandbox does
not spend minutes re-rendering what the real catalog already has.

Animated and video cards play on their own while on screen, at most 24 of them
at once — the ones nearest the centre; **All visible** lifts that cap. Under
`prefers-reduced-motion` nothing autoplays and hover plays a real video. The header's
**Animation: On/Off** stops everything, and **Real video** swaps the light
video previews for `<video>` players; all three are remembered in
`localStorage`.

**Order = publish order.** The panel shows items in the saved manual order
(`items.position`). On first open it is seeded to the look-alike similarity
order; after that, drag-and-drop reordering is saved (POST `/api/order` →
`Catalog.set_order`) and drives both the panel and `build_collection` (each
per-format set publishes in this relative order). The brand-logo preview card
is fixed first and is never reordered/counted/saved.

**Performance.** The grid is virtual; only nearby rows exist in the DOM.
Animated previews are native WebP images, with no browser animation library.
The tiers come from one place, `panel_preview.page_tiers`: **full** is 104 px
at up to 15 fps, **compact** (below 75 % zoom) is 72 px at up to 10 fps on
ordinary-density screens. `--preview-fps` (1–30, default 15) caps both. A video
card's preview is an animated WebP decoded through the same path as its still
(`panel_preview._video_animation`), cached as `<hash>@v<fps>-<size>.webp` so an
older single-frame file under `@<fps>-<size>` is never served as the animation.
The cache key includes content identity, size and rate.

**The cache is warmed in the background, and the render bound is resource-aware.**
Measured on this catalog, one animation costs ~155 ms and one video poster
~613 ms; the bound was a hard-coded 2, so a cold tier of 442 animations arrived
two files at a time — about 34 s of rendering delivered in visible chunks while
the owner scrolled. The bound is now `max(2, min(6, cpu_count - 2))`, which on a
16-core machine cut one viewport of 24 cold animations from 2.92 s to 1.55 s
(1.9×) while still leaving most of the machine to the OS and the save handlers.
A daemon thread then renders what the page is about to ask for, in grid order,
so the top of the list is ready first and the scroll meets a warm cache. It
renders exactly the tiers the page requests — the full tier, then the compact
one — for animated and video cards (a still and an animation each), plus the
compact still the holding tray shows for an excluded card; a static card is
served from `/img/` and never warmed. A file already cached is skipped, and the
log line says `preview warm-up: rendered N, already cached N, failed N`. On a
cold sandbox clone of the maintainer's catalog it rendered 1505 files in ~2.5
minutes with the panel fully usable throughout. It is best effort — an
unreadable file is skipped rather than ending the pass, and Ctrl+C never waits
for it.

Only genuinely visible animations play: the sticky header's covered region,
hidden tabs and scrolling are excluded, and of the rest at most 24 play unless
**All visible** is on. Switching animation off shows every card's still and,
with **Real video** on, releases the players' sources. Changes affect preview quality, not stored or
published media. Measure performance on a separate catalog with the same media,
viewport, zoom and warm-cache state; desktop load and browser configuration matter.

**Brand logo preview.** `--bot-username YourEmojiBot` selects branding
without a Telegram lookup. Otherwise, if `GENERAL_BOT_TOKEN` resolves to a bot
in `BRAND_LOGO_BOTS` and `BRAND_LOGO_PATH` exists, the panel shows it as a distinct **gold-bordered
first card** labelled "Brand logo (auto-added on publish)" so you can see where
it will land *before* publishing. This card is preview-only: it's not clickable,
not counted in the included/excluded totals, and never sent to `/api/save` — the
logo itself is never part of the catalog and is only actually inserted by
`build-collection` at publish time (see §12.5). For an unlisted bot, or if the
logo is not configured or missing, the card is simply not shown.

**Emoji already live in a pack are hidden.** The panel arranges the pack being
BUILT, and `is_published` skips a published item at publish time however it is
ticked here — so showing it only invites pruning work that changes nothing.

The rule was "hide only a FULL set" for one round, on the theory that a set
still being filled is still the pack being built. `--new-set` (§12.5) ended
that: a pack can now be left half-empty *on purpose*, so "full" stopped meaning
"finished", and a half-empty published pack kept reappearing in the grid for
the next one. Being published is the property that actually settles it, and it
needs neither a `publish_*.json` nor any capacity arithmetic.

The test is "does this key have a publication row", **not** "does it have a
recorded set name": a row whose `set_name` is NULL is still published, and
reading the name would turn *I do not know where it went* into *it was never
published* — offering a live emoji up to be republished.

The header says how many are hidden; a filter nobody can see is
indistinguishable from having lost the items. They are hidden, never deleted:
those catalog rows are what dedup recognises a re-download by, what maps a
source premium id to ours, and what `sync_order` reads to re-sort an already
published set. `--all` brings them back.

**Each shown pack draws its own brand logo.** Those packs already carry it as
their emoji 0 — it went up when the pack was created — and the logo card opens
that pack's run, so the marker sits above it. One card for the whole grid put
the logo on whichever pack was shown first and left the others looking as though
they had none. Without `--with-pack` the single "auto-added on publish" preview
is unchanged: there the logo is not live yet.

**`--with-pack N` is the narrow version of `--all`.** It un-hides one published
set so a half-full pack can be arranged beside the new candidates going into
it — `emojikit/panel.py --with-pack 5` shows pack 5's emoji and the unpublished ones
together, and nothing else. `--all` is the wrong tool for that job: it also
returns every finished pack, which on a grown catalog is hundreds of cards that
cannot change. The index is resolved through the publisher's own
`publish_*.json`, not by rebuilding `<base><n>_by_<bot>`, because the state file
already records the exact name. Here — and only here — the lookup asks *which*
set an item is in, so a publication row with no recorded `set_name` stays
hidden: unknown-where is not answered with a guess.

**Pack boundaries are drawn in the grid.** A full-width marker carrying the
brand logo heads each pack and is labelled with the grid range it spans, so a
selection that publishes as several packs shows where each one starts and ends.
The splits count **included** items only — an unticked card never reaches
Telegram, so it cannot push the next emoji into the following pack — which is
why ticking recomputes them and not just the counter.

**Where the boundary comes from depends on what the grid holds.** For
candidates it is capacity arithmetic: a new pack every `--per-set` minus the
logo's slot. For emoji that are ALREADY live it is real membership — each such
card knows its pack index, and the marker goes wherever that number changes.
Capacity cannot answer this case at all: two published packs of 95 and 96 are
neither of them a full set, so counting to capacity finds no seam and the grid
would show `--with-pack 2 --with-pack 5` as one unbroken run.

A candidate gets no marker of its own. It used to open a "Not in a pack yet"
run, so every emoji dragged INTO a pack split that pack in two and left a
full-width marker plus the empty rest of its row behind it — and arranging IS
dropping candidates into a pack, so the grid broke exactly while it was being
used. A candidate now continues the run it was dropped into, which is also the
pack it will publish into. The marker is deliberately
a `.packsep` and never a `.card`: the drop handler resolves its target with
`closest('.card')`, and a marker that matched would swallow a drop and silently
do nothing.

The header's **↑ Top / ↓ Bottom** buttons jump to the ends of the grid.

**Dragging shows where the card lands, because the card goes there.** While you
drag, the tile is moved into the slot it would take and drawn translucent with
a dashed outline; releasing just adopts that order. Which half of a tile the
pointer is on decides before-or-after, so the last slot of a row is reachable.
Let go outside the grid, or press Escape, and the card returns to where it
started. It can never be carried ahead of the brand logo.

**Nothing animates while you scroll.** Every card holds frame 0 from the first
scroll event until 180 ms after it settles, and only cards actually in the
viewport animate at rest. A pack of a hundred is mostly animated previews of
30-45 frames each, and scrolling is the one moment that decoding buys nothing.
Switching to another tab freezes them all. `Animation: Off` is still the
lightest the grid can be — nothing decodes at all.

#### Selection mode — move a run in one gesture

Arranging a 200-card pack one emoji at a time is the slow part, so the header
carries a second switch beside **Animation**:

| Gesture | What it does |
|---------|--------------|
| Toggle **Selection** | Reveals an EMPTY pick box on every card. Turning it off drops the picks. |
| Click a card anywhere | Picks or unpicks it — grid card or held card. The whole card is the target, not the box. |
| Shift-click a card | Picks the whole run between it and the last one picked. |
| Click a pick box | Picks or unpicks that one card, exactly once. |
| Drag across pick boxes | Picks the whole run; dragging back **shrinks** it inside the same stroke. |
| Drag a picked card | Carries every picked card together, keeping their order. |

**A check means picked, and nothing else.** An unpicked box is empty. It used to
draw the same check in both states and only change colour, so a box that was not
picked still looked ticked, and the only difference between two states was a
colour. In the tray that also made a multi-selection unreachable in practice:
the box is about 17px on a 64px card, so a click on the card did nothing, and
the drag that carries a whole selection never had a selection to carry.

Clicking the card is safe alongside dragging because a drag emits no click at
all — the two gestures separate themselves by what you did, not by where you
pressed. The grid works the same way now; it used to ignore a click entirely in
selection mode, which left that 1.7em box as the only way to pick on a 140px
card.

**A picked card wears a bright ring that travels around it.** The marker used to
be a flat inset outline, one more dark line among four format accent colours,
and a selection could not be found at a glance. The ring is a conic gradient
rotated by a transform — the compositor animates that without repainting, which
matters on a grid that is virtual precisely because repainting cost 14-17 ms per
pointer move. It stops moving, and stays bright, under `prefers-reduced-motion`.

The pick box is deliberately NOT the tick. The tick says "this ships"; the pick
says "this moves with the others" — and while selection mode is on, clicking a
card no longer toggles the tick, so arranging can never quietly drop an emoji
from the pack. A cancelled group drag puts every carried card back where it was.

### 12.7 `emoji-bot` — premium-emoji ID extractor bot

No flags. Uses `GENERAL_BOT_TOKEN` + `PACK_OWNER_USER_ID` from `.env`. One
instance at a time (two pollers cause Telegram 409 Conflict). Each reply is a
**single** collapsed quote of `emoji + ID` (per-ID tap-to-copy via `<code>`)
plus `copy_text` “Copy” button(s) underneath — not two quotes (see §8/§19).

**It also runs in reverse: send it ids and it shows you the emoji.** One per
line, comma-separated, `, `-separated, or a single id — all parse. The whole
message must be ids and separators, so a long number inside a sentence (a chat
id, a timestamp) is ignored rather than answered with a wall of placeholders.
Ids are resolved through `getCustomEmojiStickers` first: a `<tg-emoji>` tag
renders the placeholder glyph for an id that does not exist, so an unreported
typo would come back looking exactly like a success. Anything Telegram cannot
resolve is named in the reply.

### 12.8 `coins/` commands

Run each from the repository root as a module, `python -m coins.<tool>`; a
tool run as a file path cannot import the shared `emojikit` package.

The expanded migration branch additionally exposes `numera-emoji coin-alias-map`
and `coin-enhance-map`, each with `--coin-dir DIR` (default project `coins/`).
They atomically update the canonical map and filled inventory under its existing
map lease; no provider/Telegram request is made. Unknown options refuse. These
additive commands are not available in the accepted prebuilt runtime until the
full migration/package gates pass. Non-string map IDs currently refuse rather
than silently invent source-compatible formatting.

`numera-emoji coin-build-keywords [--coin-dir DIR]` on that branch uses the
native canonical-name paging and shared CSV writer. SVG wins over a same-ticker
PNG; unreadable PNGs are omitted, earlier names survive empty lookups, and an
incomplete provider lookup exits 3. It reads `COIN_PAGE_DELAY` (default 2 seconds),
needs no Telegram token and makes no Telegram request. The accepted installed
runtime and complete source/platform parity remain unchanged/pending.

The same branch adds `numera-emoji coin-fetch-logos [pages] [--coin-dir DIR]`.
The default is 40 pages of 250 coins; nonnumeric/zero counts and unknown flags
refuse before networking. Good cached PNGs resume without download, corrupt
cache is refreshed, downloaded bodies must pass image verification, SVG takes
precedence and keyword CSV retains prior names. Paging failure exits 3. This
command contacts CoinGecko/image hosts but never Telegram; it is not available
in the accepted installed package yet.

Additive `coin-fetch-paprika [--dry] [--retry-unmatched] [--coin-dir DIR]` and
`coin-fetch-cmc [--dry] [--coin-dir DIR]` wire the native provider/search/codec/
publisher seams. Exact dry mode stops before downloads or Telegram; Paprika
still saves answered search-cache entries. Unanswered searches exit 3, rejected
CMC credentials stop naming the key, and live token admission precedes provider
work. **Without `--dry` these commands can publish to Telegram**; test only with
isolated loopback fixtures until full migration/package acceptance. Existing
installed launchers and retained Python commands have not been switched.

The branch also exposes `coin-write-manifests [--out-dir DIR] [--state PATH]
[--map PATH] [--token-env KEY]`. It reads live sets without Telegram mutation,
preserves their order, joins sorted ticker aliases and marks unmapped IDs.
Default inputs/output are project `coins/`; `COIN_PACK_BASE` identifies the
family lease. The chosen token variable is redacted. Unknown live readback stops
before changing prior reports; per-file Markdown/index replacement is atomic,
not a transaction across all report files. Full source/platform acceptance is pending.

Additive `coin-check-packs [--coin-dir DIR]` (default project `coins/`) reads
`rebuild_dedup_state.json`, resumes `pack_audit4.json` and atomically writes
`pack_audit4_report.txt`. It reads/downloads Telegram media only; no pack/map
mutation. Failed decode/downloads retry on the next run, current live IDs drive
locations/totals and deleted cache IDs are pruned only after complete listings.
Each set and every 100 new analyses checkpoint; four exhausted listing attempts
preserve progress and exit 4 without replacing the report. Partial image analysis
exits 3, all failed analyses exit 4. Full source/platform acceptance is pending.

Additive `coin-remap-ids --emoji-dir DIR` exposes `--token-env`, `--state`,
`--cache`, `--out`, `--candidates`, `--max-distance`, `--min-margin` and `--apply`.
It reads live IDs/manifest signatures and matches source artwork by content,
never slot position. Dry mode writes signature cache only. Apply needs a finite
positive calibrated cutoff; margin defaults to cutoff, ambiguous or incomplete
work exits 3 and writes candidates without canonical-map replacement. A successful
replacement keeps the original map bytes in a never-overwritten `.prebroken.json`.
Family-before-map leases protect the whole live-read/replace workflow. NumPy 2.4.6
remains the supported external numerical runtime for Python 3.11; later 2.5.4
requires 3.12+. Installed/runtime and full source/platform acceptance are pending.

| Command | File | Purpose |
|---------|------|---------|
| `-m coins.fetch_logos [pages]` | `coins/fetch_logos.py` | Download coin logos (CoinGecko) + write `keywords.csv`. `pages` is how many 250-coin market pages to walk (default 40 = up to 10 000 coins). Exits 3 when paging stopped early; the file is still written, keeping names an earlier run found. |
| `-m coins.fetch_paprika [--dry] [--retry-unmatched]` | `coins/fetch_paprika.py` | Fill remaining coins from CoinPaprika. `--dry` resolves and reports only — no downloads, no pack or map changes. `--retry-unmatched` searches again for coins cached as unmatched (older runs cached provider outages that way). A search CoinPaprika never answers is **unknown**, not "no match": it is not cached, and the run exits 3 so it is retried. |
| `-m coins.fetch_cmc [--dry]` | `coins/fetch_cmc.py` | Fill remaining coins from CoinMarketCap (needs `CMC_API_KEY`). `--dry` as above. A rejected key (HTTP 401/403) stops the run naming `CMC_API_KEY`; an unanswered lookup is unknown and exits 3. |
| `-m coins.build_keywords` | `coins/build_keywords.py` | (Re)build `keywords.csv` from logos on disk. Exits 3 when a name lookup failed; earlier names are kept. |
| `-m coins.rebuild_dedup [all\|build\|map\|links]` | `coins/rebuild_dedup.py` | Default `all` = **delete the old packs** + build + map + links (DESTRUCTIVE); `build` uploads only; `map` re-derives the id map; `links` resends links. |
| `-m coins.remap_ids --emoji-dir DIR [--max-distance N --apply]` | `coins/remap_ids.py` | Rebuild `ticker_to_id.json` by image content (drift-proof). `--apply` requires `--max-distance > 0`; `--min-margin N` rejects a match whose runner-up is closer than N (default: `--max-distance`); `--candidates FILE` is where a refused `--apply` writes its reviewable result; `--cache`, `--out`, `--state` and `--token-env` move the live-signature cache, the map written, the state read and the token variable. Needs numpy (`requirements-coins.txt`). |
| `-m coins.verify_logos --emoji-dir DIR [--fix --only a,b]` | `coins/verify_logos.py` | Review logos vs official; fix only listed tickers. `--top N` (default 100) is how many coins by market cap to review, `--threshold N` (default 20) the review-flag distance; `--map`, `--state` and `--token-env` choose the map, the state file and the token variable. |
| `-m coins.check_all_packs` | `coins/check_all_packs.py` | Audit all packs for blank/duplicate stickers. |
| `-m coins.write_manifests --out-dir DIR` | `coins/write_manifests.py` | Write per-pack manifest `.md` files. `--state`, `--map` and `--token-env` choose the state file, the map and the token variable. |
| `-m coins.enhance_map` | `coins/enhance_map.py` | Map chain-suffixed tickers (e.g. `bnbbsc`) to the base id. |
| `-m coins.alias_map` | `coins/alias_map.py` | Map tickers to a base id by matching coin name. |

### 12.9 `worker/` — Cloudflare Worker (both bots + pack announcements)

Both bots hosted on Cloudflare instead of this machine, plus the endpoint the
local build calls so a finished pack is announced **by the bot** in the channel.

| Route | Auth | What |
|---|---|---|
| `POST /tg/general` | `X-Telegram-Bot-Api-Secret-Token` | Webhook, general bot |
| `POST /tg/coin` | `X-Telegram-Bot-Api-Secret-Token` | Webhook, coin bot |
| `POST /publish` | `Authorization: Bearer …` | Announce finished packs |
| `GET /health` | none | Liveness; returns no secrets |

Each bot has its **own path and its own webhook secret** — the token never
appears in a webhook request, so one shared endpoint could not tell the bots
apart, and one shared secret would let a leak from either forge the other's
updates.

**A token can use `getUpdates` or a webhook, never both.** Registering a webhook
stops `emoji-bot` (§12.7, §19) receiving anything on that token;
`deleteWebhook` hands it back. Run one or the other per token.

`ADMIN_USER_IDS` **fails closed**, exactly like `emojikit.emoji_bot.allowed_user_ids()`:
unset, empty or all-invalid means the bots answer nobody. Only plain positive
integers count — `Number()` would have accepted `0x10`, `12.5` and `1e3`.
A stranger gets one reply in private and **silence in a group**, so the bot
cannot be turned into a spam vector.

Webhook handlers return **200 even when handling fails**. Telegram redelivers
any non-2xx and every action here is a `sendMessage`, so a redelivery after a
partial success posts the reply twice; failures are logged instead. Nothing is
retried internally, for the same reason as the Python client (§5).

Local side: set `WORKER_PUBLISH_URL` **and** `WORKER_PUBLISH_SECRET` and
`emojikit.build_collection.notify()` routes links through the Worker; leave either unset
and the original direct `sendMessage` path runs unchanged. The duplicate guard
does not move — the per-milestone lists still decide, and a *failed* announcement
is deliberately not recorded as sent, or the guard would skip it forever.
Pack names are validated against `[A-Za-z0-9_]{1,64}` before they reach a public
`t.me/addemoji/` link.

**A trailing set is announced only by a run that finished cleanly** — no failed
upload and no item skipped. A channel link says "this pack is done", and the
end-of-run announcement used to be unconditional, so a run that ended 199 of 200
still posted it. Nothing is lost by withholding it: `state["sent"]` never
records it, so the next clean run announces it, and the log says why it was
held back. A set that filled to capacity *during* the run is announced as it
completes — that one is finished by definition.

**Going up and filling up are two milestones, and each gets its own link.**
`state["sent"]` records the first, `state["sent_full"]` the second; `notify(...,
full=True)` is what the capacity branch calls. One list conflated them, and a
pack announced while it was still being filled then said nothing at 200 — which
is the only moment the channel is waiting for. Pack 2 was announced at 96 emoji
and stayed silent when it reached 200. Neither milestone fires twice, and a pack
that was already announced full before this existed belongs in `sent_full` so
the next run does not repost it.

```powershell
cd worker; npm install
npm run typecheck                    # tsc --noEmit
npm test                             # vitest; fetch stubbed, nothing reaches Telegram
.\scripts\put-secrets.ps1 -DryRun    # which key comes from where; no values shown
.\scripts\put-secrets.ps1            # pipes them from ..\.env into wrangler stdin
npx wrangler deploy
```

`wrangler.toml` has **no `[vars]`**. Everything the Worker reads — both tokens,
both webhook secrets, the publish bearer, the admin list *and the channel* — is
a secret, so none of it is in the committed file. The channel is not a
credential; it is a secret only because `wrangler.toml` is public and both
forms arrive as `env.PACK_LINKS_CHAT_ID` anyway.

`scripts\put-secrets.ps1` reads `.env` with the same parse as
`emojikit.build_pack.load_env` and pipes each value to `wrangler secret put` **through
stdin** — never an argument (arguments are visible in the process list), never
printed. It composes `ADMIN_USER_IDS` from `PACK_OWNER_USER_ID` +
`BOT_ALLOWED_USER_IDS` (deduped, integers only), because that list fails closed
and a hand-typed mistake is silent: the bots simply answer nobody. Missing
webhook/publish secrets are generated and written **back** to `.env`, or the
next run would mint different ones and every delivery would fail its check. A
missing token or channel is an error, not something to invent.

Webhook registration is `scripts\set-webhooks.ps1` — one webhook per bot, read
from the same `.env` the secrets came from so the registration and the deployed
secret cannot drift. A mismatch is silent: Telegram accepts `setWebhook` and
every delivery is then rejected 401, which looks exactly like a dead bot.
`-Status` reports, `-Delete` hands a token back to the poller, and it refuses to
replace a webhook pointing elsewhere without `-Force`.

**Logs.** The channel line puts the bot tag on its own first line, then a level
emoji and the event:

```
[general]
❌ ERROR webhook
update 42: sendMessage failed (400): chat not found
2026-08-19 00:45:12 UTC
```

Both bots share this Worker, one D1 table and one channel, so an untagged line
is not worth keeping. The channel is capped at 12 messages a minute; over budget
it drops and counts rather than queueing (a queue in a Worker isolate outlives
its request and loses them anyway), and the count rides on the next message
through. That budget is per isolate, not global. `LOG_CHAT_ID` also accepts a
bare channel id copied from the Telegram UI and adds the `-100` prefix.
**Channel posts from the log channel itself are ignored** — both bots administer
it, so every line posted there came back to both and each wrote another row
about a message we had just written.
D1 stores **warnings, errors and publishes only**; routine lines and
unauthorised hits go to `console.log` / Workers Logs (`[observability]` in
`wrangler.toml`), because D1's daily allowance is account-wide. The table has
no secondary index and no AUTOINCREMENT (`migrations/0002_lean_logs.sql`), so a
stored line costs 1 row written and 0 read. It keeps the **newest 5 000 rows**:
every 250th row id runs one `DELETE … WHERE id <= ?`, a rowid range that reads
only what it deletes. One line's detail is capped at 2000 characters (a publish announcing 120 packs
listed every name and cost ~6 KB alone). `LOG_CHAT_ID` receives **errors only**
— an unauthorised hit on a public webhook URL is a WARNING, and level-based
routing would let an internet scanner turn that channel into a firehose. Both
bots must administer it; each posts its own lines. Logging is handed to
`ctx.waitUntil()` and every sink failure is swallowed, so it can neither delay
a response nor take a bot down. `GET /health` reports `log_db`.

```powershell
npx wrangler d1 execute numera-emoji-mapper-logs --remote `
  --command "SELECT ts, bot, level, event, detail FROM logs ORDER BY id DESC LIMIT 20"
```

Per-secret detail: `worker/README.md`.

---
