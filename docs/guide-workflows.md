[Complete guide index](GUIDE.md)

## Appendix A — Command cheat-sheet

Run commands from the package root. Examples use `.\numera-emoji.exe`; on
Linux/macOS use `./numera-emoji`. No Python environment is required.

```powershell
# --- setup / diagnostics ---
Copy-Item .env.example .env       # then edit configuration
.\numera-emoji.exe package-verify
.\numera-emoji.exe doctor
.\run.ps1                        # native menu; Linux/macOS: ./run.sh

# --- single pack ---
.\numera-emoji.exe make-emoji-pngs --in input\set --out build\set
.\numera-emoji.exe build-pack --base set --title "Set" --source-dir build\set --token-env GENERAL_BOT_TOKEN --dry-run
.\numera-emoji.exe build-pack --base set --title "Set" --source-dir build\set --token-env GENERAL_BOT_TOKEN

# --- collector ---
.\numera-emoji.exe fetch-pack <pack-or-link> --token-env GENERAL_BOT_TOKEN
.\numera-emoji.exe fetch-emoji-ids --ids-file <file-with-premium-ids>
.\numera-emoji.exe add-media --in input\set --emoji 😀
.\numera-emoji.exe panel
.\numera-emoji.exe build-collection --base mypack --title "My Pack" --token-env GENERAL_BOT_TOKEN --dry-run
.\numera-emoji.exe build-collection --base mypack --title "My Pack" --token-env GENERAL_BOT_TOKEN
.\numera-emoji.exe sync-order --base mypack
.\numera-emoji.exe sync-order --base mypack --apply

# --- bot ---
.\numera-emoji.exe emoji-bot

# --- coins ---
.\numera-emoji.exe coin-fetch-logos
.\numera-emoji.exe make-emoji-pngs --in coins\logos\svg --out coins\logos\emoji
.\numera-emoji.exe make-emoji-pngs --in coins\logos\png --out coins\logos\emoji
.\numera-emoji.exe coin-check-packs
.\numera-emoji.exe coin-remap-ids --emoji-dir "<coin logo folder>"
.\numera-emoji.exe coin-remap-ids --emoji-dir "<coin logo folder>" --max-distance 200 --apply
.\numera-emoji.exe coin-enhance-map
.\numera-emoji.exe coin-alias-map
.\numera-emoji.exe coin-verify-logos --emoji-dir "<coin logo folder>"
.\numera-emoji.exe coin-verify-logos --emoji-dir "<coin logo folder>" --fix --only sol,xrp
.\numera-emoji.exe coin-write-manifests --out-dir "<coin archive folder>"
# DESTRUCTIVE: all/build can delete recorded old packs before uploading.
.\numera-emoji.exe coin-rebuild all

# --- contributor tests (source checkout/toolchain required) ---
cargo test --manifest-path runtime/Cargo.toml --locked
```

Calibrate remapping without `--apply` before choosing a cutoff. Live publishing,
rebuild and repair commands act on the configured account; run them only for the
operator targets you intend to change. Use `COMMAND --help` for current flags.

---

## Appendix B — Example session transcripts (illustrative)

### B.1 Fetch + curate + publish

```
> .\numera-emoji.exe fetch-pack https://t.me/addemoji/RMaccs --token-env GENERAL_BOT_TOKEN
[..] Authenticated bot: @YourEmojiBot
[..] Pack RMaccs (Accounts store — @RMaccs): 80 stickers -> new=80 dedup=0 failed=0
Done. new=80 dedup=0 failed=0
  catalog static: 80 total (80 pending upload)

> .\numera-emoji.exe panel
Emoji curate panel: http://127.0.0.1:9450/
# (open browser, untick a few, click Save -> "Saved ✓  76 included · 4 excluded")

> .\numera-emoji.exe build-collection --base accts --title "Accounts" --token-env GENERAL_BOT_TOKEN --dry-run
DRY RUN: nothing uploaded.
  static: 76 emoji -> 1 set(s) named acctss1_by_<bot> ...
  video: 0 emoji -> 0 set(s) ...
  animated: 0 emoji -> 0 set(s) ...
```

### B.2 Resolve an id and download its pack

```
> .\numera-emoji.exe fetch-pack 5283254221590787816 --token-env GENERAL_BOT_TOKEN
```

### B.3 Audit the coin packs

```
> .\numera-emoji.exe coin-check-packs
...
total stickers audited: 5791
BLANK stickers: 0
DUPLICATE image groups: 0 (extra duplicate stickers: 0)
```

> Transcripts are illustrative; exact counts depend on your data and Telegram
> rate limits at the time.

---

## Appendix C — Conventions for contributors / agents

- Keep code, comments, filenames, logs and documentation in English.
- Production local Rust belongs in `runtime/` or shared `native/src/` modules;
  browser assets belong in `assets/`, Cloudflare code in `worker/`, docs in `docs/`.
- Extend the existing catalog and identity contracts rather than adding another
  dedup or mapping mechanism.
- Never commit configuration secrets, operator data or logs.
- Use native UTC run logs and redaction for application commands.
- Verify the relevant native tests and real browser behavior when changing UI.
  Historical oracle tests remain separate developer checks.
- Keep documentation current and follow the repository's review workflow.

## Appendix D — Building the next pack: the fast path

Distilled from the first full 200-emoji pack. The steps are the short version of
§6; the traps below each cost real time, and every one of them is now either
prevented by code or answerable in seconds if you know to look.

### The order that works

```powershell
.\numera-emoji.exe fetch-emoji-ids --ids-file ids.txt      # or fetch-pack / add-media
.\numera-emoji.exe panel                                   # curate + order, then Save selection
# check the header: it warns when the total exceeds one pack
.\numera-emoji.exe build-collection --base <Base> --title "<Title>" --mixed --dry-run
.\numera-emoji.exe build-collection --base <Base> --title "<Title>" --mixed
.\numera-emoji.exe sync-order --base <Base>                # report; --apply to place them
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
with `.\numera-emoji.exe build-collection --base <Base> --title "<T>" --preflight`.

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
  it for a picture that is visually identical. the native media comparator owns this
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
`.\numera-emoji.exe pack-archive --export <n> --zip pack<n>.zip` (§12.5d).
