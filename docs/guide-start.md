[Complete guide index](GUIDE.md)

## 1. What this project does

Numera Emoji Mapper turns images/animations into **Telegram premium custom-emoji
packs**, and also **collects** premium emoji from existing packs and posts.

Two independent but related sides:

| Side | Bots | Purpose |
|------|------|---------|
| **Crypto coins** (`coins/`) | `TELEGRAM_BOT_TOKEN` (`@YourCoinEmojiBot`) | the coin-logo packs |
| **General / collector** | `GENERAL_BOT_TOKEN` (`@YourEmojiBot`) | build any pack, copy packs, extract IDs |

Supported custom-emoji formats: **static** (PNG/WEBP, 100×100), **animated**
(`.tgs` = gzipped Lottie), **video** (`.webm`/VP9, ≤256 KB, ≤3 s).

---

## 2. Repository map

```
Numera Emoji Mapper/
  run.ps1                  Windows launcher (menu). Prefer this.
  native/src/              Rust application/data/HTTP/network/report responsibilities
  native/runtime/          installed numera-emoji executable (outside Cargo targets)
  emojikit/                exact codecs and native-backed shared coin exports
    media_bridge.py        narrow exact codec/identity IPC boundary
    media.py, media_lottie.py, identity.py, video_decode.py, repaint.py
    telegram_api.py, announce.py, packstate.py, pack_rows.py  native bindings
    cli_env.py, operator_config.py, logsetup.py, similarity.py  shared support
    sandbox_clone.py, maintenance.py, media_paths.py, sqlite_snapshot.py,
    state_artifacts.py     declared development/reference helpers
  tests/reference/         preserved pre-port application behavior (not production)
  assets/                  the panel page (panel.html) and its scripts, loaded in this order:
    panel-grid.js            the model, the virtual grid and zoom
    panel-motion.js          which cards animate, and when (the animation budget)
    panel-save.js            the save queues and the server heartbeat
    panel-drag.js            drag & drop and edge auto-scroll
    panel-actions.js         selection gestures, toolbar, toasts, the boot block
    panel-holding.js         history, holding tray and Save checkpoints
    panel-draft.js           export a draft to a file and import it back
    jsconfig.json + panel-globals.d.ts   the type-check the CI worker job runs (tsc checkJs)
  coins/                   the crypto-coin component (see §7)
    _paprika_api.py        CoinPaprika HTTP + candidate search + logo decode
  scripts/check.ps1        byte-compile + full unit suite (CI runs this too)
  native identity-repair     report/migrate/restore catalog keys
  scripts/panel_sandbox.py    the panel against a throwaway copy of the catalog
  scripts/run-actions.ps1     the launcher's menu actions (dot-sourced by run.ps1)
  tests/                   unit tests + fixtures (see tests/README.md and §10)
  docs/GUIDE.md            this file
  .env.example             config template
  requirements-dev.txt     test-only deps (playwright, for the browser suite)
  collection/              (gitignored) catalog.db + media/ + manifests/ + state
  logs/  build/  input/  logos/   (gitignored) generated/working data
```

Generated/local-only (gitignored): `collection/`, `logs/`, `build/`, `input/`,
`logos/`, `*_state.json`, `*.filled.md`, `.env`, `secrets.md`,
`coins/remap_live_cache.json`, `coins/ticker_to_id.prebroken.json`.

---

## 3. Setup (one time)

Download the matching prebuilt package from a successful **Prebuilt packages**
workflow. Normal installation requires Python 3.11 and FFmpeg/FFprobe, not
Rust/Cargo or a local source build. See [installation](../README.md#install-a-ready-built-package--no-local-rust-build).

```powershell
py -3.11 scripts/install_prebuilt.py
.\run.ps1 -Check
.\run.ps1
```

The installer uses bundled binary wheels offline, verifies hashes/platform and
refuses to overwrite an existing installation. Updates use a fresh directory;
retain configuration/data and verify the new runtime before removing the old one.
All owned temp/cache paths stay physically under the package root. Native command
paths below use the installed executable; Linux/macOS use
`.venv/bin/python scripts/launch_prebuilt.py COMMAND` instead.

### Native ownership and rollback

Rust owns the catalog, transport/retries, state/locks, collectors/publishers, bot,
HTTP panel, migration and reports. Python remains for exact Pillow/Lottie/FFmpeg
codecs, thin native-backed shared coin exports and declared development support;
it is not a second application backend. See [native backend](native-backend.md)
and [the migration decision](adr/0003-rust-local-backend.md).

Only Rust contributors need Rust 1.99+ and a platform linker and explicitly run
`python scripts/build_native.py`. The menu never compiles. The doctor checks the
codec imports and installed native executable without prompts or installation.
Persisted schemas/keys/identifiers are unchanged. For rollback, restore the
pre-cutover application in a separate checkout and use verified recovery bundles;
never discard current operator data or unresolved migration journals.

`.env` keys:

```
PACK_OWNER_USER_ID=<your numeric Telegram id>   # owns every created set; press Start on each bot once
TELEGRAM_BOT_TOKEN=<coin bot token>
GENERAL_BOT_TOKEN=<general bot token>
BOT_ALLOWED_USER_IDS=<optional: extra ids allowed to use emojikit/emoji_bot.py>
BOT_ALLOWED_CHANNEL_IDS=<optional: channel ids (-100...) whose posts the bots answer; empty = none>
PACK_LINKS_CHAT_ID=<optional: channel that receives finished-pack links>
WORKER_PUBLISH_URL=<optional: https://<worker>.workers.dev/publish — see §12.9>
WORKER_PUBLISH_SECRET=<optional: bearer for that endpoint; both or neither>
EMOJI_LOG_RETENTION_DAYS=30    # optional: prune logs/ older than N days (0 = keep all)
EMOJI_FFMPEG_TIMEOUT=300       # optional: seconds per ffmpeg/ffprobe child
CMC_API_KEY=<optional CoinMarketCap key, only for coins/fetch_cmc.py>
COIN_PAGE_DELAY=2              # optional: seconds between coin provider pages

# Your identities -- required where used, NO default (the repo names no operator)
BRAND_LOGO_BOTS=<bots whose sets lead with your logo; EMPTY = none, unset = stop>
BRAND_LOGO_PATH=private/brand-logo.png   # your logo; private/ is git-ignored
BRAND_LOGO_KEYWORDS=<optional: logo emoji keywords, default logo>
COLLECTION_PACK_BASE=<your general collection's --base>
COIN_PACK_BASE=<your coin set-name base>
COIN_PACK_TITLE=<your coin set title>
EMOJI_ARCHIVE_DIR=<where full packs are archived>
COIN_EMOJI_DIR=<where the coin logo PNGs live>
```

That is the complete set of variables any code reads, plus two environment-only
switches used for testing: `TELEGRAM_API_BASE` (Bot API endpoint override,
default `https://api.telegram.org`) and `NUMERA_EMOJI_MAPPER_NO_DOTENV=1` (makes
`emojikit.build_pack.load_env()` a no-op; it is read *before* `.env`, so it only works
from the real environment — the test suite sets it there). There is no
`GENERAL_BOT_USERNAME` / `GENERAL_BOT_NAME`: every tool takes the bot's username
from `getMe` at runtime, so a stale copy can never name the wrong bot in a set
name.

External tool: **ffmpeg + ffprobe** on `PATH` are required **only** for video
emoji. Install on Windows: `winget install Gyan.FFmpeg`.

SVG note: SVG rasterizing uses `resvg-py`, a self-contained Rust renderer
shipped as a prebuilt wheel — no system cairo and no build toolchain. It renders
straight to RGBA (gradients included). 3.11 remains the reference runtime; CI
also exercises 3.12 and 3.14.

---

## 4. The launcher (`run.ps1`)

Right-click → *Run with PowerShell*, or `.\run.ps1`. It shows a centered banner
(pink title + full-width rule + yellow `Logging to: ...`), quietly prepares
`.venv` (env/Python/ffmpeg OK lines go to the **log only**, keeping the console
clean), then shows a **colored, sectioned** menu. Sections are lettered in order
(**A** = Build, **B** = Collection, **C** = Bot, **D** = Maintenance) and each
has its own numbering, so keys stay unique — type e.g. `A1`, `B3`, `D1`:

```
                              Numera Emoji Mapper
====================================================================
Logging to: logs\run_2026-07-03_12-29-45_UTC.log

Build a single pack
  A1) Build a general emoji pack  (new bot)
  A2) Convert images to 100x100 PNGs only
  A3) Crypto-coin pack rebuild    (coin bot)

Collection (multi-format, duplicate-proof)
  B0) Collect specific emoji by id (recommended start)
  B1) Collect emoji from existing packs (download)
  B2) Add media from a folder (build from scratch)
  B3) Publish the collection into new packs
  B4) Open web panel to pick & reorder emoji (browser)
  B5) Reorder a live pack to match the panel (report, then apply)

Bot
  C1) Run the Numera Emoji Mapper bot (premium-emoji ID extractor)

Maintenance
  D1) Run the project checks (byte-compile + unit tests)
  D2) Status: is everything current? (offline)
  D3) Check / refresh the pack roster (packs/)
  D4) Check / sync the pack archive

Select {quit=exit}:
```

The menu follows the recommended path (Appendix D): **B0** collects by id
(`fetch_emoji_ids`), **B4** curates, **B3** publishes, **B5** reorders a live
pack (`sync_order`: a report first, then `--apply` only on a `[y/N]` yes).
**B3** keeps an existing family's mode — it reads `collection\publish_<base>.json`
— and for a new base asks "Publish as ONE mixed family (recommended)?", passing
`--mixed` on yes; a family cannot switch mode once started. **D3**/**D4** run
the roster/archive `--check` and offer `--refresh`/`--sync` only when stale;
**D2** is `.\native\runtime\numera-emoji.exe status`. The actions live in
`scripts\run-actions.ps1`, which `run.ps1` dot-sources.

Navigation follows the FFmWiz style: every input prompt shows a colored
`{back=0, quit=exit}` hint. Typing **0** steps back **one** prompt (e.g. from the
token prompt back to the title prompt); from the first prompt it returns to the
menu. Typing **exit** (or **quit**) leaves the launcher from anywhere. Multi-step
actions are wizards (`Run-Wizard`): each step can go back, re-ask on bad input,
or advance. Screen titles are cyan text (ANSI 256-colour; renders in Windows
Terminal / PowerShell 7). Each native or retained coin command and its exit code are
recorded in the launcher log.

Every run writes a UTC log to `logs\run_<YYYY-MM-DD_HH-mm-ss>_UTC.log` (startup,
prereq checks, menu selections, actions, warnings/errors, shutdown — no secret
values). Non-interactive health check (CI / scripts): `.\run.ps1 -Check`. It
fails (exit 1) on a missing `.venv`, a Python below 3.11 or a missing core
codec dependency or installed native executable, and warns -- by key NAME, never a value -- for every key in
`.env.example` that `.env` leaves unset or empty, with what the key is for.
Unset keys are warnings: not every workflow needs every key. When the launcher
has to create `.venv`, it only picks an interpreter that reports 3.11 or newer.
---

## 5. The collector model (how dedup, mapping & curation work)

The collector stores everything in a **content-addressed catalog**
(`collection/catalog.db`, SQLite, managed by `emojikit/catalog.py`). Each emoji
is one row keyed by a normalized **content hash**.

Guarantees (root-cause fixes — do not regress these):

- **No duplicates**: identical media (same normalized pixels for static; sampled
  frames for video; canonical Lottie JSON for animated) collapse to one row.
  Telegram `file_unique_id` is also remembered so an already-ingested sticker is
  never re-downloaded. **Perceptual near-duplicate merging is OFF by default**
  (it once wrongly merged distinct-but-similar emoji); enable it only with
  `--phash-threshold N` (Hamming distance) when you actually want look-alikes
  merged.
- **No blank emoji**: a blank/transparent render is never uploaded. A blank SVG
  render falls back to the raster source.
- **No mapping drift**: publishing records the *actual* upload order and marks
  each row uploaded (committed per item), so resuming after a skip/crash can
  never re-upload or scramble IDs.
- **No double-upload on network failures**: `addStickerToSet` /
  `createNewStickerSet` are NOT idempotent, so a timeout after Telegram already
  applied the call is never blindly re-sent. `Telegram._call` verifies the live
  set first (applied → success; not applied → safe retry; unknown →
  `AmbiguousUploadError`), and `build_collection` reconciles every unrecorded
  live sticker back to its catalog item (by `file_unique_id`, else by
  downloaded content) before computing what is still pending. Upload payloads
  are sent as bytes so a retried request re-sends the full file.
- **Published copies are remembered**: after publishing, each uploaded copy's
  `file_unique_id` is recorded in `seen_files`, so fetching your own published
  packs (or ids inside them) never downloads anything again.
- **Idempotent / resumable**: re-running fetch or publish is safe and cheap.
- **Curation**: each row has an `included` flag (default 1). The Curate panel
  toggles it; `build_collection` only publishes `included` rows.

`emojikit/media.py` cheat-sheet: `detect_format`, `to_static_png`,
`to_video_webm` (ffmpeg, VP9, ≤256 KB), `to_animated_tgs` (Lottie→gzip),
`validate_video`, `validate_tgs`, `reencode_in_place`. The Lottie/.tgs half
lives in `emojikit/media_lottie.py` and is re-exported from `media`.

`perceptual_hash` premultiplies by alpha before reducing to grey, because
`convert("L")` on RGBA discards alpha and the RGB under a transparent pixel
is whatever the last encoder left there.

`emojikit/identity.py` cheat-sheet: `content_key` (the catalog's primary
key), `perceptual_hash`, `fingerprint` (both in one pass), `same_image`
(did OUR file produce THAT sticker? `None` when undecidable), `hamming`.
It imports `media`; `media` never imports it.

---

## 6. Collector workflows (0 → 100)

### 6.1 Download an existing pack into the catalog

```powershell
# By pack short-name or t.me/addemoji/<name> link. Use the bot that can read it.
.venv\Scripts\.\native\runtime\numera-emoji.exe fetch-pack <pack_or_link> [<pack2> ...] `
    --token-env GENERAL_BOT_TOKEN [--data-dir collection] [--limit N] [--phash-threshold -1]
```

Or name the pack by one of its emoji — an id fetches the whole pack it belongs to.

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe fetch-pack 5283254221590787816 --token-env GENERAL_BOT_TOKEN
```

### 6.2 Build emoji from your own files

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe add-media --in input\myset --emoji 😀 [--as auto|static|video|animated]
# auto: still image -> static; animated GIF/MP4/WEBM -> video; Lottie .json/.tgs -> animated
# (animated emoji are VECTOR only; a GIF/video becomes a VIDEO emoji, not animated)
```

### 6.3 Curate — pick what to publish (web panel)

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe panel [--data-dir collection] [--port 9450] [--preview-fps 15] [--no-open]
                                   [--with-pack N ...] [--all]
```

**An emoji already live in a pack is HIDDEN by default**, so a bare `panel`
shows the next pack's candidates and nothing else. `--with-pack N` un-hides one
published set so it can be rearranged beside them; repeat the flag per pack.
`--all` brings back every finished pack, which is usually hundreds of cards you
cannot act on. `run.ps1 -> B4` reads the published sets from the publisher's own
state file and offers them (`[Y/n]`, Enter = yes) rather than leaving the grid
looking empty.

Dark neon panel: every emoji is a big labelled card (static=image,
video=`<video>`, animated `.tgs`=pre-rendered to animated WebP). All selected by
default. Click to toggle, **Shift+click** for a range. Look-alikes are ordered
adjacently. **Zoom** with `−` / `+`, Ctrl+wheel, or type an exact percentage
into the field between them (Enter applies it, double-click resets to 100%);
the grid is virtual, so a thousand cards cost what a hundred do. Click
**Save** → writes the `included` flag to the catalog.

The header's switches are remembered across reloads and undoable:
**Animation** (off holds every card on its first frame), **All visible** (by
default at most 24 cards animate, the ones nearest the centre of the screen; on,
every visible card animates — heavier), and **Real video** (video cards are
light animated WebP previews by default; on, they are real `<video>` players).
**Import draft**, next to **Reset all**, reads a draft exported earlier: keys
this catalog still holds take the draft's order, inclusion, pack and hold,
keys it no longer holds are dropped, and emoji added since the draft keep their
state at the end. It never posts — review, then Save — and it is one undo step;
the toast reports how many were kept, not in this catalog, and new.

**Selection mode** (the pill next to Zoom) is for moving several emoji as one
group. In the holding tray, click a card anywhere to pick it and Shift-click
another to take the run between them; in the grid, drag across the small pick
box in a card's top-left corner to select a
run, then drag any picked card to carry the whole set. While it is on, Select
all / Deselect all / Invert act on the picks instead of on publish inclusion.
**Holding** (the strip under the count line) is a place to park emoji out of
the way: drag one onto it to exclude it from the next publish without hunting
for its tick in a long grid, drag a parked one back in to re-include it at
the exact position dropped — a pack short a few emoji still auto-fills from
whatever candidates follow it, same as unticking one in place always has.

### 6.4 Publish the catalog into new packs

```powershell
# Preview (no upload):
.venv\Scripts\.\native\runtime\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN --dry-run
# Publish for real (resumable, duplicate-proof, per-format sets):
.venv\Scripts\.\native\runtime\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN [--formats static,video,animated] [--per-set 200]
```

Sets are named `<base>s<n>_by_<bot>` (static), `<base>v<n>` (video),
`<base>a<n>` (animated) — split by format for organization (since Bot API 7.2 a
set *may* mix formats, so this is a choice, not a requirement). Each finished
pack's `t.me/addemoji/...` link goes to `PACK_LINKS_CHAT_ID` (or the owner's
private chat when that is unset), directly or through the Worker, and a per-pack manifest
(`collection/manifests/<set>.md`: name + emoji ID) is written. The manifest
counts the **pack**, not the catalog rows: when a brand logo leads the set it is
row 1 and the catalog items follow from 2, because the logo is a sticker in the
pack even though it is not a catalog item. This table, the archive's
`_manifest.md`, the coin manifests and the `packs/` roster are all rendered by
`emojikit/pack_rows.py`: every keyword is listed, and a `|` or a line break in a
label is escaped rather than breaking the table.

---

## 7. Crypto-coin component (`coins/`)

Self-contained tool that reuses `build-pack` and the coin bot.

```powershell
# Logos (data): fetch + keywords
.venv\Scripts\python.exe -m coins.fetch_logos          # CoinGecko logos + keywords.csv
.venv\Scripts\python.exe -m coins.fetch_paprika --dry  # resolve + report only, nothing published
.venv\Scripts\python.exe -m coins.fetch_paprika        # fill from CoinPaprika
.venv\Scripts\python.exe -m coins.fetch_cmc            # fill from CoinMarketCap (needs CMC_API_KEY)
.venv\Scripts\python.exe -m coins.build_keywords       # (re)build keywords.csv from logos

# Convert logos to 100x100 emoji PNGs
.venv\Scripts\.\native\runtime\numera-emoji.exe make-emoji-pngs --in coins\logos\svg --out coins\logos\emoji
.venv\Scripts\.\native\runtime\numera-emoji.exe make-emoji-pngs --in coins\logos\png --out coins\logos\emoji

# Build / rebuild the packs (duplicate-proof, records upload order)
.venv\Scripts\python.exe -m coins.rebuild_dedup        # all: DELETES the packs in rebuild_state.json, then build + map + links
.venv\Scripts\python.exe -m coins.rebuild_dedup map    # only rebuild the id map + inventory
.venv\Scripts\python.exe -m coins.rebuild_dedup links  # resend the combined links message
```

### 7.1 The ticker → custom_emoji_id map (`coins/ticker_to_id.json`)

This is the lookup table consumers use. **Always derive it from image content,
not from positions** (a historical position-based bug scrambled it):

```powershell
# 1) Calibrate: run WITHOUT --apply and pick a cutoff from the reported distances.
.venv\Scripts\python.exe -m coins.remap_ids --emoji-dir "PATH\to\emoji"
# 2) Apply. --max-distance is REQUIRED with --apply (must be > 0): an uncalibrated
#    run would accept a nearest-but-wrong match and overwrite the map with it.
#    --min-margin (default: --max-distance) additionally rejects a match whose
#    runner-up is nearly as close. A refused --apply writes ticker_to_id.candidate.json
#    instead, for review.
.venv\Scripts\python.exe -m coins.remap_ids --emoji-dir "PATH\to\emoji" --max-distance 200 --apply

# Fill chain-variant / alias tickers (etharb->eth, bnbbsc->bnb, usdc.e, ...):
.venv\Scripts\python.exe -m coins.enhance_map          # strip chain suffixes -> base id
.venv\Scripts\python.exe -m coins.alias_map            # match by coin NAME -> base id
```

### 7.2 Audit, fix logos, manifests

```powershell
# Audit ALL packs for blank/duplicate stickers (downloads every sticker):
.venv\Scripts\python.exe -m coins.check_all_packs

# Review logos vs official CoinGecko art; fix ONLY confirmed-wrong tickers:
.venv\Scripts\python.exe -m coins.verify_logos --emoji-dir "PATH\to\emoji"            # report
.venv\Scripts\python.exe -m coins.verify_logos --emoji-dir "PATH\to\emoji" --fix --only sol,xrp

# Write a per-pack manifest (.md: ticker(s) + emoji id) for every pack:
.venv\Scripts\python.exe -m coins.write_manifests --out-dir "PATH\to\pack-folder"
```

> Logo similarity vs official art is **not** proof a logo is wrong (different
> icon sets differ). `verify_logos --fix` therefore only touches the tickers you
> explicitly list with `--only`.

---

## 8. The Numera Emoji Mapper bot (`emoji-bot`)

Long-polling bot (run it and leave it running; only one instance at a time):

```powershell
.venv\Scripts\.\native\runtime\numera-emoji.exe emoji-bot    # uses GENERAL_BOT_TOKEN; or run.ps1 -> C1
```

- Send it **one or more premium emoji in a row** (spaces/newlines between them
  don't matter) → it replies with a **single collapsed (expandable) quote** of
  `emoji + ID` (each line shows the **actual premium emoji** via a `<tg-emoji>`
  custom-emoji entity next to `<code>id</code>`; tap an ID to copy just that one
  on mobile), plus one or more **“Copy”/“Copy a-b” inline buttons** (`copy_text`)
  covering that message's IDs (each button is capped at 256 chars, ~12 IDs, by
  Telegram itself). As many IDs as fit in one message (~4096-char Telegram limit)
  are batched together, so a 50-id result is ~2 messages, not one-message-per-12.
  (The quote is collapsed by height, so long lists show a few lines until
  expanded — expected, not missing data.)
- Send/forward a **post with premium emoji** → same two-format reply.
- **Add it to a channel/group** (as admin) → DMs the owner the premium-emoji IDs
  from *new* posts (Bot API cannot read past channel history).
- `/start`, `/help` show the menu (registered via `setMyCommands`).

Copy-all is reliable via the “Copy all” inline `copy_text` button; per-ID
tap-to-copy uses Telegram's native `<code>` copy (mobile). Very large lists are
split across multiple messages (each under 4096 chars).

---

## 9. Telegram limits to remember

- Custom-emoji **set cap: 200**. Since Bot API 7.2 one set may mix formats
  (`build_collection --mixed`); one set per format is the default.
- **Animated = vector (Lottie/TGS) only**, on a 512×512 canvas. Raster
  animation → **video** emoji.
- Video: VP9, 100×100, ≤3 s, 30 fps, no audio, ≤256 KB.
- A bot can `getStickerSet`/`getFile` for any pack it can see, but **cannot read
  channel history** — only posts received after it joined.
- Sending custom emoji in messages and reading their keywords back is limited:
  the Bot API exposes `custom_emoji_id`/`emoji`/`set_name` (via
  `getCustomEmojiStickers`) but **not** a sticker's search keywords.

---

## 10. Testing, CI, and Git

```powershell
.venv\Scripts\python.exe -m pip install -r requirements-dev.txt    # once: ruff + playwright, not runtime deps
.venv\Scripts\python.exe -m playwright install chromium            # once: the panel's browser suite
.\scripts\check.ps1                                                # compile + lint + full suite
.venv\Scripts\python.exe -m unittest discover -s tests -t . -p "test_*.py"   # the suite alone
.\run.ps1 -Check                                                   # env doctor (venv/deps/ffmpeg/.env)
```

**The panel's browser suite needs a browser.** `tests/test_panel_browser.py`
drives the real page in headless Chromium, because that is the only level at
which the client-side behaviour exists — a save pipeline that drops the newest
edit is perfectly well-formed JavaScript, and a source-text assertion cannot
tell it from a correct one. A missing playwright or Chromium is a **hard error,
never a skip**: a browser test that reports green on a machine with no browser
is worse than no browser test at all. Set `NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` to
opt out deliberately, and know that you did.

`scripts\check.ps1` is the single command CI and a developer both run, so the two
cannot drift into different invocations. Three stages, in order: `compileall`,
`ruff check .`, then the suite — the cheap gates first, so a syntax or lint
error fails in seconds instead of after ~75 s of tests. It resolves the repo
root from its own location (any cwd, spaces in the path are fine), prefers the
repo `.venv` for both Python and ruff, bounds each step with a wall-clock
ceiling (`-TimeoutSeconds`, default 1800; a step that exceeds it is killed and
reported as exit 124), and returns a real exit code. `-Python <path>` overrides
the interpreter. It is also `run.ps1` menu entry **D1**.

**Lint.** `ruff check .` takes no arguments on purpose: `ruff.toml` at the repo
root owns the rule set and the exclusions, so a `--select` on one command line
is all it takes for CI and a local run to start linting different things. The
set is deliberately narrow — ruff's default rules plus `E402`, `BLE001`, `B` and
`RUF100`. The reason is historical: the source already carried ~120
`# noqa: E402` / `# noqa: BLE001` comments written against a linter that was
never configured, so they suppressed nothing and were never checked. Enabling
exactly the codes they name is what makes them meaningful, and `RUF100` (unused
`noqa`) is what stops them rotting back into decoration. Never make the stage
green with `--exit-zero` or `continue-on-error`.

**`-t .` is required, not cosmetic.** Without it the tests directory becomes the
top level, modules load as `test_x` instead of `tests.test_x`, and
`tests/__init__.py` — which scrubs every credential-shaped variable out of the
environment and refuses non-loopback sockets — never runs.
`tests.test_entry_points.SuiteIsHermetic` fails loudly when the suite is started
without it. Details in [`tests/README.md`](../tests/README.md).

**The Worker suite runs in CI, in its own job.** `worker/` is TypeScript with
its own vitest suite. It is a separate `worker:` job rather than a step in
`build:` because it shares nothing with the Python matrix — it needs Node, not
Python and ffmpeg — and running it once per Python version would be pure waste.
A separate job also makes a Worker failure legible as a Worker failure.

**So does the panel's browser suite**, in `panel-browser:`, for the same reason:
it exercises JavaScript, so running it in the matrix would download Chromium
once per Python version to prove the same thing. The matrix sets
`NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` explicitly — an opt-out that is written down
is the only kind this module accepts.

`check.ps1` stays Python-only: it is the command a developer runs constantly,
and requiring a Node toolchain for it would tax everyone who never touches
`worker/`. Run the Worker's own checks after changing it:

```bash
cd worker && npm ci && npm run typecheck && npx vitest run
```

CI uses `npm ci`, never `npm install`: it installs exactly the committed
lockfile and fails if `package.json` and the lock disagree, so CI cannot quietly
test a different dependency tree than the one committed. `dependabot.yml` has an
`npm` entry for `/worker` so that tree gets updates like every other one.

CI (`.github/workflows/ci.yml`) has four jobs. `build` (Python 3.11, 3.12
**and** 3.14) installs both dependency manifests, the pinned ruff and ffmpeg,
runs `scripts/check.ps1` (compile + lint + suite, `shell: pwsh`), then an
offline `build_pack` dry-run. `panel-browser` runs the browser suites in
headless Chromium, `worker` type-checks and tests the Worker and type-checks the
panel scripts, and `windows-safety` parses every PowerShell script and runs the
native-Windows lock, migration and persistence suites. Run `scripts\check.ps1`
locally before pushing.

Git: work is committed in small logical commits and pushed to `main` on
`KiaroSama/Numera-Emoji-Mapper`. Never commit `.env`, `secrets.md`, `collection/`,
`logs/`, or any token.

---

## 11. Next steps / how to extend (for a developer or AI agent)

1. **Read this guide + `README.md` first**, then the file you intend to change.
2. **Reuse the catalog + emojikit** for any new media handling. Do not add a
   parallel dedup/mapping mechanism — the catalog is the single source of truth.
3. **Preserve the guarantees in §5** (no duplicates, no blanks, no mapping
   drift, idempotent). If you add a format or ingest path, route it through
   `catalog.add(...)` with a proper `content_key` and a blank check.
4. **Publishing**: keep the per-item committed `uploaded` flag + recorded upload
   order; never reintroduce position-offset resume logic. Never blind-retry a
   non-idempotent Bot API call: go through `Telegram.add_emoji`/`add_sticker`
   with `expected_before=<live count>` (verified retry) and let
   `emojikit.build_collection.reconcile_set` attribute anything ambiguous from the live
   set before uploading more.
5. **Curation**: respect the `included` flag in any new publish path.
6. **UI changes** (panel): keep the dark neon-blue style, Inter font, visible
   focus, `prefers-reduced-motion`, lazy media (IntersectionObserver) so large
   catalogs stay fast. Verify in a real browser before claiming done.
7. **External libs/APIs**: check current docs before coding (the Telegram Bot
   API and any JS player evolve).
8. **Always**: add/maintain tests, run `.\scripts\check.ps1` (never a hand-rolled
   `unittest` invocation — see §10) plus a real run, then commit and **push to
   keep GitHub in sync**, and **update this guide** with any new command 0 → 100.

---

*Keep this document synchronized with the code. When you add or change a
command, add it here in full.*
---
