<div align="center">

<img src="assets/numera-emoji-mapper-logo-circle.png" alt="Numera Emoji Mapper" width="420">

# Numera Emoji Mapper

**Build Telegram premium custom-emoji packs from any collection of images.**

[![CI](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/github/license/KiaroSama/Numera-Emoji-Mapper)](LICENSE)
[![Runtime: Rust](https://img.shields.io/badge/runtime-Rust-DEA584?logo=rust)](runtime/Cargo.toml)
[![Rust 1.99+](https://img.shields.io/badge/Rust-1.99%2B-DEA584?logo=rust)](runtime/Cargo.toml)
[![Builds: Linux, macOS, Windows](https://img.shields.io/badge/builds-Linux%20%7C%20macOS%20%7C%20Windows-0078D6)](.github/workflows/ci.yml)
<br>
[![Backend: Cloudflare Workers](https://img.shields.io/badge/backend-Cloudflare%20Workers-F38020?logo=cloudflare&logoColor=white)](worker/README.md)
[![Telegram Bot API](https://img.shields.io/badge/Telegram-Bot%20API-26A5E4?logo=telegram&logoColor=white)](https://core.telegram.org/bots/api)
[![Docs: GUIDE](https://img.shields.io/badge/docs-GUIDE-blue)](docs/GUIDE.md)
[![Last commit](https://img.shields.io/github/last-commit/KiaroSama/Numera-Emoji-Mapper)](https://github.com/KiaroSama/Numera-Emoji-Mapper/commits/main)
<br>
[![Commit activity](https://img.shields.io/github/commit-activity/m/KiaroSama/Numera-Emoji-Mapper)](https://github.com/KiaroSama/Numera-Emoji-Mapper/commits/main)
[![Top language](https://img.shields.io/github/languages/top/KiaroSama/Numera-Emoji-Mapper)](https://github.com/KiaroSama/Numera-Emoji-Mapper)
[![Repo size](https://img.shields.io/github/repo-size/KiaroSama/Numera-Emoji-Mapper)](https://github.com/KiaroSama/Numera-Emoji-Mapper)
[![Native media codecs](https://img.shields.io/badge/media-native%20codecs-007808)](runtime/Cargo.toml)
[![FFmpeg for video emoji](https://img.shields.io/badge/FFmpeg-video%20emoji-007808?logo=ffmpeg&logoColor=white)](docs/GUIDE.md)
[![SQLite catalog](https://img.shields.io/badge/SQLite-catalog-003B57?logo=sqlite&logoColor=white)](docs/GUIDE.md)
[![Worker in TypeScript](https://img.shields.io/badge/Worker-TypeScript-3178C6?logo=typescript&logoColor=white)](worker/README.md)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

Numera Emoji Mapper started as a crypto-coin logo packer and is now a general tool: it
takes a folder of images, normalizes each one to the exact 100×100 PNG that
Telegram custom emoji require, and uploads them through a bot into one or more
custom-emoji sets owned by your account.

It ships with two independent workflows that share the same engine:

| | Workflow | Bot | Source | Purpose |
|---|----------|-----|--------|---------|
| <img src="assets/numera-coin-emoji-mapper-logo.png" alt="" width="42"> | **Crypto coins** | `TELEGRAM_BOT_TOKEN` | CoinGecko / CoinPaprika / CoinMarketCap logos | the original coin-logo packs |
| <img src="assets/numera-emoji-mapper-logo.png" alt="" width="42"> | **General** | `GENERAL_BOT_TOKEN` (`@YourEmojiBot`) | any folder of images you provide | any non-coin emoji pack |

The installed Rust commands `make-emoji-pngs` and `build-pack` power both;
only the source folder and selected bot token differ. The Rust CLI, panel and
bounded media worker use native raster, SVG, Lottie and video implementations.
The production dependency graph and ready-built archives require no Python,
Python packages, virtual environment or Rust compiler. FFmpeg/FFprobe remain
external tools for video.

The first-party Rust migration lives on
[`feature/first-party-rust`](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/70).
The production crate is `runtime/`; `native/` retains the PyO3 compatibility and
historical source-oracle harness under the explicit `numera-compat` developer
command. Compatibility wheels and Python regression dependencies stay outside
production downloads. The Cloudflare Worker and browser interface retain their
TypeScript and JavaScript implementations.

CI builds the standalone runtime and runs its native codec and offline package
checks on Linux, Windows, Intel macOS and Apple Silicon macOS. It also retains
the original Python regression matrix, browser suites, Worker checks, native
Windows checks and historical wheel/venv package lifecycle. The guarded test
runners schedule cases serially; concurrency and process-death scenarios remain
inside their existing tests. [Migration and CI status](docs/rust-ci-status.md)
records verification results and remaining platform gates.

> 📘 **[Full Guide (0 → 100) — docs/GUIDE.md](docs/GUIDE.md)** — complete
> reference for every workflow and command, written for humans and AI agents.

## How it works

1. **Prepare PNGs** — `make-emoji-pngs` converts your images (SVG, PNG, JPG,
   WEBP, GIF) into transparent 100×100 PNGs.
2. **Build the pack** — `build-pack` uploads those PNGs into Telegram
   custom-emoji sets (max 200 per set), naming them `<base><n>_by_<botusername>`
   and sending you each finished pack's `https://t.me/addemoji/...` link.

## Requirements

- **Prebuilt packages:** the standalone executable and its bundled assets. No
  Python, virtual environment, Cargo or C++ toolchain is needed to run it.
- **Package targets:** Windows x64, Linux x64 (Ubuntu 24.04+ / glibc 2.39+),
  macOS 15+ Intel and Apple Silicon. Each download must pass its hosted native
  package and media checks. Alpine/musl and other CPU targets are not covered.
- A Telegram bot from [@BotFather](https://t.me/BotFather), and your numeric
  Telegram user id. Press **Start** on the bot once before publishing.
- **ffmpeg + ffprobe** on `PATH` for video emoji (`.webm`). On Windows:
  `winget install Gyan.FFmpeg`. Static images and Lottie do not require them.

## Install a ready-built package

1. Open [Releases](https://github.com/KiaroSama/Numera-Emoji-Mapper/releases), or
   the artifacts on a successful [Prebuilt packages run](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/workflows/release.yml),
   and download the matching `numera-emoji-mapper-<version>-<platform>.zip` plus
   its `.zip.sha256`. GitHub CI artifacts require sign-in. A source ZIP is not
   a ready-built application.
2. Verify the archive checksum and extract it into its own directory. SHA256
   checks detect changed bytes; they do not authenticate the publisher.
3. Run the executable directly. The extracted directory is already runnable;
   there is no dependency installation step.

**Windows:**

```powershell
.\numera-emoji.exe package-verify
.\numera-emoji.exe doctor
.\run.ps1
```

**Linux / macOS:**

```bash
chmod +x numera-emoji run.sh
./numera-emoji package-verify
./numera-emoji doctor
./run.sh
```

`run.ps1 -Check` and `run.sh --check` run the native doctor. Both launchers pass
commands directly to the executable and never install or build prerequisites.
Use `numera-emoji panel` to open the browser panel, or `--help` for commands.
Copy `.env.example` to `.env` and fill in your configuration before contacting
Telegram. Runtime temp/cache files stay in `logs/tmp/` and `.cache/` beside the
application.

Each archive includes [third-party notices](THIRD_PARTY_NOTICES.md) and the
bundled renderer's original license and provenance files under `licenses/rlottie/`.
These files are covered by the package's SHA256 inventory.

An optional `install-prebuilt --destination /absolute/path/to/new-directory`
command copies and verifies the whole package. It uses an exclusive installation
lock and preserves the previous directory if replacement fails. Successful
replacements retain the previous directory for recovery as well: a running
application can still hold file handles into it. An existing
installation containing local data or configuration is refused: extract updates
separately and keep your `.env` and collections backed up. Hash checks and
linked/path traversal refusals apply before installation.

### Developer source build

Production contributors need Rust 1.99+, CMake and a platform C/C++ linker (MSVC
build tools on Windows). Python is unnecessary for this build:

```bash
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
runtime/target/release/numera-emoji --help
```

On Windows, use `runtime\target\release\numera-emoji.exe`. The source launchers
also find this production build automatically. To package it:

```bash
runtime/target/release/numera-emoji package-native --out dist --revision YOUR_COMMIT
```

The optional compatibility toolkit is a separate development build:
`cargo build --manifest-path native/Cargo.toml --locked --bins`. It needs Python
3.11 and the historical requirements for source-oracle and wheel tests; invoke
its developer commands through `native/target/debug/numera-compat`. It is not
part of the production archive or runtime dependency graph.

`.env` keys (see `.env.example`):

```
PACK_OWNER_USER_ID=<your numeric telegram id>
TELEGRAM_BOT_TOKEN=<coin bot token>
GENERAL_BOT_TOKEN=<general bot token>      # @YourEmojiBot
CMC_API_KEY=<optional CoinMarketCap key>
```

Your own identities — which bots lead their packs with your brand logo, the
logo itself, your pack bases and coin pack title — are `.env` keys too
(`BRAND_LOGO_BOTS`, `BRAND_LOGO_PATH`, `COLLECTION_PACK_BASE`, `COIN_PACK_BASE`,
`COIN_PACK_TITLE`, …). The repository names no operator, so none has a default:
a tool that needs one stops and names it. Keep your logo in the git-ignored
`private/` folder.

## Quick start — general emoji pack (new bot)

Run from the extracted package. `./numera-emoji` locates its assets beside the
executable. `run.ps1` and `run.sh` open the native menu; their check options run
the native doctor.

```powershell
# Put your source images in a folder, e.g. input/myset/
.\numera-emoji.exe make-emoji-pngs --in input\myset --out build\myset

# Validate without calling Telegram
.\numera-emoji.exe build-pack --base myset --title "My Emojis" `
    --source-dir build\myset --token-env GENERAL_BOT_TOKEN --emoji "😀" --dry-run

# Build for real
.\numera-emoji.exe build-pack --base myset --title "My Emojis" `
    --source-dir build\myset --token-env GENERAL_BOT_TOKEN --emoji "😀"
```

Optional: pass `--keywords path\to\keywords.csv` (columns `ticker,...,keywords`)
to attach searchable keywords to each emoji. Without it, the file name is used.

## `build-pack` options

Use `./numera-emoji build-pack --help`;
the [GUIDE CLI reference](docs/guide-cli.md) explains the options
and defaults. The ones you need first:
`--base` and `--title` (required), `--source-dir`, `--token-env` (`TELEGRAM_BOT_TOKEN` announces as the coin bot, any other as the general bot) and `--dry-run`.

Runs are **resumable**: progress is saved to `state_<base>.json`, so an
interrupted or flood-limited run continues without recreating existing sets.

## Collecting & republishing packs (multi-format)

The collector workflow downloads emoji from existing Telegram packs (and/or
builds them from scratch from your own files), deduplicates everything into a
persistent catalog, and republishes them into new packs with your own name and
labels. It supports all three custom-emoji formats — **static** (PNG/WEBP),
**animated** (`.tgs` Lottie) and **video** (`.webm` VP9).

### Animated vs. video — and how detection works

| | Animated (`.tgs`) | Video (`.webm`) |
|---|---|---|
| Nature | vector Lottie animation (gzip JSON) | VP9 pixel video |
| Cap | ≤ 64 KB | ≤ 256 KB, ≤ 3 s, 30 fps, no audio |
| `format` | `animated` | `video` |
| Build from | Lottie JSON/TGS only | any GIF/MP4/WEBM/image (ffmpeg) |

Detection is automatic: from the Bot API a sticker's `is_animated` / `is_video`
flags decide the format; downloaded files are also verified by magic bytes
(`1F 8B`→tgs, `1A 45 DF A3`→webm, PNG/RIFF-WEBP→static). A GIF or video **cannot**
become an *animated* emoji (those are vector-only) — it becomes a *video* emoji.

### Duplicate-proof by design

The old delete-and-rebuild churn is gone. The catalog (`collection/catalog.db`)
deduplicates at **ingest** time, not after publishing:

1. **`file_unique_id` pre-check** — a sticker already ingested is never
   downloaded again. Publishing also records the uploaded copies'
   `file_unique_id`s, so re-fetching your **own** published packs downloads
   nothing.
2. **Normalized content hash** — identical media from different packs collapse
   into one entry (their emoji/keywords/sources merge).
3. **Perceptual hash (dHash)** — near-identical logos merge within a threshold
   (opt-in via `--phash-threshold`; off by default so distinct look-alikes
   survive).
4. **Idempotent publish** — uploaded items are tracked per item; before
   uploading, the live set is reconciled and any applied-but-unrecorded sticker
   is attributed back to its catalog item, so interruptions can never create
   duplicates.
5. **Verified network retries** — a timeout after Telegram already processed an
   `addStickerToSet` is detected against the live set and never re-sent, so the
   same emoji can't land in a pack twice.

### Commands

```powershell
# 1. Download from one or more existing packs into the catalog
.\numera-emoji.exe fetch-pack https://t.me/addemoji/somepack_by_bot `
    --token-env GENERAL_BOT_TOKEN

# 2. (optional) Build extra emoji from your own files (auto static/video/animated)
.\numera-emoji.exe add-media --in input\myset --emoji 😀

# 3. Preview, then publish into new per-format packs (resumable)
.\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN --dry-run
.\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN
```

Sets are named `<base>s<n>_by_<bot>` (static), `<base>v<n>_by_<bot>` (video) and
`<base>a<n>_by_<bot>` (animated). All working data lives under `collection/`
(gitignored).

> Re-publishing other people's emoji under a new name may raise ownership /
> copyright concerns — only collect content you have the right to use.

## Numera Emoji Mapper bot (premium-emoji ID extractor)

The native `emoji-bot` command runs the general bot interactively (long-polling) and extracts
premium custom-emoji IDs with tap-to-copy buttons (Telegram `copy_text`):

- Send the bot a **premium emoji** → it replies with the ID on a copy button.
- Send/forward a **post mixing text + premium emoji** → it lists every ID;
  tapping a button copies them.
- **Add it to a channel/group** (as admin) → it DMs you the premium-emoji IDs
  from new posts. (Bots cannot read past channel history, only new posts.)

```powershell
.\numera-emoji.exe emoji-bot     # or run.ps1 -> C1
```

## Cloudflare Worker (both bots, hosted)

`worker/` runs the same bot behaviour on Cloudflare instead of your machine —
both bots in one Worker, each on its own path (`/tg/general`, `/tg/coin`) with
its own webhook secret, answering only the ids in `ADMIN_USER_IDS`. It also
exposes `POST /publish`, so a finished pack is announced **by the bot** in your
channel rather than by this machine: set `WORKER_PUBLISH_URL` and
`WORKER_PUBLISH_SECRET` in `.env` and **all three publishers** — `build-pack`,
`build-collection` and `coin-rebuild` — route their links through
it. Leave either unset and the existing direct path is used, unchanged.

It keeps its own log: a D1 table of warnings, errors and publishes, capped at the
newest 5 000 rows, plus an errors-only Telegram channel; routine lines go to
Workers Logs only, so a message to a bot costs no D1 write. Every line starts with the bot that wrote it,
because both bots share the Worker, the table and the channel.

> **A Telegram bot token can use `getUpdates` (polling) or a webhook — never
> both.** Registering a webhook for a token stops `emoji-bot` receiving
> anything on it; `deleteWebhook` hands it back. Run one or the other per token.

Setup, secrets and deployment: [`worker/README.md`](worker/README.md).

```powershell
cd worker
npm install
.\scripts\put-secrets.ps1                       # from .env, nothing printed
npx wrangler deploy
.\scripts\set-webhooks.ps1 -BaseUrl https://<your-worker>.workers.dev
```

## Curate panel (pick which emoji go into the pack)

The native `panel` command opens a local dark neon-blue web panel showing every emoji in the
catalog as a large labelled card. All cards are selected by default; click one
to toggle it (deselected = excluded from the next publish), Shift+click for a
range, **drag** to set the publish order — each card shows its position,
and dragging to the top or bottom edge scrolls the page so you can move an
item across the whole catalog in one go. **Click the `premium-id:` label
to copy that id** to the clipboard — it does not toggle the card. Visually similar emoji start out next
to each other so look-alikes are quick to deselect. Click **Save**, then
`build-collection` only publishes the included items, and
`plan-apply` shows (and with `--apply` makes) the moves between
packs the saved layout asks for, at most 20 Telegram changes per run.

**The grid is virtual**: only the rows near the viewport exist in the page,
whatever the catalog holds, so a thousand cards scroll and drag like a
hundred. **Zoom** with the `−` / `100%` / `+` buttons, Ctrl+wheel or
Ctrl+plus/minus (Ctrl+0 resets) — out to fit more emoji per screen, in to
inspect one; below 75 % the text under each thumbnail is dropped so the rows
pack tighter. The level is remembered across reloads.

**Selection mode** shows only the pick checkbox. Shift-click picks the whole
range from the last anchor; Select all / Deselect all / Invert act on picks.
**Hold** removes picked cards from the grid and pack counts. Each held card has
**Unhold**, and **Unhold all** restores the group. A full original pack produces
a capacity error without releasing any conflicting cards.

**Undo/Redo** covers picking, inclusion, hold/unhold, dragging, zoom, animation
and backdrop. **Reset all** restores this page's last successful explicit Save
checkpoint; Reset is undoable. Order continues to auto-save, while inclusion
changes require Save. Older replies never acknowledge edits made after a Save.
A failed request retains the draft; transient failures retry, and a permanently
rejected body waits for a changed request. **Export draft** preserves a local
JSON copy if reconciliation is needed, and **Import draft** (next to Reset
all) reads one back: keys this catalog still holds take the draft's decisions,
unknown keys are dropped, emoji added since the draft stay at the end. Nothing
is posted until you press Save, the import is undoable, and a toast says how
many were kept, not in this catalog, and new.

**At most 24 cards animate**, the ones nearest the centre of the screen; the
**All visible** switch animates every visible card (heavier). Video cards are
light animated WebP previews by default; the **Real video** switch brings back
`<video>` players, which are never created or torn down mid-scroll. A scroll
freezes only the cards that were playing and thaws them once it settles.
Previews come in two tiers — 104 px at up to 15 fps, and below 75 % zoom 72 px
at up to 10 fps — and a background warm-up renders exactly those for animated
and video cards (plus the holding tray's still) at start-up. The server renders
up to max(2, min(6, CPU count − 2)) uncached previews at once. Switching
**Animation** off holds every card on its first frame. The source media and
published files are unchanged.

If a refresh cannot open the catalog (a publish holding it), the page keeps the
last view and says "catalog busy (a publish may be running) — showing the view
from HH:MM UTC".

Panel actions and browser error locations join the normal UTC `logs/panel_*.log`
file. Event fields contain counts, revisions, status and source locations;
tokens, labels, media IDs and arbitrary error text are excluded. Log delivery
is bounded and does not block saving.

```powershell
.\numera-emoji.exe panel        # or run.ps1 -> B4
.\numera-emoji.exe panel --preview-fps 12
# Branding without a Telegram username lookup:
.\numera-emoji.exe panel --bot-username YourEmojiBot
```

B4 reopens an existing panel on the requested port instead of starting a second
server. Its current session and unsaved work stay active. A different service
on that port is refused; use another port for a separate panel session.

## Crypto-coin workflow (one component: `coins/`)

The Rust CLI owns provider and coin-domain workflows, including Telegram
transport, upload proofs, locks, atomic state, announcements and table rendering.
The generic `build-pack` command also supports the coin bot (`TELEGRAM_BOT_TOKEN`).
Coin data, images and retained compatibility modules live under `coins/`.
The executable includes `coin-alias-map`, `coin-enhance-map` and
`coin-build-keywords`, `coin-fetch-logos [pages]`, `coin-fetch-paprika` and
`coin-fetch-cmc`, with `--coin-dir DIR` for isolated coin data work. Provider
commands retain exact `--dry`; Paprika also accepts `--retry-unmatched`.
`coin-write-manifests` writes read-only live coin reports with
`--out-dir`, `--state`, `--map` and `--token-env`. `coin-check-packs
[--coin-dir DIR]` resumes a live read-only blank/duplicate audit; failed images
remain retryable and incomplete listings never replace the report.
`coin-remap-ids --emoji-dir DIR` rebuilds mappings by content, dry by default;
calibrated `--apply` preserves a backup and refuses ambiguous/incomplete results.
It makes no Telegram mutations.
`coin-verify-logos --emoji-dir DIR` is review-only unless explicit
`--fix --only` selects repairs; fixing mutates Telegram and requires separate
operator authorization.
`coin-rebuild-map [--coin-dir DIR] [--emoji-dir DIR] [--token-env KEY]`
uses the frozen rebuild plan and recorded sources to prove every live sticker by
image content. It preserves current aliases, refuses lost aliases/unreviewed
shared crowds and writes review candidates rather than overwrite an unproven map.
It reads Telegram only.
`coin-rebuild [all|build|map|links]` accepts the same options.
**Default `all` and explicit `build` may delete old packs and upload**; `links`
resends messages, and `map` only reads Telegram. These Rust commands are exercised against synthetic providers and Telegram
fixtures in CI. Platform/package results are tracked in
[CI acceptance](docs/rust-ci-status.md); tests do not publish to real accounts.
The historical source-retirement ledger remains a separate review gate in
[migration status](docs/native-backend.md).
Run the installed executable from your package root, for example
`./numera-emoji coin-rebuild map` (`.exe` on Windows):

- `coin-fetch-logos` — download CoinGecko logos and write `coins/keywords.csv`.
- `coin-fetch-paprika` / `coin-fetch-cmc` — fill missing logos from the other providers.
  Unanswered lookups remain unknown and uncached; partial runs exit 3.
  `coin-fetch-paprika --retry-unmatched` retries older unmatched searches.
- `coin-build-keywords` — rebuild the CSV while preserving earlier resolved names.
- `coin-rebuild [all|build|map|links]` — rebuild packs, identifiers and link messages.
- `coin-remap-ids` — reconstruct `ticker_to_id.json` from image content.
- `coin-enhance-map` / `coin-alias-map` — preserve chain-variant aliases.
- `coin-verify-logos` — review official art and repair only explicitly selected tickers.
- `coin-write-manifests` — write each pack's ticker/emoji-ID manifest.
- `coin-check-packs` — audit packs for blank and duplicate stickers.

Coin compatibility/similarity dependencies are installed from
`requirements-coins.txt`. The retained Python modules support the original
regression suite; public coin commands use Rust.

Convert coin logos to 100×100 PNGs (images live in `coins/logos/{svg,png}` →
`coins/logos/emoji`):

```powershell
.\numera-emoji.exe make-emoji-pngs --in coins\logos\svg --out coins\logos\emoji
.\numera-emoji.exe make-emoji-pngs --in coins\logos\png --out coins\logos\emoji
```

Then build/rebuild with the coin bot:

```powershell
# build + rebuild the id map + send the links (DESTRUCTIVE: deletes the old packs
# first). Subcommands: build = upload only, map = rebuild the id map, links = resend.
.\numera-emoji.exe coin-rebuild
```

## Project layout

```
Numera Emoji Mapper/                  # the whole project
  runtime/                     # standalone Rust application and native media codecs
    src/                       # production library, CLI, media worker and packaging
    vendor/rlottie/             # pinned statically linked Lottie renderer
    tests/                     # Python-free native and extracted-package acceptance
  native/                      # optional PyO3 compatibility/oracle development harness
    src/                       # shared pure Rust modules and historical adapters
    tests/                     # retained contract and platform acceptance
  emojikit/                    # bounded Python compatibility, not CLI applications
    media_bridge.py            # retained compatibility IPC for original regressions
    media.py media_lottie.py    # static/video/Lottie decode, convert, validate, preview
    video_decode.py            # decoder selection and frame caching
    identity.py repaint.py     # stable content identity and exact tint operations
    telegram_api.py announce.py # native-backed coin transport/proof/announcement exports
    packstate.py pack_rows.py   # native-backed leases, atomic state and table rendering
    cli_env.py operator_config.py logsetup.py errors.py
                               # shared coin/codec configuration, logging and errors
    similarity.py              # required native compute bindings
    sandbox_clone.py maintenance.py sqlite_snapshot.py
    state_artifacts.py media_paths.py
                               # development clone/snapshot/reference support
  worker/                      # Cloudflare Worker: both bots + /publish (TypeScript)
    src/                       # auth, telegram, emoji-id extraction, routing
    test/                      # vitest, fetch stubbed (never reaches Telegram)
  assets/                      # the project's own images + the panel page and its scripts:
                               # panel-grid, -motion, -save, -drag, -actions, -holding, -draft
                               # (.js, in load order) + jsconfig.json/panel-globals.d.ts (tsc)
  run.ps1 / run.sh              # direct standalone menu/doctor launchers
  scripts/run-actions.ps1      # historical PowerShell action compatibility fixture
  scripts/check.ps1            # byte-compile + full unit suite (also used by CI)
  numera-emoji                  # packaged standalone executable (.exe on Windows)
  scripts/run-native.ps1       # menu/doctor native boundary (no Cargo)
  docs/native-backend.md       # runtime boundaries, acceptance and rollback
  scripts/panel_sandbox.py     # the panel against a throwaway copy of the catalog
  requirements.txt
  requirements-dev.txt         # test-only: ruff + playwright (never at runtime)
  requirements-lint.txt        # the pinned ruff, shared by requirements-dev.txt and CI
  .env.example                 # configuration template
  README.md  LICENSE                # CONTRIBUTING/SECURITY live in .github/
  tests/                       # unit tests + fixtures (see tests/README.md)
  coins/                       # ONE component: the crypto-coin emoji tool
    fetch_*.py                 # coin logo fetchers (CoinGecko/Paprika/CMC)
    build_keywords.py
    rebuild_dedup.py           # duplicate-proof rebuild + inventory fill
    check_all_packs.py         # pack integrity audit
    run_convert.ps1 run_rebuild_loop.ps1
    keywords.csv               # coin ticker -> keywords (curated input)
```

Generated/local-only (gitignored): `logos/` (and `coins/logos/`), `build/`,
`input/`, `collection/`, `packs/`, `*_state.json`, `*.filled.md`, `.env`,
`secrets.md`. The coin component's own map and inventory
(`coins/ticker_to_id.json`, `coins/currency-emoji-inventory.md`,
`coins/shared_logo_groups.json`, `coins/unresolved_logos.json`) are local-only
too: this repository ships the tool that builds emoji packs, not anybody's
published packs, and those files name live custom-emoji ids. They are rebuilt
from Telegram by `./numera-emoji coin-rebuild map`.

## Checks

CI runs on every push and pull request, and supports manual dispatch.
The full native acceptance step uses Cargo's automatic suite discovery; a new
integration test does not need adding to a hand-maintained target list. The
migration checkpoint is a limited diagnostic workflow, not full acceptance.

Production checks use the dedicated bounded native runner (on Linux, its private
network namespace requires a host permitting unprivileged user namespaces):

```bash
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked
```

The retained compatibility check command byte-compiles and lints the historical
Python files and runs their full regression suite:

```powershell
# once: core, coins, ruff and playwright, then the panel suite's Chromium
.venv\Scripts\python.exe -m pip install -r requirements.txt -r requirements-coins.txt -r requirements-dev.txt; .venv\Scripts\python.exe -m playwright install chromium
.\scripts\check.ps1
.\scripts\check.ps1 -Tests test_catalog -SkipCompile -SkipLint   # chosen suites only
.\run.ps1 -Check      # native codec/configuration doctor; no Python environment needed
.\numera-emoji.exe status   # is the roster/archive current? offline, exit 3 if stale
```

If anything the gate needs is missing, `check.ps1` prints that one install line
and exits 2.

`tests/test_panel_browser.py` drives the real panel in headless Chromium and
**raises** rather than skipping when playwright or Chromium is missing — a
browser test that reports green on a machine with no browser is worse than
none. `NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` opts out deliberately; CI does that in
the Python matrix and runs the module in its own job instead.

Lint is `ruff check .` with **no** arguments: `ruff.toml` at the repo root owns
the rule set, so nothing can diverge between CI and a local run. That set is
deliberately narrow — ruff's default rules plus `E402`, `BLE001`, `B` and
`RUF100` — because the source already carried ~120 `# noqa: E402` /
`# noqa: BLE001` comments written against a linter that was never configured.
Enabling exactly the codes those comments name is what makes them mean
something, and `RUF100` fails a `# noqa` that no longer suppresses anything, so
they cannot rot again.

Running the suite by hand? Use exactly this form:

```powershell
.venv\Scripts\python.exe -m unittest discover -s tests -t . -p "test_*.py"
```

`-t .` is not cosmetic. Without it the tests directory becomes the top level,
modules load as `test_x` instead of `tests.test_x`, and `tests/__init__.py` —
which scrubs credentials out of the environment and refuses non-loopback sockets
— never runs. See [`tests/README.md`](tests/README.md).

## Security

Tokens and the owner id live only in `.env` (never committed) and are read at
runtime. See [SECURITY.md](.github/SECURITY.md). Revoke a leaked token via @BotFather
and update `.env`.

## License

Numera Emoji Mapper — builds Telegram premium custom-emoji packs.
Copyright (C) 2026 KiaroSama

This program is free software: you can redistribute it and/or modify it under
the terms of the **GNU General Public License** as published by the Free
Software Foundation, either version 3 of the License, or (at your option) any
later version. See [LICENSE](LICENSE) for the full text.

This program is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the GNU General Public License for more details. If you
did not receive a copy along with this program, see
<https://www.gnu.org/licenses/>.

The GPL covers this project's own source. Third-party logos and data keep their
own providers' licenses, which the GPL does not and cannot change (see below).

## Sources & attribution

Coin vector logos come from open icon sets
([spothq/cryptocurrency-icons](https://github.com/spothq/cryptocurrency-icons),
[Cryptofonts/cryptoicons](https://github.com/Cryptofonts/cryptoicons)); raster
logos from [CoinGecko](https://www.coingecko.com/),
[CoinPaprika](https://coinpaprika.com/) and
[CoinMarketCap](https://coinmarketcap.com/). Respect each source's license and
terms when redistributing. For general packs, only use images you have the right
to use.

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
