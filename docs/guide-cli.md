[Complete guide index](GUIDE.md)

## 12. Full CLI reference

Every entry point, every flag, with defaults and examples. All commands assume
you run them from the project root with the venv Python
(`.venv\Scripts\python.exe`). On macOS/Linux use `.venv/bin/python`.

### 12.1 `make-emoji-pngs` — image → 100×100 PNG

| Flag | Default | Meaning |
|------|---------|---------|
| `--in <dir>` | *(empty)* | General mode: source folder of mixed images. |
| `--out <dir>` | `<in>_emoji` | Output folder for the 100×100 PNGs. |
| `--limit <n>` | `0` (all) | Convert at most N images this run. |

Behaviour:

- **General mode** (when `--in` is given): converts every `.svg .png .jpg .jpeg
  .webp .gif .bmp .apng` in `--in` to `<name>.png` (100×100, transparent) in
  `--out`. Skips files already converted (idempotent), and records a marker so a
  hung SVG is blacklisted on the next run (`.svg_skip.txt`, `.svg_cur`).
- **Legacy coin mode** (no `--in`): reads `coins/logos/svg/*.svg` then
  `coins/logos/png/*.png` and writes `coins/logos/emoji/<ticker>.png`.
- **Blank guard**: if an SVG renders blank (e.g. an unsupported gradient), it is
  **not** saved — the loop falls back to the raster `png/` source; a blank raster
  is skipped too. No blank emoji is ever produced.
- **`logos/.incoming/<run>/`** is staging, not a source folder. Each fetcher run
  gets its OWN subdirectory: the download happens before any lock is taken, so a
  path shared by ticker name could be replaced by a second fetcher between this
  run's hash, its upload, its verification and its promotion. A logo moves into
  `logos/emoji/` only once its upload is confirmed and the map records its id,
  so `logos/emoji/<ticker>.png` always describes a sticker that really exists —
  the other tools use that file to decide which live sticker belongs to which
  ticker. A run deletes only its own staging directory, never another's, which
  may still be mid-upload; a directory left by a killed run is harmless and can
  be removed by hand once no fetcher is running.

  One file survives on purpose: if a run ends with an **unresolved** upload (the
  add reached Telegram but could not be confirmed), the image it sent is kept so
  the next run can prove what landed. Deleting it would leave that run falling
  back to its own fresh download of the same ticker and publishing *that* as the
  logo for a sticker made from the original image. It is removed once the
  unresolved upload is settled. If it is ever lost anyway, the next run still
  identifies the live sticker and updates the map, but says plainly that
  `logos/emoji/<ticker>.png` was **not** updated — re-fetch the ticker to refresh
  it.

Examples:

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe make-emoji-pngs --in input\myset --out build\myset
.venv\Scripts\.\native\runtime\numera-emoji.exe make-emoji-pngs --in input\myset --limit 50
.venv\Scripts\.\native\runtime\numera-emoji.exe make-emoji-pngs            # legacy coin mode
```

### 12.2 `build-pack` — upload a folder of PNGs to emoji sets

| Flag | Default | Meaning |
|------|---------|---------|
| `--base` | *(required)* | Set-name base: begins with a letter; letters, digits and single underscores between them. |
| `--title` | *(required)* | Human-readable set title. |
| `--source-dir` | `coins/logos/emoji` | Folder of 100×100 PNGs to upload. |
| `--token-env` | `TELEGRAM_BOT_TOKEN` | Env var holding the bot token. |
| `--keywords` | `auto` | keywords CSV; `auto` = `coins/keywords.csv`, only for the default source or `COIN_EMOJI_DIR`. |
| `--emoji` | `🪙` | Associated standard emoji. |
| `--user-id` | `PACK_OWNER_USER_ID` | Numeric owner id. |
| `--per-set` | `200` | Emojis per set. Accepted range is **1–200** (Telegram's cap); anything larger exits 2 with a usage error. |
| `--limit` / `--start` | `0` / `0` | Process a slice of the source. |
| `--state` | `state_<base>.json` | Resume file (per pack, never clobbered). |
| `--dry-run` | off | Validate inputs without calling Telegram. |

Resumable: progress is saved to `state_<base>.json`; an interrupted/flood-limited
run continues without recreating existing sets. Use `--dry-run` first.

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe build-pack --base myset --title "My Emojis" `
    --source-dir build\myset --token-env GENERAL_BOT_TOKEN --emoji 😀 --dry-run
```

### 12.3 `fetch-pack` — download a Telegram pack into the catalog

| Flag | Default | Meaning |
|------|---------|---------|
| `packs...` | *(required)* | One or more pack short-names, `t.me/addemoji/<name>` links, or emoji ids (digits or `premium-id:<id>`) — an id fetches the whole pack it belongs to. Each pack is fetched once; an id that names no pack is reported and counts as a failure (exit 3 when something else succeeded). |
| `--token-env` | `GENERAL_BOT_TOKEN` | Bot token env var. |
| `--data-dir` | `collection` | Catalog/media directory. |
| `--phash-threshold` | `-1` (off) | Hamming distance for near-dup merging; `-1` keeps look-alikes. |
| `--limit <n>` | `0` (all) | Max new items per pack. |
| `--repaintable` | `ask` | What to do with emoji Telegram REPAINTS (`ask`/`skip`/`keep`). The client overrides their colours, so the source pack does not show the stored art: in one of ours they render that art instead — sometimes flat black, sometimes full colour. Look before you decide; with no terminal to answer, `ask` skips them. |

Re-running is cheap: stickers whose `file_unique_id` was already ingested are
skipped without downloading; identical media collapse to one catalog row.

### 12.3b `fetch-emoji-ids` — download *specific* emoji by ID (not whole packs)

Downloads only the individual premium custom-emoji you name — e.g. the
`premium-id:<n>` entries inside bot inventory files — and nothing else from
their packs. Only **real entry lines** that start with `premium-id:` are taken;
example/prose mentions like `(e.g. premium-id: 123)` are skipped so example IDs
are never fetched by mistake. IDs are then **de-duplicated** so each real emoji
is fetched at most once, resolved via `getCustomEmojiStickers` (batched,
≤200/call), downloaded and content-hashed into the same catalog.

| Flag | Default | Meaning |
|------|---------|---------|
| `--ids-file <path>` | *(empty)* | File containing `premium-id:<n>` lines or bare IDs. Repeatable. |
| `--id <n>` | *(empty)* | A single custom-emoji ID. Repeatable. |
| `--token-env` | `GENERAL_BOT_TOKEN` | Bot token env var (any bot can resolve IDs). |
| `--data-dir` | `collection` | Catalog/media directory. |
| `--phash-threshold` | `-1` (off) | Near-dup merge threshold; `-1` keeps look-alikes. |
| `--repaintable` | `ask` | What to do with emoji Telegram REPAINTS (`ask`/`skip`/`keep`). The client overrides their colours, so the source pack does not show the stored art: in one of ours they render that art instead — sometimes flat black, sometimes full colour. Look before you decide; with no terminal to answer, `ask` skips them. |
| `--tint <#RRGGBB>` | *(off)* | Bake the repaint into the asset: flatten every REPAINTABLE emoji to this colour, keeping its silhouette — the same thing a client does, done by us because the flag itself cannot be set on an existing set. Answers `--repaintable`, so nothing is skipped. Animated goes through the Lottie so the animation survives; static fills through the alpha; video is refused. Recorded on the item as `tint:#RRGGBB`. |

Two levels of de-duplication protect you: **ID-level** (repeated IDs across
files fetched once) and **content-level** (two different IDs pointing at the
same media collapse to one catalog row). It also reports **within-file
duplicates** (the same real entry repeated inside one file) separately from
cross-file duplicates. The run reports
`unique_ids / new / dedup / failed / missing`; `missing` counts IDs Telegram
could no longer resolve. Each row keeps its origin in `keywords`
(`premium-id:<n>`). The extraction/dedup logic is covered by
`tests/test_fetch_emoji_ids.py` (example lines, trailing labels, bullets,
within-file and cross-file duplicates, bare-ID lists).

Example — pull only the emoji referenced by four bot inventory files:

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe fetch-emoji-ids `
  --ids-file "...\GV Swap bot\bot-emoji-inventory-user.md" `
  --ids-file "...\GV Swap bot\bot-emoji-inventory-admin.md" `
  --ids-file "...\YourBrand Payment Bot\bot-emoji-inventory-admin.md" `
  --ids-file "...\YourBrand Payment Bot\bot-emoji-inventory-user.md"
# -> collected 240 real id occurrences -> 101 unique (83 ids duplicated across files)
# -> Done. unique_ids=101 new=100 dedup=1 failed=0 missing=0
```

### 12.4 `add-media` — build emoji from local files into the catalog

| Flag | Default | Meaning |
|------|---------|---------|
| `files...` | *(optional)* | Individual source files. |
| `--in <dir>` | *(empty)* | Folder of source files. |
| `--as` | `auto` | `auto`/`static`/`video`/`animated` target format. |
| `--emoji` | `😀` | Associated standard emoji. |
| `--keywords` | *(empty)* | Comma-separated extra keywords. |
| `--data-dir` | `collection` | Catalog/media directory. |
| `--phash-threshold` | `-1` (off) | Near-dup merge threshold. |

Auto format: still image → static; animated GIF/APNG/MP4/WEBM/MOV → **video**
(ffmpeg); Lottie `.json`/`.tgs` → **animated**. Remember: a raster animation
cannot become an *animated* emoji (those are vector-only) — it becomes a *video*
emoji.

### 12.5 `build-collection` — publish the catalog into new packs

**`--mixed` publishes every format into ONE family**, named `<base><n>_by_<bot>`
with no format letter, in the curate panel's order. Without it each format gets
its own sets — which was this tool's default from before Bot API 7.2 allowed
mixed sets, and it costs the curation: the panel's order runs *across* formats,
so splitting regroups a hand-arranged pack into format blocks. It is a flag
rather than the new default for one concrete reason: `state["sets"]` and the
frozen plan are keyed by format, so flipping it would make an existing
half-published family unresumable. A family started one way refuses to continue
the other, with a usage error rather than a stranded set.

`--base` follows Telegram's own rule for a set name: letters, digits and
**single** underscores, beginning with a letter. The `_by_<bot_username>` tail
is appended and is **not optional** — Telegram rejects a name without it. The
64-character limit is checked against the real bot username before the first
upload, not discovered as a Bot API error after the plan is frozen.

**The publish takes the panel's order at the moment you publish.** The plan
(`publish_plan_<base>.json`) is the append-only record of WHAT is queued; the
pending items are uploaded in the catalog's current panel order, so arranging
the next pack's candidates in the panel decides which emoji fill it and in what
order. A `--dry-run` or `--preflight` computes the plan without writing it and
changes nothing. Resume safety does not depend on this order: each item is
marked published per family, and the upload order is recorded in the state file.


**Set titles are one sequence across every format.** `--title "@YourBrand Emoji
Packs"` produces `@YourBrand Emoji Packs 1`, `2`, `3` … in creation order,
whatever format each set holds. They used to carry the format word and count
per format (`… Animated 1`, `… Static 1`), so two different packs were both
called "1". The number counts every set already recorded, which is also what
makes it resumable: a restarted run continues the count instead of restarting
it. Set **names** are unchanged — `<base>s<n>` / `<base>v<n>` / `<base>a<n>`
remain the identity, and only the human title moved.

| Flag | Default | Meaning |
|------|---------|---------|
| `--base` | *(required)* | Set-name base: begins with a letter; letters, digits and single underscores between them. |
| `--title` | *(required)* | Human-readable title. |
| `--token-env` | `GENERAL_BOT_TOKEN` | Bot token env var. |
| `--user-id` | `PACK_OWNER_USER_ID` | Numeric owner id. |
| `--emoji` | `😀` | Fallback associated emoji. |
| `--formats` | `static,video,animated` | Which formats to publish, in order. |
| `--per-set` | `200` | Emojis per set. |
| `--new-set` | off | Start this run in a **fresh** set instead of filling the current one. |
| `--into-pack` | *(newest)* | Add to pack **N** instead of the newest one, so any pack with room can be topped up. |
| `--data-dir` | `collection` | Catalog/media directory. |
| `--brand-logo` | `BRAND_LOGO_PATH` | First-emoji brand logo, for the bots in `BRAND_LOGO_BOTS` only. |
| `--no-brand-logo` | off | Disable the mandatory first-emoji logo. |
| `--mixed` | off | Publish every format into ONE family (see above). |
| `--dry-run` | off | Show the plan without uploading. |
| `--preflight` | off | Ask Telegram to validate every queued file, then stop. Publishes nothing. |
| `--repaint` | off | Create NEW sets with `needs_repainting`, so the client paints every emoji in them the text/accent colour. Whole-set and creation-only: it cannot be added later and it flattens colour art. |

**What `--preflight` reports, and what each exit code means.** Four outcomes,
counted apart, because collapsing them is how a run once claimed a validation
it never performed:

| Outcome | Meaning | Effect on the exit code |
|---------|---------|-------------------------|
| accepted | Telegram said the file is uploadable. | — |
| refused | Telegram rejected it (`BotApiError`); the message is printed. | exit 1 |
| missing | The catalog row points at a file that is not on disk. | exit 1 |
| not checked | The request never reached Telegram (transport). | exit 3 |

The last row is the one that matters, and the count of unreachable files does
not change it. A dropped connection is **not** a verdict on the artwork, so it
is never reported as a refusal — exit 1 is what a refusal returns, and
answering a lost connection with it sends someone editing artwork that was
never the problem. It is not reported as an acceptance either: the old counter
counted attempts, so a run in which every single check failed at the transport
printed "all accepted" and exited 0. Exit 3 says the one true thing — the run
did not finish asking. `tests/test_preflight_outcomes.py` pins each of the four.

**Leaving a pack unfinished (`--new-set`).** Normally set *N+1* opens only when
set *N* reaches `--per-set`, so a pack you want to stop early has no way
forward. `--new-set` rolls this run into a fresh set and leaves the current one
at whatever size it has — that is how you start pack 3 while pack 2 is still
half empty. It is consulted **once per run**: later items in the same run fill
the new set normally, so pass it on the run that should open the pack, not on
every run afterwards (each one would open another set). The two workarounds it
replaces are both wrong: a smaller `--per-set` caps every *later* set at the
same wrong size, and a second `--base` starts a new family whose `publications`
table is empty, so the entire catalog would be re-uploaded into it.

**Topping up an older pack (`--into-pack N`).** Publishing always appended to
the newest set, so once pack 3 existed there was no way to put anything back
into a half-empty pack 1 — its remaining room was unreachable. `--into-pack 1`
aims this run at that pack instead.

It fails loudly rather than falling back, because a silent fallback would fill
some *other* pack and still look like success: an unknown number, a full pack,
or one holding a sticker this publisher cannot identify each stop the run. That
last case is not fussiness — appending past an unattributable sticker is what
hands a new key someone else's `custom_emoji_id`.

`--new-set` and `--into-pack` together are rejected: one opens a fresh pack and
the other fills an existing one, so there is no sensible combined meaning.

The subtle part is bookkeeping, not upload. The live count and the recorded key
order used to be written to the *last* set's record, which is the same thing as
the target only while you are filling the newest pack. Filling a middle one
would have credited the upload to the wrong record — state describing a set the
sticker never entered, which is exactly the drift `reconcile_set` exists to
catch. Both now follow the pack actually being written to, and a test pins it.

Only **included** (panel-selected), not-yet-uploaded, non-skipped items are
published. Per-format sets, drift-proof resume, per-pack manifests.

**Brand logo (first emoji of every set).** Your own logo, configured in
`.env` -- the repository ships none, because it names no operator:

| Key | Meaning |
|-----|---------|
| `BRAND_LOGO_BOTS` | Bot usernames whose sets lead with the logo, comma-separated. **Set it empty** for "no bot"; unset stops the publish. |
| `BRAND_LOGO_PATH` | The logo image. Relative paths are under the project root; `private/` is git-ignored for exactly this. |
| `BRAND_LOGO_KEYWORDS` | Optional search keywords for the logo emoji (default `logo`). |

When publishing with a listed bot the logo is inserted as the **first emoji of
every set** (`--brand-logo` overrides the path). A listed bot whose logo file
is missing stops the publish before anything changes: the first slot cannot be
filled afterwards. Since Bot API 7.2 (March 2024) a single custom-emoji set may
contain **mixed formats**, so the logo is always a **static** 100x100 PNG and
leads a static, video *or* animated set alike (verified live). Leave the coin
bot out of `BRAND_LOGO_BOTS` to keep it exempt. Disable with `--no-brand-logo`.
The logo occupies position 0, so item `custom_emoji_id`s are read from position 1 onward (handled automatically).

### 12.5b `sync-order` — reorder an ALREADY PUBLISHED pack

| Flag | Default | Meaning |
|------|---------|---------|
| `--base <name>` | *(required)* | The published family to reorder. |
| `--data-dir <dir>` | `collection` | Catalog/state directory. |
| `--token-env <VAR>` | `GENERAL_BOT_TOKEN` | Which token owns the packs. |
| `--pack N` | *(all)* | Only reorder pack N (repeatable). Publishing appends, so the live order still has to be applied separately - and applying it family-wide would move stickers in packs you never arranged. |
| `--apply` | off | Actually move stickers. Without it, report only. |

Rearranging the panel after a pack is live does **not** mean republishing it.
`setStickerPositionInSet` moves a sticker that is already in the set, so every
emoji keeps its `file_id` **and** its `custom_emoji_id`: nobody who already
uses one is affected, and no upload happens.

```powershell
$PY -m emojikit.sync_order --base mypack             # what would move
$PY -m emojikit.sync_order --base mypack --apply
```

It is the one set mutation in this project that is genuinely idempotent —
setting the same sticker to the same index twice leaves the same set — so it
may be re-run, and an interrupted run simply continues. Moves are planned as a
selection sort (one API call each, ~120 ms apart), which means a run stopped
halfway leaves a set that is correct up to where it stopped rather than
scrambled.

Two refusals, both about identity:

- **An unrecognised sticker stops that set.** Positions only mean something
  once every sticker is identified; shuffling around an unknown one would move
  a stranger's emoji into the middle of the pack.
- **The brand logo stays at position 0.** It is not a catalog item, so it is
  the one legitimately unknown sticker — and only at index 0.

It takes the same pack-family lock as the publisher: reordering while a publish
appends would move stickers out from under it.

**It also rewrites the order the publisher recorded** (`state["sets"][n]["keys"]`)
to match what it just made live, and does so even when nothing needed moving.
The publisher verifies every recorded position by identity before adding to a
set, so a reorder that left the record behind made the family unpublishable:
`position N now holds a sticker this publisher cannot identify`. A report-only
run writes nothing.

### 12.5c `pack-manifest` — the roster of what is in every published pack

| Flag | Default | Meaning |
|------|---------|---------|
| `--refresh` | — | Read the live sets and rewrite `packs/`. |
| `--check` | — | Is `packs/` still current? Exit 3 if not. Touches no network. |
| `--family` | `all` | With `--refresh` only: `general` or `coins`. The other family's packs and id lookups in `index.json` are kept, with the input fingerprint they were built from. |

Writes three files per set, named after it: `.json` to parse, `.md` to read, and
`.html` to LOOK at — one self-contained page, every thumbnail inlined as a
`data:` URI so animation plays with no player, no CDN and no network, and the
whole roster repeated in a `<script type="application/json">` block so a parser
never has to scrape the markup.

**The page is the curate panel, minus everything that mutates.** Same grid, card,
per-format accent, position pill and checkerboard thumb, and the same view-only
header: ↑ Top, ↓ Bottom, the four-way backdrop cycle and the animation switch.
What is absent is everything that writes — no `draggable`, no tick, no selection,
no save, no form field at all.

**It gates its media like the emojikit.panel.** Every animated card inlines a still as
well as the animation, starts frozen, and only what is on screen is swapped to
the moving version; scrolling freezes everything until 180 ms after it settles,
and `Animation: Off` freezes it permanently. Thumbnails are 88px at 9fps — fps
is the biggest lever on the inlined weight, and a roster is for telling emoji
apart rather than admiring the motion. It is a record; a control that
looks live but saves nothing is worse than none. Each card carries all THREE ids,
labelled and separately click-to-copy: **this pack**, **original pack** where
the emoji came from someone else's, and **this pack, before** — every id the
emoji held in OUR packs earlier, oldest first. Only a replace mints a new
`custom_emoji_id` (`setStickerPositionInSet` and `setStickerEmojiList` leave it
alone), and the retired one is exactly what an external map still points at, so
it is kept rather than overwritten. The catalog cannot supply it: `publications`
is keyed `(base, content_key)`, so a replace overwrites the old id and it is
gone. The roster is therefore its own archive — each `--refresh` reads the
previous roster and carries the trail forward, keyed by `history_key`
(`ck:<content_key>`, `logo:<set>`, or `coin:<tickers>`) so the history survives
both a new id and a move to another pack. `packs/index.json` adds
`by_current_id`, `by_source_id` and `by_previous_id` (a retired id → the id that
took its place) over every live pack, plus an `id_changes` count.

Every row gives the emoji's `custom_emoji_id`, its `#` numbered **from 0** (the
brand logo is emoji 0) beside the 1-based `slot` Telegram shows, its format, the
**glyph the sticker carries** (shown under the artwork in this page and in the
curate panel — Telegram never displays it, so these two grids are the only place
the label can be checked against the art), a name, and — when it came from someone else's pack — **the id it had
there**. The coin family carries no brand logo (that bot is exempt), so its
emoji 0 is a real coin and the page says so.

**The order and the ids are read from Telegram, never from the publisher state.**
A live reorder or an in-place replace (which mints a NEW id) would leave a
state-derived roster confidently wrong, and a roster that is quietly wrong is
worse than none.

Thumbnails are cached in `packs/.thumbs/<custom_emoji_id>.<ext>`, keyed on the
id rather than the path because a replaced sticker gets a new id — exactly when
its picture must be re-made. `packs/` is git-ignored: it is derived data, and one
refresh rewrites ~79 MB.

Coin artwork lives outside the repo; point `COIN_EMOJI_DIR` at it, or the coin
pages come out without pictures (the data is unaffected).

**Keep it current.** `--check` is cheap and offline; it exits 3 when the
roster no longer matches its inputs — a new download, a replaced or recoloured
sticker, a reorder, a coin remap. Run it after anything that changes a pack (or
wire it into your own automation); `--refresh` rebuilds the roster.
`.\native\runtime\numera-emoji.exe status` (§12.5e) runs this check and the archive's together.

### 12.5d `pack-archive` — the finished pack's media leaves the project

Once a pack is FULL its media moves out to the owner's archive, one folder per
pack, and the project keeps no copy. A pack still being filled is left alone on
purpose: the filename carries the emoji's **slot**, and an unfinished pack can
still be reordered, which would make every name in its folder wrong.

| Flag | Default | Meaning |
|------|---------|---------|
| `--check` | — | Does the archive still describe the packs? Exit 3 if not. Local only, no network. |
| `--sync` | — | Archive every FULL pack and regenerate its metadata from the live set. |
| `--export PACK --zip PATH` | — | Copy one pack of the general family (its number or set name, full or not) into a zip, in slot order, named like the archive, with its `_manifest.md`. Moves and changes nothing. Refused while the roster is stale (run `pack_manifest --refresh`), and refused -- nothing written -- when any file of the pack cannot be found. Needs `COLLECTION_PACK_BASE` only. |

`EMOJI_ARCHIVE_DIR` is the archive root and `COLLECTION_PACK_BASE` the pack
family it follows; both are required, with no default. Each folder holds:

```
001_logo.png                         your brand logo, copied from BRAND_LOGO_PATH
<slot>_<format>_<key[:12]>.<ext>     one per emoji; slot is 1-based, the logo is 1
_history.json  _history.md           every position, id, content key and glyph
_manifest.md                         name -> current id, the quick lookup
```

`--sync` also rewrites `items.file_path` in the catalog, because that absolute
path is what the roster gallery reads to draw the artwork; dedup is unaffected,
since a `content_key` hashes normalised pixels held in the database rather than
the file. It **renames** anything whose slot moved and regenerates the three
metadata files every run — a recolour mints a new id and a reorder moves slots,
so an archive written once and never revisited stops describing its pack. It
never deletes: a file the live pack no longer knows is reported, not removed.

`--check` is cheap and offline and exits 3 when the archive has fallen behind;
run it after anything that changes a full pack, and `--sync` to catch up.
`.\native\runtime\numera-emoji.exe status` (§12.5e) runs it together with the roster check.

### 12.5e `status` — is everything current?

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe status
```

Offline and read-only: runs the roster check (`pack_manifest --check`) and the
archive check (`pack_archive --check`), and counts the included catalog emoji
not yet published to `COLLECTION_PACK_BASE`. One line per item, each with the
command that fixes it. Exit 0 when nothing is stale, 3 when something is;
emoji waiting to be published are a to-do, not staleness. An archive whose
`EMOJI_ARCHIVE_DIR` is unset is reported as not checked, never as current.
