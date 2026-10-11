[Complete guide index](GUIDE.md)

## Appendix A — Command cheat-sheet

All commands run from the project root. `PY = .venv\Scripts\python.exe` (Windows)
or `.venv/bin/python` (macOS/Linux).

```powershell
# --- setup ---
py -3.11 -m venv .venv
.venv\Scripts\python.exe -m pip install -r requirements.txt         # core
.venv\Scripts\python.exe -m pip install -r requirements-coins.txt   # only for coins/remap_ids.py
copy .env.example .env            # then edit tokens + owner id
.\run.ps1                         # launcher menu
.\run.ps1 -Check                  # env doctor (CI-style)
.\scripts\check.ps1               # byte-compile + full unit suite

# --- single pack (general) ---
$PY -m emojikit.make_emoji_pngs --in input\set --out build\set
$PY -m emojikit.build_pack --base set --title "Set" --source-dir build\set --token-env GENERAL_BOT_TOKEN --dry-run
$PY -m emojikit.build_pack --base set --title "Set" --source-dir build\set --token-env GENERAL_BOT_TOKEN

# --- collector ---
$PY -m emojikit.fetch_pack <pack-or-link> --token-env GENERAL_BOT_TOKEN
$PY -m emojikit.fetch_emoji_ids --ids-file <file-with-premium-ids> [--id <n>]
$PY -m emojikit.add_media --in input\set --emoji 😀
$PY -m emojikit.panel                                            # curate, then Save
$PY -m emojikit.build_collection --base mypack --title "My Pack" --token-env GENERAL_BOT_TOKEN --dry-run
$PY -m emojikit.build_collection --base mypack --title "My Pack" --token-env GENERAL_BOT_TOKEN
$PY -m emojikit.sync_order --base mypack                          # reorder a LIVE pack
$PY -m emojikit.sync_order --base mypack --apply

# --- bot ---
$PY -m emojikit.emoji_bot

# --- coins ---
$PY -m coins.fetch_logos
$PY -m emojikit.make_emoji_pngs --in coins\logos\svg --out coins\logos\emoji
$PY -m emojikit.make_emoji_pngs --in coins\logos\png --out coins\logos\emoji
$PY -m coins.rebuild_dedup        # all: deletes the recorded old packs first
$PY -m coins.remap_ids --emoji-dir "<coin logo folder>" --max-distance 200 --apply
$PY -m coins.enhance_map
$PY -m coins.alias_map
$PY -m coins.check_all_packs
$PY -m coins.verify_logos --emoji-dir "<coin logo folder>"
$PY -m coins.verify_logos --emoji-dir "<coin logo folder>" --fix --only sol,xrp
$PY -m coins.write_manifests --out-dir "<coin archive folder>"

# --- tests / CI-locally ---
.\scripts\check.ps1                                     # what CI runs (compile + suite)
$PY -m unittest discover -s tests -t . -p "test_*.py"   # -t . is REQUIRED (see §10)
$PY -c "from emojikit import build_pack, make_emoji_pngs, fetch_pack, fetch_emoji_ids, add_media, build_collection, emoji_bot, panel"

# --- git ---
git add -A; git commit -m "..."; git push origin main
gh run list --repo KiaroSama/Numera-Emoji-Mapper --limit 1 --json status,conclusion
```

---

## Appendix B — Example session transcripts (illustrative)

### B.1 Fetch + curate + publish

```
> .\native\runtime\numera-emoji.exe fetch-pack https://t.me/addemoji/RMaccs --token-env GENERAL_BOT_TOKEN
[..] Authenticated bot: @YourEmojiBot
[..] Pack RMaccs (Accounts store — @RMaccs): 80 stickers -> new=80 dedup=0 failed=0
Done. new=80 dedup=0 failed=0
  catalog static: 80 total (80 pending upload)

> .\native\runtime\numera-emoji.exe panel
Emoji curate panel: http://127.0.0.1:9450/
# (open browser, untick a few, click Save -> "Saved ✓  76 included · 4 excluded")

> .\native\runtime\numera-emoji.exe build-collection --base accts --title "Accounts" --token-env GENERAL_BOT_TOKEN --dry-run
DRY RUN: nothing uploaded.
  static: 76 emoji -> 1 set(s) named acctss1_by_<bot> ...
  video: 0 emoji -> 0 set(s) ...
  animated: 0 emoji -> 0 set(s) ...
```

### B.2 Resolve an id and download its pack

```
> .\native\runtime\numera-emoji.exe fetch-pack 5283254221590787816 --token-env GENERAL_BOT_TOKEN
```

### B.3 Audit the coin packs

```
> python -m coins.check_all_packs
...
total stickers audited: 5791
BLANK stickers: 0
DUPLICATE image groups: 0 (extra duplicate stickers: 0)
```

> Transcripts are illustrative; exact counts depend on your data and Telegram
> rate limits at the time.

---

## Appendix C — Conventions for contributors / agents

- **Language:** all code, comments, filenames, logs, docs in English.
- **Placement:** source at root or in `emojikit/`/`coins/`; tests in `tests/`;
  docs in `docs/`; shipped images in `assets/`; the Cloudflare Worker in
  `worker/`. Don't clutter the root.
- **No new dedup/mapping mechanisms** — extend the catalog (§5, §13, §16).
- **Never** commit `.env`, `secrets.md`, `collection/`, `logs/`, tokens.
- **Logging** is mandatory for executable scripts via `emojikit.logsetup`
  (UTC file logs; secrets redacted).
- **Verify before claiming done:** `.\scripts\check.ps1` + a real run; for UI, a
  real browser (Playwright) at the relevant breakpoints.
- **Sync:** push to `main` and keep this guide + `README.md` current.

*This guide is the single source of truth for how Numera Emoji Mapper works. If code
and guide disagree, fix whichever is wrong and re-sync.*

---

## Appendix D — Building the next pack: the fast path

Distilled from the first full 200-emoji pack. The steps are the short version of
§6; the traps below each cost real time, and every one of them is now either
prevented by code or answerable in seconds if you know to look.

### The order that works

```powershell
$PY -m emojikit.fetch_emoji_ids --ids-file ids.txt      # or emojikit/fetch_pack.py / emojikit/add_media.py
$PY -m emojikit.panel                                   # curate + order, then Save selection
# check the header: it warns when the total exceeds one pack
$PY -m emojikit.build_collection --base <Base> --title "<Title>" --mixed --dry-run
$PY -m emojikit.build_collection --base <Base> --title "<Title>" --mixed
$PY -m emojikit.sync_order --base <Base>                # report; --apply to place them
```

### Count the brand logo

It is the first emoji of every set and occupies one of Telegram's 200. **200
catalog items + the logo = 201, which publishes as TWO sets.** For a single
pack, include 199. The panel numbers the logo #1 and warns in the header, so
trust the header, not your own count of the grid.

### Preflight runs automatically

Launcher **B3** now runs `--preflight` between the dry run and the upload: every
queued file is offered to Telegram's validator first, and a refusal stops the
run before a single sticker is published. About a minute for a 200-emoji queue,
against the 46 it cost to discover the same file mid-publish. Run it by hand
with `$PY -m emojikit.build_collection --base <Base> --title "<T>" --preflight`.

### Probe a suspect file before a 45-minute run

`uploadStickerFile` runs the **same validator** as `addStickerToSet` and touches
no pack, so it answers "will Telegram take this file?" in one call. Worth doing
for anything unusual before starting a long publish — a single refused emoji
cost a whole run here.

**A `.tgs` Telegram happily *plays* can still be refused on *upload*.** The
uploader's Lottie validator is stricter than the player: a **subtract mask**
(`mode: "s"`) is rejected, while add masks pass. Proven by sending Telegram's
own untouched original back and having it refused. Our re-encode is innocent.

The way out is to stop asking the Lottie validator at all: **render the
animation and ship it as a VIDEO emoji.** The mask is applied by the renderer,
so the artwork is unchanged, and `format=video` never goes near the `.tgs` path.

**And the mirror case: a video source can UNDER-DECLARE its own length.** One
sticker served by Telegram carried a `duration=3.000` container header over
3.916 s of packets — 94 frames ending at 3.875 s. Telegram's uploader reads the
header, so the original was accepted; our `-c copy` remux recomputes duration
from the packets, wrote the truthful 3.916 s, and came back
`STICKER_VIDEO_LONG`. The picture was identical, frame for frame. Here our
re-encode is the honest one and the source is not.

`reencode_in_place` therefore keeps the original whenever a remux would cross
`WEBM_MAX_SECONDS`, exactly as it already does for the byte caps: a byte-clone
is a lesser failure than an upload that cannot happen. To publish such a source
as our own bytes, re-time it first — speeding the whole loop to fit keeps the
animation, where trimming the tail does not.

### Expect flood waits, and read the log

A 200-emoji publish takes roughly 45 minutes, most of it in Telegram's flood
waits (240–270 s each). The log records every wait and every successful upload
(`uploaded <key> -> <set> #<n> (n/total this run)`), so a quiet log now means
stopped, not slow.

### Stopping is safe; resuming is automatic

Every upload commits its own flag, so a killed run loses nothing. The lock its
process left behind frees itself **120 seconds** after the process dies — just
re-run the same command. `reconcile_set` attributes anything that landed without
being recorded.

### The channel link waits for a clean run

A trailing pack is announced only by a run with **no failure and no skip**. If
one is withheld, the log says so, and the next clean run posts it.

### Order is not frozen at publish time

`sync-order` moves stickers with `setStickerPositionInSet`: no re-upload, and
`file_id` **and** `custom_emoji_id` survive, so nobody using the emoji is
affected. Run it report-only first; it refuses a set holding any sticker the
catalog cannot identify.

### Two things that will mislead a measurement

- **Telegram re-encodes what it stores.** Never confirm an upload by exact
  content key — the key is a SHA of exact pixels, so a lossy re-encode changes
  it for a picture that is visually identical. `media.same_image` owns this
  comparison for every caller: exact key first, then **both** a dHash and a
  colour check, since neither alone is safe. dHash survives the re-encode but
  is grayscale, so a stranger's green square sits 4 bits from our red one; the
  mean channel delta sees colour but not structure. Measured over 30 known-same
  and 30 known-different pairs across three live packs — dHash 0–3 vs 12–47
  bits, mean delta 0.02–2.35 vs 35.46–188.22.
  A false negative costs a halted publish and is recoverable; a false positive
  attributes a stranger's sticker to our item and is not. That asymmetry is why
  it asks for two independent agreements, and why anything it cannot compare
  (animated is vector, so there is no raster hash) returns *undecidable* rather
  than *no*.
- **VP9 keeps alpha in a separate layer.** Probing a video emoji without
  `-c:v libvpx-vp9` reports every one of them as opaque, correct ones included.
  The same flag is required when re-encoding, or transparency is silently lost.

### Keeping a copy

A pack still being filled keeps its files under `collection/media/<format>/`,
named by content key, with `collection/manifests/<set>.md` listing what went
where; a FULL pack's media moves to the archive (§12.5d). To take one pack away as a zip in pack order with an index:
`.\native\runtime\numera-emoji.exe pack-archive --export <n> --zip pack<n>.zip` (§12.5d).
