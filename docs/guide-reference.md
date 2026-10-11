[Complete guide index](GUIDE.md)

## 21. Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `GENERAL_BOT_TOKEN not set` / `getMe failed: Not Found` | Token missing/invalid in `.env`, or a stale shell env var shadowing it. | Put a valid token in `.env`; in PowerShell clear a leftover var: `Remove-Item Env:\GENERAL_BOT_TOKEN`. |
| `ffmpeg not found` | ffmpeg/ffprobe not on PATH. | `winget install Gyan.FFmpeg` (only needed for video). |
| `cannot find ft2build.h` / reportlab build fails | An old checkout still pinning `reportlab<4`, which had no cp312 wheel. | Pull latest: the reportlab/svglib stack was replaced by `resvg-py`. |
| `Python int too large to convert to SQLite INTEGER` | Old catalog code stored an unsigned 64-bit phash. | Fixed in `emojikit/catalog.py` (signed storage); pull latest. |
| Fetch reports `dedup` on a pack with no real duplicates | Near-dup merging was on. | It's **off by default** now (`--phash-threshold -1`); pull latest or pass `-1`. |
| Panel images 404 | `/img/<key>` not URL-decoded. | Fixed; pull latest. |
| Bot replies nothing in a group | Privacy mode on / not admin. | Make the bot admin or disable privacy mode in BotFather. |
| Bot stops with **409 Conflict** | Two `getUpdates` consumers. | Run only one `emoji-bot` instance. |
| `STICKERSET_INVALID` right after deleting a set | Telegram locks a freed set name ~2 min. | The client auto-waits and retries; just let it run. |
| `flood wait Ns` | Telegram rate limit. | The client honors `retry_after` automatically; let it continue. |
| Animated card shows ▶ but never plays | Bad/non-standard `.tgs`, or JS blocked. | Other cards still work; that single TGS just won't render. |
| Wrong logo for a coin | Source logo is a different token with the same ticker. | `python -m coins.verify_logos --fix --only <ticker>` with the correct image. |
| Scrambled ticker→id map | Position-based mapping (legacy). | Rebuild with `coins\remap_ids.py ... --apply`. |
| `canonical_map.lock is held by pid …` | Another coin tool is mid-write of `ticker_to_id.json`. Every writer holds one lock across the whole read-modify-write, so neither can lose the other's ids. | Let the other tool finish, then re-run. The map was **not** modified. |
| `publish_<base>.lock is held by pid …` **and that process is gone** (you stopped a publish, or it crashed) | The lock outlives the run that made it. It is reclaimed once the holder is provably dead AND the lock is over two minutes old — the delay covers the moment between a lock being created and its owner being written into it. | Wait ~2 minutes and re-run the same command. It resumes where it stopped; nothing is re-uploaded. |
| `the pending replacement … was recorded against map A, not this run's map B` | A `verify_logos --fix` was interrupted; its intent is bound to the exact `--map`/`--state` it started against. | Re-run with the **original** `--map`/`--state` so it can be resolved, or review the pack and delete `coins\verify_logos_intent.json` deliberately. |
| `… exists but its first sticker is not <x>.png; refusing to adopt` | A set of that name exists but this run did not create it (leftover family, or someone else's). Existence is not identity. | Rename the family, or delete the stale set, then re-run. |
| `position N could not be examined …` from `build_collection` | A live sticker could not be downloaded or hashed, so the publisher cannot tell whether it is one of ours. Guessing "not ours" is what publishes a second copy. | Usually transient — re-run. If it repeats on **video or animated** sets, ffmpeg is off PATH: content hashing needs it, and without it every such sticker is unexaminable. Install ffmpeg (see [§3 Setup](#3-setup-one-time)). |
| `Bad Request: wrong file type` on an **animated** item | Telegram's **uploader** refuses a subtract mask (`masksProperties[].mode == "s"`); its **player** renders one happily. So a sticker can be live in a published pack for years and still be refused when you upload the same bytes -- proven by downloading one from a live pack and sending it straight back untouched. Add masks (`"a"`) are fine. Nothing local can see it: the file is valid gzip, valid Lottie, 512x512, in-spec fps and duration. | `validate_tgs` now refuses it at ingest and names the layer, so this should no longer reach a publish. If it does, the publisher records it as a **skip** with the reason rather than retrying it on every future run. The only repair is re-exporting the animation without that mask, which changes the artwork -- an owner decision, never automatic. |
| CI red on push | A check failed (install/import/checks/dry-run). | `gh run view <id> --log-failed`; reproduce locally with `.\scripts\check.ps1`; the matrix is Python 3.11, 3.12 **and** 3.14, so check which one failed. |
| `ModuleNotFoundError: No module named 'numpy'` from `coins\remap_ids.py` | numpy is the coin extra, not part of the core manifest. | `pip install -r requirements-coins.txt`. |
| `SuiteIsHermetic ... the hermetic guard was NOT active` | The suite was started without `-t .`, so `tests/__init__.py` never ran. | Use `.\scripts\check.ps1`, or the exact form the failure message prints. |

---

## 22. FAQ

**Q: Can I turn a GIF into an *animated* emoji?**
No. Animated emoji are vector (Lottie). A GIF/MP4/WEBM becomes a **video** emoji.

**Q: Does fetching the same pack twice cost bandwidth?**
No. `file_unique_id` is remembered; already-seen stickers are skipped.

**Q: How do I keep distinct-but-similar emoji?**
Leave near-dup merging off (default). Only exact-content duplicates merge.

**Q: Can the bot scan a whole channel's history?**
No. The Bot API only delivers posts received after the bot joined.

**Q: Where do the share links go?**
To `PACK_LINKS_CHAT_ID` (a channel id or `@name`; the bot must be an admin
there), or to the owner's private chat (`PACK_OWNER_USER_ID`) when it is unset —
from this process, or through the Worker when `WORKER_PUBLISH_URL` is set
(§12.9). All three publishers use `emojikit.announce.announce_packs`.

**Q: Is my data uploaded anywhere?**
Only to Telegram, via your bots. `collection/`, `logs/`, `.env`, `secrets.md`
stay local and are gitignored.

**Q: Can one set hold both static and animated emoji?**
Yes — since Bot API 7.2 (March 2024) a single custom-emoji set may mix static,
video and animated stickers (each `InputSticker` carries its own `format`;
verified live). `build_collection` still splits into `s`/`v`/`a` sets by default
for organization, but the static brand logo is added as the first emoji of every
set regardless of the set's format.

---

## 23. Glossary

- **custom_emoji_id** — Telegram's id for a premium emoji; what consumers store.
- **file_unique_id** — stable per-sticker id used for fast pre-dedup.
- **content_key** — our normalized content hash; the catalog primary key.
- **dHash / perceptual hash** — 64-bit fingerprint of an image's gradient; close
  Hamming distance ≈ visually similar.
- **TGS** — gzip-compressed Lottie JSON = an animated sticker/emoji.
- **set / pack** — a Telegram sticker set (≤200 custom emoji).
- **plan** — the append-only list of items queued for a pack family; they upload in the panel's current order.
- **included flag** — per-item publish toggle set by the Curate emojikit.panel.
- **drift** — when live sticker order ≠ assumed order, scrambling a position map.

---

## 24. Telegram Bot API methods used

| Method | Where | Purpose |
|--------|-------|---------|
| `getMe` | everywhere | Auth check + bot username. |
| `getStickerSet` | fetch/audit/build | List a set's stickers. |
| `getFile` + file download | fetch/audit/remap | Download sticker bytes. |
| `uploadStickerFile` | build_pack | Upload a sticker file. |
| `createNewStickerSet` | build | Create a new custom-emoji set. |
| `addStickerToSet` | build | Append to a set. |
| `replaceStickerInSet` | verify_logos / fixes | Swap a wrong sticker in place. |
| `deleteStickerSet` | rebuild | Remove old packs before a clean rebuild. |
| `setStickerPositionInSet` | sync_order | Move a sticker; ids survive. |
| `getCustomEmojiStickers` | bot / resolve | id → emoji char + `set_name` (≤200/call). |
| `sendMessage` (+ `entities`, `reply_markup`) | bot / links | Send IDs, copy buttons, links. |
| `deleteMessage` | bot housekeeping | Remove old bot messages. |
| `setMyCommands` | bot | Register `/start`, `/help`. |
| `getUpdates` | bot | Long polling. |

Tap-to-copy uses Telegram's native `<code>` entity copy (works for a single id
and for a whole multi-line `<code>` block); collapsed quotes use
`<blockquote expandable>`. Emoji sets cap at 200 stickers (a brand logo, when
added, counts as the first of those 200).

---

## 25. Data files reference

| Path | Tracked? | What |
|------|----------|------|
| `.env` | no | Tokens + owner id. |
| `secrets.md` | no | Local secret registry. |
| `collection/catalog.db` | no | The content-addressed catalog. |
| `collection/media/{static,video,animated}/` | no | Downloaded/built media. |
| `collection/manifests/<set>.md` | no | Per-pack manifest (name + id). |
| `collection/publish_<base>.json` | no | Publish state (sets, sent links, keys, skipped). |
| `collection/publish_plan_<base>.json` | no | Append-only per-format record of what is queued (not its order). |
| `collection/pack_plan.json` | no | The panel's move plan: which emoji should change pack, and which are parked. Written on every Save. |
| `coins/ticker_to_id.json` | no | Canonical ticker → custom_emoji_id map. The owner's own pack data, not part of the tool. |
| `coins/keywords.csv` | **yes** | ticker → name/keywords. |
| `coins/currency-emoji-inventory.md` | no | Inventory source. Names live custom-emoji ids, so it stays local. |
| `coins/currency-emoji-inventory.filled.md` | no | Generated, id-filled inventory. |
| `coins/rebuild_dedup_state.json` | no | Live coin pack set names/order. Written by `rebuild_dedup.py` **and** by the providers when they top the family up — they add their own `provider_in_flight` intent and `provider_added` tally beside the rebuild's keys, under the same pack-family lock. |
| `coins/rebuild_dedup_plan.json` | no | Frozen coin upload plan. |
| `coins/remap_live_cache.json` | no | remap signature cache. |
| `coins/ticker_to_id.<date>.bak.json` | no | Dated snapshot taken before a remap. **This is the revert target.** |
| `coins/unresolved_logos.json` | no | Tickers deliberately left unmapped because their source PNG is not their own logo. |
| `coins/ticker_to_id.prebroken.json` | no | Historic, and **not** a usable restore point despite the name — measured against the live stickers it scores the same as the map it was supposed to repair. |
| `logs/*.log` | no | Per-run UTC logs. |
| `state_<base>.json` | no | `build_pack` resume state. |

---

## 26. Security & secrets

- Tokens and the owner id live only in `.env` (and a local `secrets.md`
  registry). Both are gitignored and must never be committed, printed, logged,
  or pushed.
- `emojikit.logsetup` raises `urllib3`/`requests` to WARNING so the Telegram API
  URL (which contains the token in the path) is never written to a log file, and
  `redact()` masks token-shaped strings in any message it does log.
- A leaked bot token must be revoked via @BotFather (`/revoke`) and replaced in
  `.env`.
- The repository is **public** and licensed **GPL-3.0-or-later** (`LICENSE`).

---

## 27. History of real bugs fixed (and the guards that prevent them)

These were found with real data; the guards must not regress.

1. **Scrambled ticker→id map** — position-based mapping shifted by skipped
   uploads. Fix: content-based `remap_ids.py`; builds now record actual upload
   order. (§17)
2. **Wrong source logo (e.g. Solana)** — a ticker collision pulled a different
   token's logo. Fix: `verify_logos --fix --only`, plus replacing the source
   asset. (§7.2)
3. **64-bit phash overflow** — unsigned dHash exceeded SQLite's signed range and
   dropped rows. Fix: signed storage + restore. (§13.4)
4. **Over-aggressive dedup** — perceptual merge (threshold 5) collapsed 80
   distinct emoji to 70. Fix: near-dup merging **off by default**. (§16.2)
5. **Blank emoji from gradient SVG** — the old svglib backend could not paint
   gradients and returned a fully transparent image. Fix: blank detection +
   raster fallback, then replacing the backend with resvg, which renders them.
   (§15.1)
6. **reportlab 3.12 wheel** — `reportlab<4` had no cp312 wheel, which pinned the
   whole project to Python 3.11. Fix: the reportlab/svglib stack was removed.
   (§3)
7. **Token in logs** — urllib3 DEBUG logged the bot-token URL. Fix: silence those
   loggers + `redact()`. (§26)
8. **Panel image 404** — `/img/<key>` wasn't URL-decoded. Fix: `unquote`. (§20)
9. **Resume duplicate** — positional resume could re-upload after a skip. Fix:
   dedup by the per-item committed `uploaded` flag + persisted `skipped`. (§6.4)
10. **Two publishers, one lock** — the pack-family lock decided ownership by
    reading a file, comparing a token and *unlinking* it, so its own stale-lock
    recovery could seat two processes at once; two real processes held one lock
    for 1.54 s. Fix: an OS-level lock (`msvcrt.locking` / `flock`) held for the
    whole critical section, and the lock file is never unlinked. (§12.5)
11. **A stranger's picture given our identity** — recovery accepted the single
    nearest candidate by perceptual hash, and dHash is a *grayscale structure*
    hash: an opaque red square and an opaque blue one are zero apart. Fix: a
    perceptual match may only NOMINATE; content verification decides, and
    "more than one" and "could not examine" stay distinct from "no match".
    (§13.6)
12. **Preflight claimed an acceptance it never got** — the counter counted
    attempts, so a run in which every check failed at the transport printed
    "all accepted" and exited 0. Fix: four counters, and an unreachable
    Telegram is its own outcome. (§12.5)
13. **Alpha applied twice** — `fit_100` pasted the image using itself as the
    mask, which composites against the transparent canvas beneath: colour came
    back multiplied by alpha and alpha squared. `(255,0,0,128)` fitted to
    `(128,0,0,64)`. Fix: an unmasked copy onto a fully transparent
    destination. (§14.1)
14. **Video identity ignored alpha, and disagreed with itself** — the default
    `vp9` decoder drops the alpha layer in silence, so two clips differing only
    in opacity shared one content key; and the two fingerprint APIs sampled
    differently, so one clip had two hashes. Fix: one `video_decode` module
    that picks the decoder from the probed codec. Existing rows are migrated
    deliberately by `scripts/identity_repair.py`, never implicitly. (§14.2,
    §13.6)
15. **A stale tab spoke for the whole catalog** — `/api/save` carries the full
    selection, so a page opened before another change silently re-included rows
    it had never seen, and answered `{"ok": true}`. Fix: the request states its
    scope; a page too old to say gets 409. (§20)
16. **A Lottie timeline that was not a number** — `fr = NaN` passed
    `validate_tgs`, because every comparison against NaN is False. Fix: an
    explicit `math.isfinite` check on `fr`/`ip`/`op`. (§14.3)
17. **Video identity degraded open, and cached the degradation** — a missing
    `libvpx-vp9`, a failed codec probe and a container naming no codec all
    answered "no decoder needed", which drops VP9's separate alpha layer; one
    transient probe error was then cached, so every later decode of that file
    in the process lost its alpha silently. Two clips differing only in opacity
    share one key, and `Catalog.add` merges them and deletes the file it merged
    away. Fix: `UndecodableVideo` — establish fidelity or refuse. (§14.2)
18. **A video was compared at one frame** — two clips sharing ten opening frames
    compared equal, and so did two differing in exactly one frame. Fix: walk the
    timeline at 30 fps, and stop treating content-key equality as proof for
    video. (§14.3)
19. **A migration moved three tables and stopped** — the owner's state file was
    left naming 51 keys the catalog no longer had, which `reconcile_set` reads
    as a reordered pack; the derived hashes stayed stale; the archived filenames
    kept the old key. Fix: one versioned migration over every durable reference,
    with a journal. (§13.6)
20. **The migration backup was not WAL-safe** — `copy2` of the `.db` alone
    opened as a database with `no such table: items`. Fix: SQLite's online
    backup API, verified before anything is changed. (§13.6)
21. **One verified candidate beat an unreadable rival** — with two candidates
    matching, deleting one candidate's FILE made the resolver answer "unique"
    with the survivor. Removing evidence must not promote a guess. (§13.6)
22. **The panel confused "a save is in flight" with "there is unsaved work"** —
    an acknowledgement of an older snapshot cleared the dirty state, said
    "Saved" and let the tab close on newer ticks. (§20)
23. **A refusal retrying could not fix was retried forever** — and the
    five-second heartbeat walked past the backoff, while a fetch that never
    resolved claimed the queue for the life of the page. (§20)
24. **A Worker error could echo a bot token** — every Bot API URL embeds it and
    a transport failure names the URL, which reached the HTTP 502 body and every
    log sink. (§12.9)

---

## 28. Keeping this guide in sync

When you add or change any command, flag, file, table, or workflow:

1. Update the matching section here **in full** (0 → 100, with an example).
2. Update `README.md` if the change is user-facing.
3. Run the unit tests + a real run; verify UI in a browser when relevant.
4. Commit and **push to `main`** so GitHub stays in sync.

*End of guide.*
---
