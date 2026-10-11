[Complete guide index](GUIDE.md)

## 13. The catalog database (`collection/catalog.db`)

SQLite, created/managed by the native `catalog` responsibility. Four tables: `items`,
`seen_files`, `meta` and `publications`.

**Media paths survive a folder rename.** They used to be absolute: renaming the
project folder left 68 rows pointing at a folder that no longer existed, and the
panel showed each as a broken thumbnail. Opening a catalog now converts it once
(`_migrate_media_paths`): every path inside the data folder becomes `./<path>`,
relative to it. The `./` marks the row itself -- older code never wrote a leading
`./`, so its relative rows (project-root relative) stay distinguishable without
looking at any other table. `meta.media_paths = data-relative` only records that
the scan is done, so an already converted catalog's open writes nothing.

### 13.1 `items` — one row per distinct emoji

| Column | Type | Meaning |
|--------|------|---------|
| `content_key` | TEXT PK | Normalized content hash, prefixed by format: `s:` static, `v:` video, `a:` animated, `r:` raw fallback. |
| `format` | TEXT | `static` / `video` / `animated`. |
| `file_path` | TEXT | Where the media file is. Inside the data folder it is stored **relative to that folder** (`./media/static/<key>.webp`), so the folder can be renamed or moved; anywhere else (the archive on another drive) it is absolute. Read it through `emojikit.media_paths.resolve(data_dir, stored)`, never with a bare `Path(...)`. |
| `emojis` | TEXT (JSON) | Associated standard emoji(s), e.g. `["🪙"]`. |
| `keywords` | TEXT (JSON) | Search keywords / labels. |
| `sources` | TEXT (JSON) | Where it came from, e.g. `["RMaccs"]` or `["local:foo.png"]`. |
| `phash` | INTEGER | 64-bit dHash stored as **signed** 64-bit (two's complement) to avoid SQLite overflow; restored to unsigned on read. |
| `custom_emoji_id` | TEXT | Live Telegram id after upload (else NULL). |
| `uploaded` | INTEGER | Legacy: `1` once published to ANY family. Still mirrored for the ingest commands' progress counts; publish state is per family in `publications` (§13.3b). |
| `included` | INTEGER | `1` = will be published (panel selection); `0` = excluded. |
| `created_utc` | TEXT | `YYYY-MM-DD HH:MM:SS UTC`. |
| `position` | INTEGER | The panel's order, which is the publish order (`set_order`). |

Indexes: `idx_items_format`, `idx_items_uploaded`.

### 13.2 `seen_files` — fast pre-dedup

| Column | Type | Meaning |
|--------|------|---------|
| `file_unique_id` | TEXT PK | Telegram's stable per-sticker id. |
| `content_key` | TEXT | The catalog row it maps to. |

If a sticker's `file_unique_id` is already here, `fetch_pack` skips the download
entirely and just merges labels.

### 13.3 `meta` — key/value (e.g. `schema_version`).

### 13.3b `publications` — what is live, per pack family

| Column | Type | Meaning |
|--------|------|---------|
| `base` | TEXT | The pack family (`--base`). Primary key `(base, content_key)`. |
| `content_key` | TEXT | The catalog row. |
| `set_name` | TEXT | The set it went into (NULL when an older run did not record it). |
| `custom_emoji_id` | TEXT | Its live id. |
| `uploaded_utc` | TEXT | When it was recorded. |

Per family, not per item: one `uploaded` flag meant publishing a catalog to one
base marked it done everywhere, so the same catalog could never be published to
a second base, and a deleted pack could not be rebuilt without hand-editing the
database.

### 13.4 Why phash is stored signed

A dHash is an **unsigned** 64-bit integer. SQLite integers are signed 64-bit, so
values ≥ 2⁶³ raised *"Python int too large to convert to SQLite INTEGER"* and
dropped rows. The catalog converts: `to_db = x - 2⁶⁴ if x ≥ 2⁶³ else x`, and
`from_db = v & (2⁶⁴-1)`. Hamming distance is computed on the restored unsigned
values. (Regression test: `tests/test_catalog.py::test_large_phash_64bit`.)

### 13.5 Inspecting the catalog manually

```powershell
.venv\Scripts\python.exe -c "import sqlite3;d=sqlite3.connect('collection/catalog.db');
print(d.execute('SELECT format,COUNT(*),SUM(included) FROM items GROUP BY format').fetchall());
print(d.execute('SELECT base,COUNT(*) FROM publications GROUP BY base').fetchall())"
```

### 13.6 Identity migration, recovery and rollback (`scripts/identity_repair.py`)

A video decoder correction can change an item's key and perceptual hash. The
migration updates `items`, `publications`, `seen_files`, publisher state/plans,
and media names together. It never uploads to Telegram.

```powershell
.venv\Scripts\python.exe scripts\identity_repair.py report
.venv\Scripts\python.exe scripts\identity_repair.py migrate-video-keys --apply
```

The canonical data directory owns one native maintenance/writer lock. Catalog
connections, collector storage, publishers (including a first family), panel
saves and archive moves all honor it. Acquisition order is catalog ownership,
existing family locks, then map locks. A `Catalog` must be closed to release its
lease. A busy panel save returns HTTP 503 and retains the browser's queued edit.
Reports take ownership for a consistent view but do not alter catalog/media/state.

`--apply` owns discovery, revalidation, backup, all mutations, final verification
and journal retirement. It first creates a WAL-safe SQLite backup plus a sibling
`catalog.before-video-identity-<stamp>.rollback.json` manifest. The manifest
records original publisher JSON, catalog signatures, byte digests and each
source/destination intent before files change. Media moves use exclusive sibling
hard links followed by removal of the verified source. An identical existing
destination can be reused; unrelated destination bytes are never overwritten.
Filesystems without hard-link support refuse safely with the journal retained.

**After interruption**, run the same `migrate-video-keys --apply` command. The
validated journal is replayed before a fresh survey: a process killed between a
rename and its SQLite path update is recoverable. Source-only, destination-only
and both-present identical states have explicit replay behavior. Ordinary writers
refuse while the journal remains. An unsupported/invalid journal is retained for
inspection; deleting it is not a repair.

**Rollback restores the application snapshot**, including paths and JSON, rather
than only copying the database back. Use the exact bundle path printed by the run:

```powershell
.venv\Scripts\python.exe scripts\identity_repair.py restore `
    --bundle catalog.before-video-identity-<stamp>.rollback.json
.venv\Scripts\python.exe scripts\identity_repair.py restore --apply `
    --bundle catalog.before-video-identity-<stamp>.rollback.json
```

The first command verifies without restoring; the second restores and verifies
media bytes, the complete database (order, inclusion, IDs and publications), and
original state/plan JSON. Later unrelated catalog edits, publisher changes or
replacement media cause refusal. Previously existing identical destinations are
preserved. An interrupted restore resumes with the same restore command. Keep
both the `.db` and `.rollback.json` files; a whole-archive duplicate is unnecessary.
Restoring the old snapshot can make the corrected decoder report pending identity
work again; it restores the prior application version's state, not new identities.

**Legacy migration repair** accepts `--from-backup <catalog.before-...db>`.
Shared immutable FUID/CID evidence must identify one matching row in each snapshot,
and every shared identifier must agree. The current file is then checked against
its current identity. Paths and archive slots never prove identity. Missing or
conflicting provenance leaves required live/in-flight references uncovered and
refuses the repair with those references named.
Only obsolete frozen-plan candidates and skipped history may remain unresolved;
the publisher intentionally skips those absent candidates. History is preserved.

**Exit codes:** `0` verified or clean; `2` usage/missing catalog; `3` pending work
or a verified restore preview; `4` incomplete/refused. Exit 4 after interruption
can mean partial application, with the journal and rollback bundle retained.
Missing media, decode failures, stale required references, orphaned identifiers,
collisions or unresolved operations never produce a `Verified` success.

`SUSPECT MAPPINGS` remains a heuristic over historical near matches. It does not
validate historical FUID/CID bindings: an old file_unique_id alone cannot retrieve
its source bytes from Telegram, and zero suspects is not proof of past correctness.

**Refused storage collisions** retain the complete incoming file under
`<data-dir>/media/<format>/quarantine/`, outside per-run scratch cleanup. A JSON
record preserves its checksum, size, attempted destination and available source
identifiers. Repeating the same refused payload reuses its retained copy. Existing
destination bytes and catalog bindings remain intact for deliberate recovery.

---

## 14. `emojikit` library API

### 14.1 `emojikit.media`

Constants: `SIZE=100`, `TGS_MAX_BYTES=65536`, `WEBM_MAX_BYTES=262144`,
`WEBM_MAX_SECONDS=3.0`, `WEBM_FPS=30`.

| Function | Returns | Notes |
|----------|---------|-------|
| `detect_format(path)` | `"static"\|"animated"\|"video"\|"unknown"` | By magic bytes, then extension. |
| `detect_format_bytes(head)` | same | From leading bytes only. |
| `telegram_sticker_format(sticker)` | format | From `is_animated`/`is_video`. |
| `ext_for_format(fmt)` | `.png`/`.tgs`/`.webm` | Canonical extension. |
| `media_extension(path, fmt)` | str | Refines static into `.png` vs `.webp`. |
| `fit_100(img)` | PIL.Image | Trim transparent borders, center on 100×100 RGBA. |
| `to_static_png(src, out)` | Path | Any image (SVG via resvg) → 100×100 PNG. |
| `to_video_webm(src, out)` | Path | ffmpeg → VP9 WEBM, 100×100, ≤3 s, transparent-padded; CRF escalates until ≤256 KB. |
| `probe_video(path)` | `VideoInfo(width,height,duration,codec)` | via ffprobe. |
| `validate_video(path)` | raises on violation | dims/duration/codec/size checks. |
| `to_animated_tgs(src, out)` | Path | Lottie `.json`/`.tgs` → valid 512×512 `.tgs` (gzip). Any other canvas is refused, never rescaled. |
| `validate_tgs(path)` | raises | ≤64 KB, gzip, required Lottie keys, 512×512 canvas, ≤60 fps, ≤3 s. |

Content keys and hashes live in `emojikit.identity`, not here: `content_key(path,
fmt)` (the dedup primary key, §16), `perceptual_hash(path, fmt)` (64-bit dHash
for static/video; `None` for animated), `hamming(a, b)` and `same_image`.

The ffmpeg filter used for video:
`fps=30,scale=100:100:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba,pad=100:100:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=yuva420p`,
encoded with `libvpx-vp9 -pix_fmt yuva420p -auto-alt-ref 0`.

A **video input is decoded with an explicitly named alpha-capable decoder**,
placed before `-i`: `libvpx-vp9` for VP9, `libvpx` for VP8. Both of those
formats keep alpha in a separate WebM layer that ffmpeg's *default* `vp9`/`vp8`
decoders drop without a word, so the filter chain would see no alpha and the
transparent pad would land on an opaque frame — a re-encoded transparent emoji
came out a black square. The decoder is chosen from the codec `probe_video`
reports, never from the file extension: `.webm` is a container and says nothing
about what is inside it. GIF/PNG inputs get no override. The same choice is
needed to *inspect* a clip — probing a VP9 emoji with the default decoder
reports every one of them as opaque, correct ones included — which is why the
decision lives in one module (§14.2) that both the converter and the identity
layer call.

### 14.2 `emojikit.video_decode`

One module, because "which decoder reads this file's alpha" was previously
answered in two places and they disagreed — `fingerprint()` and
`perceptual_hash()` returned different keys for the same clip.

| Function | Returns | Notes |
|----------|---------|-------|
| `decoder_available(name)` | bool | Is this ffmpeg decoder built in? Cached per name. |
| `decoder_args(path)` | `["-c:v", …]` or `[]` | The decoder for this file's **probed** codec. Raises `UndecodableVideo` when alpha fidelity cannot be ESTABLISHED; `[]` only on positive evidence that no override applies. |
| `frames_rgba(path, fps=SAMPLE_FPS)` | bytes | 64×64 RGBA frames at `fps`, alpha intact. `SAMPLE_FPS` (10) is the IDENTITY stream and is frozen — every stored content key hashes it. |
| `first_frame_bytes(path)` | bytes \| None | The first of those frames. |

`frames_rgba` is memoised on `(path, size, mtime_ns)` with a small bounded LRU.
`same_image` used to decode the same file up to six times per comparison; the
cache cut the video identity suite from 163 s to 48 s, and shortens real ingest
by the same mechanism. Keying on mtime and size — not the path alone — is what
keeps a re-encoded file from answering with its old frames.

### 14.3 Comparing two videos

`same_image(a, b, "video")` walks the timeline. It used to read frame zero and
nothing else — both halves of its two-agreement rule went through
`_first_video_frame` — so two real clips sharing ten opening frames compared
equal, and reconciliation bound the foreign clip's ids to our catalog item.

Two rules make it sound:

* **Every sampled frame pair must agree**, under the same structure-and-colour
  rule a static image gets. There is no averaging across frames: an average is
  exactly what lets one differing segment hide. A length difference beyond one
  frame of slack is a different clip, however well the opening matches.
* **Content-key equality is not proof, for video.** The key hashes the 10 fps
  identity stream, and a change shorter than one sampling interval falls between
  its frames — two clips differing in exactly ONE frame (flat blue against flat
  green, premultiplied mean delta 100.5) hash to the same key. The comparison
  therefore samples at `VIDEO_COMPARE_FPS` (30, the rate this project's own
  encoder produces) and never short-circuits on the key.

Failing closed stays the contract: a clip that cannot be read is `None`
(undecidable), never `False`.

### 14.4 `emojikit.repaint`

Baking Telegram's `--tint` into artwork, rather than publishing a set that asks
the client to flatten it. Split out of `media.py` (a different job, and that
file had reached the size ceiling); callers import `emojikit.repaint` directly.

| Function | Returns | Notes |
|----------|---------|-------|
| `parse_tint(text)` | `(r, g, b)` | `#rrggbb`, `#rgb`, or `r,g,b`. |
| `repaint_in_place(path, fmt, rgb)` | bool | Recolour a Lottie or a static in place; video is refused. |

Two shapes it must not skip, both of which used to return `True` having changed
nothing: an **animated colour** is a list of keyframe objects with `s`/`e`
rather than a flat `[r,g,b,a]`, and a **gradient** packs its colour stops
`[offset,r,g,b] × p` followed by its opacity stops `[offset,alpha]` in one flat
array — so `len // 4` as the stop count overwrites the opacity ramp with colour.
`g.p` is the stop count and is honoured when present.

### 14.5 Native catalog

Rust owns catalog schema, queries, dedup decisions and durable writes through
`native/src/catalog.rs`. Commands access it directly. Shared test references keep
the pre-port Catalog API for parity and rollback verification; there is no
production `emojikit.catalog` application. Persistent rows, signed hashes,
inclusion/order and per-family publication contracts remain unchanged (§13).

### 14.6 `emojikit.logsetup` (advanced logging)

`setup_logging(name, *, console_level=INFO, file_level=DEBUG, color=None)`
configures a console handler plus a fresh UTC file log under `logs/`, named
`<name>_YYYY-MM-DD_HH-mm-ss_UTC_<run_id>.log`. It is idempotent per process and
returns the root logger. Capabilities:

- **Per-run id** — an 8-hex id stamped on every file line.
- **Automatic secret redaction** — known secret *values* (auto-registered from
  `TELEGRAM_BOT_TOKEN`, `GENERAL_BOT_TOKEN`, `CMC_API_KEY`, …) and token-shaped
  strings/`/bot<token>/` URLs are masked in **every** record, including
  exception tracebacks. Content hashes (`s:...`) and numeric ids are **not**
  redacted. Register extra secrets with `register_secret(value)`.
- **Rich file format** — `[UTC] [LEVEL] [run_id] [logger] module:line message`;
  concise (optionally ANSI-colored on TTY) console format.
- **Uncaught-exception capture** — `sys.excepthook` + threading hook log full
  tracebacks as CRITICAL, and the traceback printed to stderr is redacted too
  (delegating to the default hook would have printed the token raw).
- **Quiet third parties** — `urllib3`/`requests`/`PIL` turned down;
  `logging.captureWarnings(True)`.
- **End-of-run summary** (atexit) — `run <id> finished in N.NNs | warnings=… errors=… critical=…`.

The everyday surface is three names, plus `record_exit_code(code)`, which the
entry points call so the end-of-run summary states the exit code:

```python
from emojikit.logsetup import setup_logging, redact, register_secret
setup_logging("myscript")
print(redact(some_text))        # mask before any manual print
```

Secret handling is covered by `tests/test_logsetup.py`, which also fails the
build if any value from `.env` appears in a git-tracked file.

### 14.7 `emojikit.telegram_api.Telegram`

Thin Bot API client (used everywhere). Key methods: `get_me`, `send_message`,
`get_sticker_set`, `download_file`, `create_emoji_set`/`add_emoji` (format-aware,
for static/animated/video), and the legacy static `create_set`/`add_sticker`.
`call(method, data=, files=)` (public; `_call` is its old name) handles flood waits (`retry_after`) and the
~2-minute `STICKERSET_INVALID` name-release delay automatically.
---

## 15. Media formats deep-dive

### 15.1 Static (`static`)

- File: PNG or WEBP, **exactly 100×100**, RGBA (transparent background).
- Built by `emojikit.make_emoji_pngs._fit_100` / `media.to_static_png`: trim fully
  transparent borders, scale to fit 100×100 with LANCZOS, center on a
  transparent canvas. SVG sources are rasterized by resvg straight to RGBA
  (gradients included).
- Telegram stores static stickers as WEBP; when downloaded they come back as
  `.webp` (the panel renders them directly in `<img>`).

### 15.2 Animated (`animated`)

- File: `.tgs` = **gzip-compressed Lottie JSON** (vector animation).
- Hard cap **64 KB**; canvas **512×512**; ≤3 s; at most 60 fps.
- **Vector only.** You cannot turn a GIF/MP4 into an animated emoji — that path
  produces a *video* emoji. `media.to_animated_tgs` only packages/validates an
  existing Lottie (`.json` or `.tgs`): load → **refuse** a canvas that is not
  512×512 (rescaling only the top-level layer moves and clips the artwork, so
  the fix is a re-export) → `json.dumps` minified → gzip with `mtime=0`
  (deterministic) → `validate_tgs` (size, gzip, required keys `v, fr, ip, op,
  layers`, canvas, frame rate, duration).
- In the panel, `.tgs` is rendered server-side to an animated WebP
  (`/preview/<key>`) and played natively by the browser as an `<img>`.

### 15.3 Video (`video`)

- File: `.webm` with **VP9** codec, 100×100, ≤3 s, 30 fps, **no audio**, **≤256 KB**.
- Built by `media.to_video_webm` with ffmpeg (see §14.1 for the exact filter).
  CRF escalates `32 → 40 → 48 → 56 → 63` until the file fits 256 KB.
- `validate_video` checks dimensions, duration, codec, and size via ffprobe.
- Requires `ffmpeg`/`ffprobe` on PATH. The panel previews video with `<video>`.

### 15.4 Detection summary

| Leading bytes | Format |
|---------------|--------|
| `1F 8B` (gzip) | animated (`.tgs`) |
| `1A 45 DF A3` (EBML) | video (`.webm`) |
| `89 50 4E 47` (PNG) or `GIF8` | static |
| `RIFF....WEBP` | static (WEBP) |

From the Bot API, prefer the sticker object's `is_animated` / `is_video` flags.

---

## 16. Deduplication deep-dive

### 16.1 The content key

`identity.content_key(path, fmt)` produces the catalog primary key:

- **static** → `"s:" + sha256(image.convert(RGBA).resize(64×64, LANCZOS).tobytes())[:32]`.
  Two byte-different files that look identical after normalization collapse;
  truly distinct images get distinct keys.
- **video** → `"v:" + sha256(<sampled frames>)`. ffmpeg samples `fps=10` over
  ≤3 s at 64×64 RGBA; visually identical videos match regardless of container.
  **A single-frame video is shorter than one sampling interval and yields no
  frames**, so an empty resample retries at the file's own frames rather than
  falling through to a byte hash — which is not a content key, and which made
  two containers of the same frame fail to dedup while a `-c copy` remux moved
  the key (WebM randomises its SegmentUID). `fingerprint()` shares this exact
  decode; it must never grow its own copy again.
- **animated** → `"a:" + sha256(<canonical Lottie JSON>)`. The `.tgs` is
  gunzipped, parsed, re-serialized with sorted keys; re-gzipped copies match.
- unknown → `"r:" + sha256(raw bytes)` — reached only when ffmpeg renders
  nothing at all. A ffmpeg **failure or timeout** is not that case: it raises
  `MediaError` and the ingest site fails that one item, because a byte-hash key
  looks valid, never dedups, and hides the timeout.

### 16.2 Perceptual near-duplicate (opt-in)

`identity.perceptual_hash` is a 64-bit **dHash** (difference hash): downscale to
9×8 grayscale, compare horizontally adjacent pixels → 64 bits. `hamming(a,b)`
counts differing bits.

Near-dup merging is controlled by `Catalog.phash_threshold`:

- `-1` (**default**) → OFF. Only exact content + `file_unique_id` dedup. This is
  what you want when faithfully copying a pack (distinct-but-similar emoji are
  kept). *History: the old default of 5 wrongly merged 80 distinct RMaccs emoji
  down to 70 — see §27.*
- `>= 0` → merge rows whose dHash is within that Hamming distance. Use only when
  you intentionally want look-alikes collapsed.

### 16.3 Empirical guarantee

`coins/check_all_packs.py` downloads **every** live sticker and reports BLANK
(≤8 visible pixels) and DUPLICATE (pixel-identical distinct ids) groups. The 29
coin packs audited **0 blank / 0 duplicate** across 5,791 stickers.

---

## 17. The ticker → id map, drift, and the content-based fix

### 17.1 What went wrong historically

The coin packs were built by uploading a frozen plan in order. A handful of plan
items **failed to upload and were skipped**. The id map was then derived by
*position* (`live[i] = plan[i]`), but every skip shifted later positions — so
from the first skip onward, `ticker_to_id.json` pointed many tickers at the
**wrong** sticker (e.g. USDT showed another coin's logo).

### 17.2 The fix: content-based mapping (`coins/remap_ids.py`)

Ignore positions entirely. Download every live sticker once, compute a small
16×16 **signature** (RGB on black, plus the alpha channel as a fourth plane —
see below for why alpha is not optional), and match each local source logo
(`<ticker>.png`) to the live sticker whose signature is nearest (L2 via a
chunked Gram matrix). Drop matches above `--max-distance` (coins never
uploaded). This is the canonical way to (re)build the map; a correct run scores
100% against the live stickers (`--max-distance 200 --min-margin 0` is the
calibration that has worked in practice — recheck the distance histogram the
dry run prints before trusting it on a changed corpus).

```powershell
.venv\Scripts\python.exe -m coins.remap_ids --emoji-dir "<coin logo folder>" --max-distance 200 --apply
```

A resumable cache (`coins/remap_live_cache.json`, gitignored) avoids
re-downloading.

Things `remap_ids.py` does **not** do for you, each learned the hard way:

* It writes only tickers that have a `<ticker>.png`, so letting it replace the
  map outright **deletes every alias** that has no file of its own (`1inchbsc`,
  `avaxc`, …). Re-derive them before writing, either from a sibling that shares
  the old map's id, or — for an alias that was never in any prior map, such as
  a chain-suffixed ticker added straight to the inventory — via
  `coins._inventory.base_ticker()`, the same resolver `alias_map.py` and
  `enhance_map.py` use.
* **The signature must carry shape, not just colour.** RGB-on-black alone is
  blind to any logo drawn in black on transparency: it flattens to a uniformly
  black square, so Aptos/Arkham/NEAR/Worldcoin/Bittensor and every other
  black-wordmark coin measured pairwise distance **0.0** — indistinguishable
  from each other. That produced a confident, entirely wrong diagnosis once
  ("128 coins have the wrong logo file"; they did not — the files were fine,
  the metric was blind). **A distance of 0.0 between two files is a claim about
  the metric before it is a claim about the files** — look at the images before
  trusting the number. `SHARED_GROUP_LIMIT` (20) still earns its place as a
  safety net for a *genuinely* corrupt or duplicated source file; just don't
  assume that is the only thing it can mean.

Take your own dated backup first. `ticker_to_id.prebroken.json` is **not** one —
it is drifted too, and scores the same as the map it was meant to repair.

### 17.3 Preventing drift in new builds

Both `coins/rebuild_dedup.py` and `build-collection` now record the **actual
upload order** and mark each item uploaded as they go, then assign real
`custom_emoji_id`s from that recorded order — never from positions. `verify_logos
--fix` is the safe tool for individual wrong logos.

### 17.4 Chain-variant / alias tickers

Many inventory tickers are the same coin on another chain (`etharb`, `bnbbsc`,
`maticusdce`, `sysevm`, `stzent`, …) and have no own logo file. `enhance_map.py`
(strip chain suffix → base) and `alias_map.py` (match by coin name) point them at
the base coin's id. This filled the NOWPayments inventory to 354/354.

---

## 18. Step-by-step worked examples

### 18.1 Copy a foreign pack faithfully, curate, republish

```powershell
# 1) Download the source pack (all distinct stickers kept; near-dup merge off):
.venv\Scripts\.\native\runtime\numera-emoji.exe fetch-pack https://t.me/addemoji/SomePack_by_bot --token-env GENERAL_BOT_TOKEN
#    -> "Pack SomePack (...): 80 stickers -> new=80 dedup=0 failed=0"

# 2) Open the curate panel and untick anything you don't want, then Save:
.venv\Scripts\.\native\runtime\numera-emoji.exe panel
#    (browser opens http://127.0.0.1:9450/ ; click cards / Shift+click ranges / Save)

# 3) Preview the publish plan:
.venv\Scripts\.\native\runtime\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN --dry-run
#    -> "static: 76 emoji -> 1 set(s) named mypacks1_by_<bot> ..."

# 4) Publish (resumable). Each finished pack's addemoji link is announced:
.venv\Scripts\.\native\runtime\numera-emoji.exe build-collection --base mypack --title "My Pack" `
    --token-env GENERAL_BOT_TOKEN
#    -> manifests written to collection/manifests/mypacks1_by_<bot>.md
```

### 18.2 Find which pack an emoji id belongs to, then download it

```powershell
# An id (bare or premium-id:<id>) resolves to its set and fetches it:
.venv\Scripts\.\native\runtime\numera-emoji.exe fetch-pack <ID> --token-env GENERAL_BOT_TOKEN
```

### 18.3 Build a video emoji pack from GIFs

```powershell
# ffmpeg must be installed. GIFs become VP9 .webm video emoji.
.venv\Scripts\.\native\runtime\numera-emoji.exe add-media --in input\my-gifs --emoji 🔥
.venv\Scripts\.\native\runtime\numera-emoji.exe build-collection --base myvid --title "My Animations" `
    --token-env GENERAL_BOT_TOKEN --formats video
```

### 18.4 Rebuild the coin id map after any pack change

```powershell
.venv\Scripts\python.exe -m coins.remap_ids --emoji-dir "<coin logo folder>" --max-distance 200 --apply
.venv\Scripts\python.exe -m coins.enhance_map
.venv\Scripts\python.exe -m coins.alias_map
.venv\Scripts\python.exe -m coins.write_manifests --out-dir "<coin archive folder>"
```
---

## 19. The Numera Emoji Mapper bot internals (`emoji-bot`)

Long-polling loop: `getUpdates(offset, timeout=50, allowed_updates=[message,
channel_post, edited_channel_post, my_chat_member])`. Each update is dispatched
to `handle_update`. Errors in one update are caught and logged so the bot never
dies on a single bad message.

Pure, unit-tested helpers:

- `extract_custom_emoji_ids(message)` — ordered, de-duplicated `custom_emoji_id`s
  from `entities`, `caption_entities`, **and `quote.entities` /
  `external_reply.quote.entities`**. The `quote` field (Bot API `TextQuote`) is
  set when the user manually quotes part of a message they're replying to; per
  the Bot API, only bold/italic/underline/strikethrough/spoiler and
  **custom_emoji** entities are preserved inside that quote. Without scanning
  `quote.entities`, premium emoji inside a quoted reply were silently dropped
  (only entities on the reply's own text were seen). Repeats (same id, whether
  inside the quote, the reply text, or both) are de-duplicated to one occurrence.
- `build_payloads(ids, labels, rich=True)` — returns a **list of
  `(html_text, inline_keyboard)`** payloads. Each message has a single collapsed
  `<blockquote expandable>` where every line is
  `<tg-emoji emoji-id=id>fallback</tg-emoji> <code>id</code>` when `rich`
  (renders the **real premium emoji**); each `<code>` is tap-to-copy (mobile).
  Copy is provided by `copy_text` **“Copy”/“Copy a-b” inline button(s)**
  (`_copy_keyboard`) covering that message's ids; Telegram caps each button at
  256 chars (~12 ids), so a message with more ids gets several chunked buttons
  that together cover all of it.
  `_batch_ids` splits the id list by the **message**-length budget
  (`MSG_MAX`=3500 chars, mode-independent ~110 chars/id) so as many ids as
  possible share one message — batching by the smaller button limit was tried
  and reverted because it fragmented a 50-id reply into 5 short messages instead
  of ~2; rich/plain renders still align 1:1 batch-for-batch. Multi-message
  replies are labelled "part i/n". `send_reply` sends the rich version and, if a
  message is rejected (a custom emoji the bot can't render), automatically
  re-sends that message with `rich=False` (fallback chars only).
- `enrich_labels(tg, ids)` — `getCustomEmojiStickers` (≤200/call) → `id → emoji char`.

Behaviour by chat type:

| Incoming | Action |
|----------|--------|
| `/start`, `/help`, `/menu` (private) | Send the help/menu text. |
| Private message with premium emoji | Reply with the two-format collapsed quotes + Copy-all button. |
| Group message with premium emoji | DM the **owner** the IDs (header = group title). |
| Channel post with premium emoji | DM the **owner** the IDs (header = channel title). |

"Tap to copy" = Telegram `InlineKeyboardButton.copy_text` (Bot API 9.0); no
callback is needed — the client copies locally. `setMyCommands` registers
`/start` and `/help` in the bot's menu.

The same extraction is ported to TypeScript in `worker/src/emoji.ts` (§12.9),
including the `quote` / `external_reply.quote` entities — among the few that
survive into a partial quote, and missing them silently drops premium emoji
inside quoted replies. **A token can serve `getUpdates` or a webhook, never
both**: point a webhook at the Worker and this poller goes deaf on that token.

Operational notes: run exactly one instance (two concurrent `getUpdates` cause
**409 Conflict**). For groups the bot needs admin or privacy-mode off to see
messages; for channels it must be an admin to receive `channel_post`, and the
channel's id must be listed in `BOT_ALLOWED_CHANNEL_IDS` (in `.env`, and as a
Worker secret). A channel post has no sender to check, so an unlisted channel is
ignored; empty means the bots answer no channel at all.

---
