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

```text
Numera Emoji Mapper/
  numera-emoji.exe           packaged Windows executable (numera-emoji on Linux/macOS)
  run.ps1 / run.sh           thin launchers for an existing executable
  runtime/                  standalone production Rust crate, codecs and package tooling
  native/src/               shared Rust catalog, transport, workflow and persistence modules
  native/Cargo.toml         optional numera-compat developer/oracle harness
  assets/                   panel HTML and browser JavaScript
  lib/                      packaged native libraries when required by the platform
  worker/                   Cloudflare Worker TypeScript and its tests
  tests/                    independent fixtures and compatibility tests
  runtime/tests/            standalone Rust integration tests
  docs/                     operator and contributor references
  .env.example              configuration template
  collection/               local catalog.db + media/ + manifests/ + state
  logs/ build/ input/ logos/ local generated or working data
```

The package executable, assets and native libraries stay together. Generated
operator data, `.env`, `secrets.md`, caches, recovery journals and working folders
are not source files to commit. Historical `emojikit/`, `coins/*.py` and oracle
helpers are compatibility/developer material, not application launch paths.

---

## 3. Setup (one time)

Download the matching standalone package from a trusted release or successful
**Prebuilt packages** workflow and extract it into a fresh directory. Normal
use needs no Python, pip, NumPy, virtual environment or Rust installation.
Keep the packaged assets and libraries beside the root executable.

```powershell
Copy-Item .env.example .env       # then edit tokens, owner and data paths
.\numera-emoji.exe package-verify
.\numera-emoji.exe doctor
.\run.ps1
```

On Linux/macOS use `./numera-emoji package-verify`, `./numera-emoji doctor` and
`./run.sh`. A source checkout without `package.json` can run the native doctor;
package verification requires a generated standalone package.

An explicit offline copy is available with
`numera-emoji install-prebuilt --destination DIR`. It verifies the source
package and refuses an occupied or linked destination. Updates use a fresh
directory: retain configuration/data and verify the new application before
removing the old one. See [launcher and packaging](native-launcher.md).

### Native ownership and rollback

Rust owns the catalog, transport/retries, state/locks, collectors/publishers,
bot, HTTP panel, media worker, raster/SVG/Lottie codecs and coin matching.
The browser UI remains JavaScript and the Cloudflare Worker remains TypeScript.
FFmpeg/FFprobe are external tools for video. Neither launchers nor the local
media/backend runtime start a Python interpreter.

Contributors explicitly build with
`cargo build --manifest-path runtime/Cargo.toml --locked --release`; see
[contributor requirements](native-launcher.md#transitional-source-check-orchestration).
The menu never compiles or installs dependencies. The optional `numera-compat`
harness has separate historical oracle dependencies and is not needed to run
an operator package.

Persisted schemas, keys and identifiers retain their compatibility contracts.
For rollback, keep the prior application separately and use verified recovery
bundles; never discard operator data or unresolved migration journals.

`.env` keys:

```
PACK_OWNER_USER_ID=<your numeric Telegram id>   # owns every created set; press Start on each bot once
TELEGRAM_BOT_TOKEN=<coin bot token>
GENERAL_BOT_TOKEN=<general bot token>
BOT_ALLOWED_USER_IDS=<optional: extra ids allowed to use emoji-bot>
BOT_ALLOWED_CHANNEL_IDS=<optional: channel ids (-100...) whose posts the bots answer; empty = none>
PACK_LINKS_CHAT_ID=<optional: channel that receives finished-pack links>
WORKER_PUBLISH_URL=<optional: https://<worker>.workers.dev/publish — see §12.9>
WORKER_PUBLISH_SECRET=<optional: bearer for that endpoint; both or neither>
EMOJI_LOG_RETENTION_DAYS=30    # optional: prune logs/ older than N days (0 = keep all)
EMOJI_FFMPEG_TIMEOUT=300       # optional: seconds per ffmpeg/ffprobe child
CMC_API_KEY=<optional CoinMarketCap key, for coin-fetch-cmc>
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

Native commands and the panel resolve log retention after loading configuration.
Only direct regular log files are eligible; symlink/reparse entries are retained.
Test harnesses can set `NUMERA_EMOJI_MAPPER_NO_DOTENV=1` before startup to suppress
`.env` loading and use isolated endpoint configuration. These are developer
controls, not normal installation requirements. There is no
`GENERAL_BOT_USERNAME` / `GENERAL_BOT_NAME`: every tool takes the bot's username
from `getMe` at runtime, so a stale copy can never name the wrong bot in a set
name.

External tool: **ffmpeg + ffprobe** on `PATH` are required **only** for video
emoji. Install on Windows: `winget install Gyan.FFmpeg`.

SVG rasterization uses the bundled Rust resvg renderer, including gradients and
alpha. There is no system Cairo or Python extension to install.

---

## 4. The launcher (`run.ps1`)

Run `.\run.ps1` on Windows or `./run.sh` on Linux/macOS to open the native menu.
Both launch an existing executable and preserve its exit code. With no packaged
root executable, source-checkout launchers can use an explicitly built
`runtime/target/release` or `runtime/target/debug` executable. A missing executable
is an error; there is no automatic environment setup, dependency installation or
Cargo build.

The menu groups the existing workflows:

| Group | Actions |
|---|---|
| Build | General pack, image conversion, coin pack rebuild |
| Collection | Collect IDs/packs, add media, publish, curate panel, report/apply live order |
| Bot | Run the custom-emoji ID extractor |
| Maintenance | Native environment check, offline status, roster and archive checks |

The usual path is collect → panel → Save → publish dry run → preflight → publish.
Publishing keeps an existing family's mixed/split mode. Report/apply actions retain
their own confirmation and validation; opening the menu does not publish anything.

Prompts support `0` to return one step and `exit`/`quit` to leave. Foreground
commands remain attached to the terminal so cancellation reaches their owned
processes. Native commands create redacted UTC logs beneath `logs/`.

For non-interactive diagnostics use `.\run.ps1 -Check`, `./run.sh --check` or
`numera-emoji doctor`. The doctor checks native codec roundtrips, assets and
package integrity and reports missing video tools or unset configuration keys.
It never imports Python packages or contacts Telegram. The maintenance `check`
command is the same doctor, not the compatibility test suite.

---

## 5. The collector model (how dedup, mapping & curation work)

The collector stores everything in a **content-addressed catalog**
(`collection/catalog.db`, SQLite, managed by the native catalog). Each emoji
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
  applied the call is never blindly re-sent. The native Telegram client verifies the live
  set first (applied → success; not applied → safe retry; unknown →
  `AmbiguousUploadError`), and `build-collection` reconciles every unrecorded
  live sticker back to its catalog item (by `file_unique_id`, else by
  downloaded content) before computing what is still pending. Upload payloads
  are sent as bytes so a retried request re-sends the full file.
- **Published copies are remembered**: after publishing, each uploaded copy's
  `file_unique_id` is recorded in `seen_files`, so fetching your own published
  packs (or ids inside them) never downloads anything again.
- **Idempotent / resumable**: re-running fetch or publish is safe and cheap.
- **Curation**: each row has an `included` flag (default 1). The Curate panel
  toggles it; `build-collection` only publishes `included` rows.

The native media worker owns detection, fitting, conversion, validation,
reencoding and comparison. Static images use normalized RGBA identity; video
uses the frozen alpha-aware sample stream; animated identity uses canonical
Lottie JSON. See [codec contracts](native-codec-contracts.md).

For RGBA dHash, color is premultiplied by alpha before conversion to luminance,
so invisible RGB cannot become visual evidence. Fingerprints retain the original
source-mode distinction; provider identity explicitly converts to RGBA.

---

## 6. Collector workflows (0 → 100)

### 6.1 Download an existing pack into the catalog

```powershell
# By pack short-name or t.me/addemoji/<name> link. Use the bot that can read it.
.\numera-emoji.exe fetch-pack <pack_or_link> [<pack2> ...] `
    --token-env GENERAL_BOT_TOKEN [--data-dir collection] [--limit N] [--phash-threshold -1]
```

Or name the pack by one of its emoji — an id fetches the whole pack it belongs to.

```powershell
.\numera-emoji.exe fetch-pack 5283254221590787816 --token-env GENERAL_BOT_TOKEN
```

### 6.2 Build emoji from your own files

```powershell
.\numera-emoji.exe add-media --in input\myset --emoji 😀 [--as auto|static|video|animated]
# auto: still image -> static; animated GIF/MP4/WEBM -> video; Lottie .json/.tgs -> animated
# (animated emoji are VECTOR only; a GIF/video becomes a VIDEO emoji, not animated)
```

### 6.3 Curate — pick what to publish (web panel)

```powershell
.\numera-emoji.exe panel [--data-dir collection] [--port 9450] [--preview-fps 15] [--no-open]
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
.\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN --dry-run
# Publish for real (resumable, duplicate-proof, per-format sets):
.\numera-emoji.exe build-collection --base mypack --title "My Pack" `
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
the native pack-row renderer: every keyword is listed, and a `|` or a line break in a
label is escaped rather than breaking the table.

---

## 7. Crypto-coin component (`coins/`)

Self-contained tool that reuses `build-pack` and the coin bot.

```powershell
# Logos (data): fetch + keywords
.\numera-emoji.exe coin-fetch-logos          # CoinGecko logos + keywords.csv
.\numera-emoji.exe coin-fetch-paprika --dry  # resolve + report only, nothing published
.\numera-emoji.exe coin-fetch-paprika        # fill from CoinPaprika
.\numera-emoji.exe coin-fetch-cmc            # fill from CoinMarketCap (needs CMC_API_KEY)
.\numera-emoji.exe coin-build-keywords       # (re)build keywords.csv from logos

# Convert logos to 100x100 emoji PNGs
.\numera-emoji.exe make-emoji-pngs --in coins\logos\svg --out coins\logos\emoji
.\numera-emoji.exe make-emoji-pngs --in coins\logos\png --out coins\logos\emoji

# Build / rebuild the packs (duplicate-proof, records upload order)
.\numera-emoji.exe coin-rebuild        # all: DELETES the packs in rebuild_state.json, then build + map + links
.\numera-emoji.exe coin-rebuild map    # only rebuild the id map + inventory
.\numera-emoji.exe coin-rebuild links  # resend the combined links message
```

### 7.1 The ticker → custom_emoji_id map (`coins/ticker_to_id.json`)

This is the lookup table consumers use. **Always derive it from image content,
not from positions** (a historical position-based bug scrambled it):

```powershell
# 1) Calibrate: run WITHOUT --apply and pick a cutoff from the reported distances.
.\numera-emoji.exe coin-remap-ids --emoji-dir "PATH\to\emoji"
# 2) Apply. --max-distance is REQUIRED with --apply (must be > 0): an uncalibrated
#    run would accept a nearest-but-wrong match and overwrite the map with it.
#    --min-margin (default: --max-distance) additionally rejects a match whose
#    runner-up is nearly as close. A refused --apply writes ticker_to_id.candidate.json
#    instead, for review.
.\numera-emoji.exe coin-remap-ids --emoji-dir "PATH\to\emoji" --max-distance 200 --apply

# Fill chain-variant / alias tickers (etharb->eth, bnbbsc->bnb, usdc.e, ...):
.\numera-emoji.exe coin-enhance-map          # strip chain suffixes -> base id
.\numera-emoji.exe coin-alias-map            # match by coin NAME -> base id
```

### 7.2 Audit, fix logos, manifests

```powershell
# Audit ALL packs for blank/duplicate stickers (downloads every sticker):
.\numera-emoji.exe coin-check-packs

# Review logos vs official CoinGecko art; fix ONLY confirmed-wrong tickers:
.\numera-emoji.exe coin-verify-logos --emoji-dir "PATH\to\emoji"            # report
.\numera-emoji.exe coin-verify-logos --emoji-dir "PATH\to\emoji" --fix --only sol,xrp

# Write a per-pack manifest (.md: ticker(s) + emoji id) for every pack:
.\numera-emoji.exe coin-write-manifests --out-dir "PATH\to\pack-folder"
```

> Logo similarity vs official art is **not** proof a logo is wrong (different
> icon sets differ). `coin-verify-logos --fix` therefore only touches the tickers you
> explicitly list with `--only`.

---

## 8. The Numera Emoji Mapper bot (`emoji-bot`)

Long-polling bot (run it and leave it running; only one instance at a time):

```powershell
.\numera-emoji.exe emoji-bot    # uses GENERAL_BOT_TOKEN; or run.ps1 -> C1
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
  (`build-collection --mixed`); one set per format is the default.
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

Operator diagnostics and contributor tests are separate:

```powershell
.\numera-emoji.exe doctor
cargo test --manifest-path runtime/Cargo.toml --locked
```

The first command needs only the installed application. The second is for a
source checkout with the contributor toolchain. It tests the standalone backend,
codec fixtures, package behavior and native workflow contracts. See
[native test migration](native-test-migration.md) and the current CI workflows
for platform-specific checks and recorded evidence.

Historical Python tests, reference implementations and API fault-injection tests
remain in the optional `numera-compat` developer harness. Their Python, Ruff,
Playwright and oracle dependencies do not become production prerequisites.
When running that retained suite, use its documented command and preserve
credential scrubbing and loopback-only test isolation. For Python discovery,
`-t .` remains essential so `tests/__init__.py` runs.

Browser behavior needs a real browser: source-text checks cannot establish save,
drag, keyboard or animation behavior. The panel's browser suite and the
Cloudflare Worker's TypeScript tests remain separate from application runtime
requirements. A missing browser must be reported rather than counted as a
successful browser run.

Worker contributor checks:

```bash
cd worker
npm ci
npm run typecheck
npx vitest run
```

Use the committed lockfile. Keep work in reviewable commits and follow the
repository's branch/review workflow. Never commit `.env`, `secrets.md`,
`collection/`, `logs/` or credentials. A local test run is evidence for that run,
not a claim that all package platforms or live Telegram workflows passed.

---

## 11. Next steps / how to extend (for a developer or AI agent)

1. Read this guide, `README.md` and the implementation you intend to change.
2. Reuse the Rust catalog and media worker; do not add another dedup or mapping
   backend. Route new ingest paths through content identity and blank admission.
3. Preserve per-item committed upload state and actual recorded order. Reconcile
   ambiguous live outcomes before retrying a non-idempotent Telegram mutation.
4. Respect inclusion, ordering, pack-family locks and recovery journals.
5. Keep browser behavior accessible and bounded, including the default 24-card
   animation budget. Verify UI changes in a real browser.
6. Run the relevant native, compatibility, browser or Worker checks and report
   which were executed. Update the command/reference documentation with changes.

*Keep this document synchronized with the code. When you add or change a
command, add it here in full.*
---
