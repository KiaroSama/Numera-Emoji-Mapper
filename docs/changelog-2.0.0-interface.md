## [2.0.0] - 2026-09-29

### Breaking

- **Renamed to Numera Emoji Mapper**: the environment switches are now
  `NUMERA_EMOJI_MAPPER_*`; the old names are not read.
- **Operator identities are required configuration**: brand-logo bots and
  path, pack bases, the coin pack title and the archive folders come from `.env`
  with no defaults; a tool that needs an unset one stops and names it.
- **The Worker and its D1 log store are renamed**; each operator creates their
  own D1 database (see `worker/README.md`, first deploy).
- **Channel posts need `BOT_ALLOWED_CHANNEL_IDS`**: unset means the bots answer
  no channel at all.
- **Coin tools run as modules** from the repository root
  (`python -m coins.<tool>`); running `coins\<tool>.py` by path no longer works.
- **Publishing follows the panel's current order**; a dry run or a preflight no
  longer writes the plan.
- **A test file run directly refuses to start**; run suites with
  `python -m unittest tests.<name>`.

### Changed - the curate panel stays light on a big catalog

- **At most 24 cards animate**, the ones nearest the centre of the screen. The
  new **All visible** switch animates every visible card, as before (heavier).
  A scroll freezes and thaws only the cards that were playing.
- **Video cards are light animated previews by default.** The **Real video**
  switch brings back `<video>` players; players are never created or torn down
  mid-scroll.
- **The preview warm-up renders exactly what the page asks for** - the 104 px
  and 72 px tiers for animated and video cards, plus the holding tray's still -
  skips what is cached, and logs rendered / already cached / failed. The render
  bound is max(2, min(6, CPU count - 2)) instead of 2.
- **Less work per frame**: CSS variables are written only when they change,
  Deselect all and holding a group each walk the list once, the holding tray is
  windowed like the grid, the grid mounts half a screen or three rows beyond the
  viewport (whichever is smaller), compact cards carry only header and
  thumbnail, the pick ring pauses while the page scrolls or drags, and the
  checker backdrop is one gradient.
- **The panel starts without waiting on Telegram**: the browser opens straight
  after the port is bound and the bot name is looked up in the background.
- **The server keeps connections alive** (HTTP/1.1, 30 s idle timeout) and
  queues up to 64 connections, so a page loading its scripts and a screenful of
  thumbnails is no longer refused.
- **A sandbox copies the preview cache** (best effort), so it does not
  re-render everything.

### Added - import a draft back into the panel

- **Import draft** (next to Reset all) reads an exported draft: keys the page
  holds take the draft's decisions, unknown keys are dropped, new emoji stay at
  the end. It never posts and it is undoable; the toast counts kept, not in this
  catalog, and new.

### Fixed - a busy catalog, framing, and three publish-safety gaps

- **A refresh that cannot open the catalog keeps the last view** and says
  "catalog busy (a publish may be running) - showing the view from HH:MM UTC",
  instead of looking like a catalog with no news.
- **The panel refuses to be framed and stops content sniffing**: every response
  carries `X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`;
  the page also `X-Frame-Options: DENY` and `frame-ancestors 'none'`.
- **A test file run directly refuses** (`python tests\test_x.py` skipped the
  credential scrub and the network block); run suites through `check.ps1`.
- **An unusable brand logo stops the publish before any set is created**,
  instead of producing packs without their logo.
- **`pack_archive --sync` commits each move as it goes**, uses Windows-safe
  folder names, and only slot 1 can be the logo.

### Changed - coin tools run as modules

- **Every coin tool runs from the repository root as `python -m coins.<tool>`**,
  e.g. `python -m coins.rebuild_dedup build`; the docs no longer show
  `python coins\<tool>.py`.

### Fixed - the publish follows the panel's current order

- **Pending emoji upload in the order the panel shows when you publish.** The
  first run of a pack family -- a dry run included -- used to freeze the order,
  so rearranging the next pack's candidates in the panel changed nothing.
- **`--dry-run` and `--preflight` no longer write the publish plan.**

### Security - channel posts need an allowlist

- **The bots answer a channel post only from a listed channel.** A channel post
  has no sender to check, so any channel that made a bot its administrator could
  make it message the operator under a title of its choosing. The new
  `BOT_ALLOWED_CHANNEL_IDS` key (in `.env` and as a Worker secret) lists the
  channels; empty means channel posts are ignored.

### Changed - dependencies on their latest releases

- **Panel browser tests** run on playwright 1.63.0 (Chromium 153).
- **urllib3 2.8.0**, which fixes two high-severity issues, under requests.
- **numpy floor 2.4**: 2.4.6 on Python 3.11; newer Pythons get 2.5.
- **Worker tooling**: wrangler 4.142, vitest 5.0.2, current workers-types.
- **CI** tests Python 3.14 as well, runs the Worker on Node 24 LTS, and uses
  actions/checkout v7.

### Fixed - renaming the project folder no longer breaks the catalog

- **Media paths inside the data folder are stored relative to it.** They were
  absolute, so renaming the project folder left every unarchived emoji pointing
  at a folder that no longer existed and the panel showed broken thumbnails.
  Archive paths on another drive stay absolute.
- **Older catalogs convert themselves once when opened**, and every tool that
  reads the table directly resolves a path the same way the catalog does.

### Changed - Worker log store uses far less D1

- **Only warnings, errors and publishes are stored in D1.** The routine line of
  every handled message (91% of rows) and unauthorised webhook hits go to the
  console and Workers Logs, which is now enabled.
- **A stored line costs 1 row written and 0 read (was 4 and 2).** The log table
  is rebuilt without its two unused indexes and without AUTOINCREMENT; every
  existing row is kept.
- **The cap is the newest 5 000 rows**, evicted by an id range that reads only
  the rows it deletes, instead of a 10 MB budget that summed the whole table.

### Changed - new project logos

- **New logos.** The project logo (README, general workflow, the curate panel's
  tab and header icon) and the coin logo (coin workflow) are the new designs;
  the old images are gone from the current files.
- **The panel's icon URL is versioned by the icon's own content.** It was served
  immutable-cached without a version, so a replaced logo kept showing the old one
  in every browser that had opened the panel before.
- **README badges:** sixteen verified badges, including support/donations linking
  to the Donate section.

### Changed - the project is Numera Emoji Mapper, and it names no operator

- **Renamed to Numera Emoji Mapper** everywhere: documentation, interface text,
  the launcher, the Worker package, asset names and the environment switches,
  which are now `NUMERA_EMOJI_MAPPER_*`. The old names are not read any more.
- **Your identities are your configuration.** The brand-logo bots, the logo, its
  keywords, the collection and coin pack bases, the coin pack title and the
  archive folders come from `.env` (`BRAND_LOGO_BOTS`, `BRAND_LOGO_PATH`,
  `BRAND_LOGO_KEYWORDS`, `COLLECTION_PACK_BASE`, `COIN_PACK_BASE`,
  `COIN_PACK_TITLE`, `EMOJI_ARCHIVE_DIR`, `COIN_EMOJI_DIR`; see `.env.example`).
  None has a default: a tool that needs an unset one stops before changing
  anything and names it. `BRAND_LOGO_BOTS` set but empty means no bot gets a
  logo. The repository no longer ships a brand logo; keep yours in the
  git-ignored `private/`.
- **A listed bot whose logo file is missing now stops the publish** instead of
  warning and publishing packs without their first emoji.
- The pack archive names the logo `001_logo.png`; a folder archived under an
  older logo name has that file renamed on the next `--sync`.

### Fixed - a sandbox leaves the source alone, and a state write leaves other files alone

- **Making a sandbox no longer writes to the real catalog.** The source was
  opened through `Catalog`, whose constructor switches it to WAL, creates missing
  tables and commits -- so cloning an older catalog upgraded it, even when the
  clone was then refused. It is opened read-only now, under the writer lease.
- **The sandbox holds the right media.** A relative path resolves against the
  project root, as every tool does, instead of preferring a file beside the
  source; each copy is checked by SHA-256 on both sides instead of by size; and
  file names are the hex of the content key, so two keys can no longer share one.
- **A sandbox is owned from the moment it is built until it is cleaned up.** The
  lease now lives outside the sandbox and is taken first; before, a sweep could
  delete a finished clone in the gap before it was served. Marker protocol 2;
  version-1 directories are preserved, not guessed abandoned.
- **Writing a state file cannot damage an unrelated one.** Each write uses its own
  exclusive temporary file instead of a shared `<name>.tmp`, which was
  overwritten if it already existed and collided between two writers.
- **A rewrite keeps the file's permissions** (the exclusive temporary file is
  owner-only, and publishing it narrowed every rewritten file), and **Windows'
  transient refusal to replace a file is retried** for up to a second: two
  writers to one file lost a write in 13 of 40 runs without it.

### Fixed - an emoji joins the pack it was dropped on, and a selection can be seen

- **A drop reads the card you aimed at, not the card it landed above.** Those
  differ at every pack boundary: dropping a held emoji on pack 4's FIRST card
  stamped it pack 3 and filed it as pack 3's last slot. The destination is now
  taken from the card under the pointer while the drag is live.
- **A drag inside the grid re-stamps too.** Only drops from the holding tray
  did, so an emoji carried from pack 1 into the middle of pack 4 kept pack 1 and
  SPLIT pack 4 into two runs with a false separator between them.
- **Nothing is inferred when nothing was aimed at**, and an emoji dropped into a
  run with no pack number yet keeps no number, so it continues that run.
- **A click anywhere on a grid card selects it** in selection mode, with
  Shift-click taking the range. The grid ignored the click entirely, leaving a
  1.7em box as the only target on a 140px card; the tray was fixed for this and
  the grid was not.
- **A picked card wears a bright moving ring.** The flat inset outline read as
  one more dark line among the format accent colours. The rotation is a
  transform, so it costs no repaint, and it stops under `prefers-reduced-motion`.
- A drop that changes only the pack now records history, so it can be undone.

### Fixed - the holding tray can be selected from and moved in bulk

- **A pick box now means what it shows.** It drew the same check whether or not
  the card was picked and only changed colour, so an unpicked box looked ticked
  and two states were told apart by colour alone. Unpicked is empty now; the
  check appears only when picked, on grid cards and held cards alike.
- **A held card is selectable by clicking it.** Only a ~17px box responded, so a
  click on the card did nothing and selecting a run meant aiming twice.
  Shift-click takes the whole range. Dragging is unaffected -- a drag emits no
  click, so the gestures cannot collide.
- **Several held emoji move in one drag.** The machinery was already right; it
  was unreachable, because a multi-selection could not be made by hand.
- **The test sandbox starts again.** It held the clone's own catalog lock for
  the server's lifetime, so the panel it started was refused its own database
  and exited; and it cleared the environment before reading it, leaving the
  process without even `PATH`. It had served nothing since it shipped.


### Fixed - a save is answered for what it actually submitted

- **A stale Save is refused instead of quietly shrunk.** The panel intersected
  the tab's scope with the server's shared view and applied whatever was left,
  answering `{"ok": true}` for a decision it had silently narrowed. The whole
  submitted scope is now validated against the live catalog under the writer
  lease: it applies completely or returns 409 with nothing written, and the
  draft can be exported before reloading.
- **The webhook validates its envelope before dispatching.** Parsing proved the
  body was JSON, never that it was an update, so a malformed authenticated body
  reached the dispatcher. Only `update_id` is required, so unknown future update
  types stay valid.
- **The test sandbox no longer hands the panel real credentials.** It cloned the
  catalog and then passed its own environment through untouched, so a sandboxed
  panel still reached live Telegram as the real bot. It now runs with dotenv
  reading off and every credential-shaped variable stripped.
- **The test sandbox can no longer be argued into serving the real catalog.**
  Unknown options were forwarded to the panel AFTER the wrapper's own
  `--data-dir`, so one passed through won. Arguments are an allowlist now, with
  abbreviations refused and nothing forwarded.
- **The sandbox clone shares no bytes with the original.** Media was
  hard-linked, a path outside the collection was left pointing at the owner's
  archive, and the database was copied as a file, which skips an uncheckpointed
  WAL. It is now an online snapshot plus per-file copies with every path
  rewritten, `pack_plan.json` included, and it refuses rather than finishing a
  clone it cannot make faithfully.
- **Starting a sandbox no longer deletes another one that is running.** The
  start-up sweep removed every directory matching its name prefix. It now
  requires both a directory-bound `.sandbox-owner.json` and the ability to take
  that directory's lock, and it keeps the lock file when it reclaims.


### Fixed - saved curation survives, and the writes that carry it are bounded

- **A saved pack move now survives a reload.** The page rebuilt its layout from
  live Telegram membership alone, so an emoji moved to another pack reappeared
  at its old one and the next save erased the move. Intent is read back from
  `pack_plan.json` and overlaid on the render; the server's own view of what is
  live is unchanged, so `from_pack` still means what it says.
- **A partial view no longer discards decisions it cannot see.** A page showing
  part of the catalog replaces only its own scope and merges the rest. The plan
  gained `targets` (the authoritative intent), `known` and `excluded` (the scope
  those decisions were made in) and `logo_slots`; `moves` is now derived from
  `targets`. Plans written by an older panel are still read. See
  `docs/GUIDE.md` § "The move plan" for the full shape.
- **A Save carries one immutable body.** The request used to be rebuilt from the
  live model on every attempt, so a failed save for pack 1 could retry as pack 2
  after an unsaved edit. Pack-only edits now count as unsaved work for the dirty
  marker and the close warning, and a refusal is matched to the body it refused.
- **An HTTP 200 is no longer taken as proof on its own.** A success status with a
  malformed, null or never-finishing body cleared pending work and said "Saved".
  The client now requires a complete acknowledgement and retries otherwise.
- **The inclusion write and the plan write share one catalog lease.** Releasing
  it between them let maintenance re-key the database and then receive a fresh
  plan full of obsolete keys. Keys are validated against the database while it
  is owned; a plan I/O failure answers 503 instead of acknowledging.
- **Migration and rollback now cover the curation plan.** Their inventory
  enumerated `publish_*.json` only, so a re-key left the plan naming keys that
  no longer existed and the rollback bundle contained no plan at all. One shared
  inventory remaps only schema-defined ID fields; free text is left alone.
- **SQLite backups stop waiting forever.** `sqlite3.connect(timeout=...)` does
  not bound `Connection.backup`'s own BUSY retry loop, so a locked database
  could block a snapshot indefinitely. Capture is bounded at 30 s. Rollback
  writes onto the live catalog and is bounded far higher, because abandoning a
  restore half-written is worse than waiting out a competing reader.
- **A video sample cache could return another file's pixels.** Its key used a
  relative filename plus size and mtime, so two `clip.webm` files with matching
  metadata in different working directories collided. Keys now use the resolved
  path and include the FFmpeg identity the samples were decoded with.
- **Native Windows now has its own CI job**, covering locks, migration and
  restore, HTTP persistence, video identity and the CLI contracts — paths that
  Linux and WSL do not exercise. Which suites belong there is a judgement rather
  than a property of the code, so each one declares `RUNS_ON_NATIVE_WINDOWS` and
  `tests/test_ci_coverage.py` enforces the match in both directions: a marked
  suite missing from the job, and a name in the job that no longer marks itself,
  each fail.
- **A saved plan no longer grows forever.** Decisions about emoji the catalog no
  longer holds are dropped on the next save, checked against the database under
  the same lease that writes the plan. Previously every out-of-scope key was
  kept unconditionally, so a long-lived catalog accumulated dead entries that
  still counted toward a pack's total and could report a false `over_capacity`.

### Changed - relicensed under the GNU GPL v3 or later

- **`LICENSE` is now the GNU General Public License v3**, replacing the previous
  proprietary All Rights Reserved terms. Everyone may use, study, modify and
  redistribute this project, provided derivative works carry the same freedoms.
  The licence text is the canonical one, verbatim and unedited.
- The declaration was corrected everywhere it appeared, not just in `LICENSE`:
  `README.md` carries the standard notice, `docs/GUIDE.md` no longer calls the
  repository private or All Rights Reserved, `worker/package.json` gained
  `"license": "GPL-3.0-or-later"`, and the contributing guide and pull-request
  template now welcome outside contributions under the same terms instead of
  requiring a written agreement and a copyright assignment.
- Every runtime dependency was checked for compatibility first: requests
  (Apache-2.0), Pillow (MIT-CMU), resvg-py (MIT), numpy (BSD-3/0BSD/MIT/Zlib/
  CC0) and rlottie-python (LGPL-2.1, upgradeable to the GPL by its own terms).
  Third-party logos and provider data keep their own licences, which this change
  does not and cannot alter.

### Changed - CI runs on GitHub-hosted runners

- **The workflow moved from a self-hosted WSL runner to `ubuntu-latest`.** The
  self-hosted runner existed because this account's hosted minutes are blocked
  for private repositories; they are free for public ones, so publishing the
  repository removed the reason. A self-hosted runner is also incompatible with
  a public repository on purpose: a public repo accepts pull requests from
  anyone, and a fork's workflow would execute that code on the owner's own
  machine, so the runner was deregistered before publication.
- Consequences now baked into the jobs: ffmpeg and Chromium's system libraries
  are **installed** by the jobs rather than assumed from a global setup, and the
  ffmpeg step verifies the binaries actually resolve afterwards — apt can answer
  a mirror outage by installing nothing and still exiting 0, which surfaces
  three steps later as an application error naming the wrong component.
  Dependency caching is on, and the timeouts are ceilings sized for the smaller
  two-core hosted machines.

### Fixed - the holding tray, cross-pack drops and cold previews

- **Several held emoji can be picked at once.** The pick box lives on a grid
  card and a held emoji has none, so the tray was the one place selection mode
  could not reach and held emoji moved one Unhold at a time. Held cards now
  carry the same pick box: click plus shift-click takes a run, dragging one
  picked card carries every picked held emoji, and Unhold on a picked card
  returns the whole set.
- **An emoji dropped into another pack now joins it, and Save writes the plan.**
  The panel states the intended layout rather than mirroring Telegram: drag an
  emoji into the pack it should end up in, and it is re-stamped into that pack
  instead of being silently re-grouped by the pack it came from — which is what
  made the drag look broken. Nothing moves on Telegram at that moment and
  nothing can (the Bot API has no move-between-sets call), so **Save now writes
  `<data-dir>/pack_plan.json`**: every emoji that changed pack with its `from`
  and `to`, every emoji parked in the tray with the pack it came out of, the
  resulting per-pack counts, and any pack over capacity. The real
  rearrangement is a separate deliberate step read from that file. Packs stay
  fixed 200-emoji buckets: a full one refuses the drop and names the gesture
  that makes room — park one of its emoji in the tray, then bring the
  replacement in. Capacity is now asked of the destination pack; it used to be
  asked of the emoji's own, which refused moves out of a full pack and allowed
  moves into one.
- **Previews are warmed in the background and render wider.** One animation
  costs ~155 ms and one video poster ~613 ms, and the render bound was a
  hard-coded 2, so a cold tier arrived in visible chunks while the owner
  scrolled. The bound is resource-aware now (24 cold animations: 2.92 s to
  1.55 s on 16 cores) and a daemon thread renders the page's previews in grid
  order ahead of the scroll.

### Fixed - curation and coordinated identity recovery

- Held cards leave the grid and pack counts; Unhold/Unhold all restore positions
  with capacity checks. Shift range picking, complete Undo/Redo and a last-Save
  Reset checkpoint cover the curation workflow.
- Newer saves survive older permanent refusals. UI action/error logging is
  bounded, and conflict warnings offer a local draft export.
- Compact previews reduce size/frame rate, covered animations stop, and inactive
  video players release their decoders. Rendering work is limited per server.
- Video merges and recovery verify native timelines, including unmatched tails;
  colliding sampled keys preserve distinct files and identities.
- Migration excludes permanent writers, replays evidenced per-file operations,
  refuses unrelated destinations, preserves an application rollback bundle and
  requires complete final invariants before success.
- Nine support modules moved into the shared package; public CLI paths stay
  stable. CI covers the new browser/native regressions.

### Added - a holding area for parked emoji

- **A holding area in the curate panel.** Drag an emoji onto it to exclude it
  from the next publish without hunting for its tick in a long grid; drag a
  parked one back into the grid to re-include it at the exact position
  dropped. Reuses the existing `included` flag — no new state, no backend
  change — rendered in its own strip inside the sticky header instead of
  wherever the card's position happens to be.
- **Zoom now takes an exact percentage.** `zoomReset` is a typeable field:
  type a value and press Enter to apply it, double-click still resets to
  100 %.

### Fixed - selection mode looked broken, and a pack's own count lied

- **Select all / Deselect all / Invert only ever touched publish inclusion.**
  While selection mode was on (picking several emoji to move together), these
  three buttons had no visible effect on the picks at all — nothing on
  screen responded, which read as the mode being broken outright. They now
  drive the picks while selection mode is on, publish inclusion otherwise.
- **The pick checkbox was easy to miss.** Its resting-state border and glyph
  used the same near-invisible colour as the card's own border; it now uses
  the same contrast the publish tick already has.
- **A pack's own upper bound could overcount by exactly what was excluded
  from its tail.** It was computed from the raw size of the underlying array,
  which counts an excluded item sitting past the pack's real end as if it
  still occupied a slot. It is now the last actually-included item, wherever
  it sits — a pack that is not the last one is unaffected, since it correctly
  keeps drawing on whatever candidates follow to stay full.

### Fixed - the launcher's yes/no prompts

- **A "yes" answer at four launcher prompts silently acted as "go back".**
  `Ask-YesNo` returns a bare `[bool]` for a plain yes/no or the string `'back'`
  for an explicit `0`; comparing a `[bool]` to `'back'` with `-eq` coerces the
  string operand to match the bool side (`'back'` becomes `$true`), so
  `$true -eq 'back'` is `True`. Every "yes" — including the default on a bare
  Enter — read as "back". B4 (open the curate panel) was the visible case: an
  answer of "yes" to "show the published packs too?" silently returned to the
  menu with no error, and "no" fell through correctly but then opened the
  panel without `--with-pack`, showing 1 catalog emoji instead of every pack.
  The same anti-pattern also affected A1 and A3's "run it now?" confirmation
  and B3's "dry-run then upload now?" — a "yes" there stepped the wizard back
  one screen instead of proceeding. Fixed at all four sites by checking the
  return type instead of relying on implicit string/bool coercion.

### Fixed - a second audit: identity, migration and the panel's queues

- **Video identity now fails closed.** A missing `libvpx-vp9`, a failed codec
  probe or a container naming no codec used to answer "no decoder needed",
  which drops VP9's separate alpha layer — and a probe failure was CACHED, so
  one transient error made every later decode of that file lose its alpha
  silently. Two clips differing only in opacity then share one content key, and
  `Catalog.add` merges them and deletes the file it merged away. All three paths
  now raise `UndecodableVideo`, and nothing undecidable reaches the catalog.
- **Comparing two videos walks the timeline.** `same_image(..., "video")` read
  frame zero and nothing else: two clips sharing ten opening frames compared
  equal. It now compares every sampled frame at 30 fps with no averaging, and no
  longer accepts content-key equality as proof for video — the identity stream
  is sampled at 10 fps, so a change shorter than one interval falls between its
  frames.
- **A content-key migration is one versioned change** (`collection_migrate.py`).
  The previous round moved three tables and stopped, leaving the publisher's
  state file naming keys the catalog no longer had — which `reconcile_set` reads
  as a reordered pack — plus stale derived hashes and archived filenames still
  carrying the old key. Every durable reference now moves together, under the
  pack-family lock, behind a verified snapshot taken with SQLite's online backup
  API (`copy2` of a WAL database opens as `no such table: items`). Stages are
  idempotent and journalled, so an interrupted run resumes and a second run is a
  verified no-op. `--from-backup` repairs a migration that ran before journals
  existed, pairing rows by content rather than position.
- **A failed inspection is not a success.** A row whose media was missing was
  counted as "unchanged", so a catalog nobody could read printed "Every video
  key already matches" and exited 0. Outcomes are counted apart and exit 4 means
  incomplete — distinct from clean (0) and from pending (3).
- **Unique attribution requires excluding every rival.** With two candidates
  matching one live sticker the resolver correctly reported ambiguity; deleting
  one candidate's FILE made it answer "unique" with the survivor.
- **The panel tells unsaved work from an in-flight request.** An acknowledgement
  of an older snapshot used to clear the dirty state, show "Saved" and drop the
  navigation guard while newer ticks were unsaved. It now tracks what the server
  has acknowledged, marks the Save button, and guards the tab until the visible
  selection is stored.
- **A refusal that retrying cannot fix stops being resubmitted.** HTTP 400/409
  ended the retry loop its own comment said could not help; the five-second
  heartbeat now honours the backoff instead of bypassing it; and a request that
  never resolves is released by a raced deadline rather than claiming the queue
  for the life of the page.
- **The Worker cannot leak a bot token through an error.** Every Bot API URL
  embeds it and a transport failure names the URL, which reached the HTTP 502
  body and every log sink. One redactor now covers the URL shape, the bare token
  shape and the configured secrets, on every path out.

### Fixed - identity, locking and validation defects found by an external audit

Fifteen numbered findings and eight adjacent-path items, each reproduced against
real artifacts before it was fixed. The ones that could corrupt data:

- **Two publishers could hold one pack-family lock.** Ownership was decided by
  reading a file, comparing a token and unlinking it — and an unlinked inode is
  a lock nobody else can see, so the stale-lock recovery path could seat two
  processes at once. Measured: two real processes inside one lock for 1.538 s.
  The lock is now an OS-level byte-range lock (`msvcrt.locking` on Windows,
  `flock` elsewhere) held for the whole critical section, and the file is never
  unlinked. The recorded holder is still written, but only as a hint for the
  "who has it" message.
- **Recovery could give a stranger's picture our identity.** The nearest
  candidate by perceptual hash was accepted outright, and dHash is a *grayscale
  structure* hash: an opaque red square and an opaque blue square are zero
  apart. A perceptual match may now only nominate; `same_image` decides, more
  than one verified match is `AMBIGUOUS`, and a candidate that could not be
  examined is `UNDECIDABLE`. Both refuse instead of guessing.
- **Video identity discarded alpha, and the two fingerprint APIs disagreed.**
  ffmpeg's default `vp9` decoder drops the separate alpha layer in silence, so
  two clips identical but for opacity shared one content key; and `fingerprint`
  and `perceptual_hash` sampled differently, so one 30 fps clip had two hashes.
  The decoder is now chosen from the **probed codec** (never the extension) in
  one new module, `emojikit/video_decode.py`, which both callers share — with a
  bounded frame cache that also cut the identity suite from 163 s to 48 s.
- **Fitting an image applied its alpha twice.** `fit_100` pasted the image using
  itself as the mask, compositing against the transparent canvas underneath:
  `(255, 0, 0, 128)` came out `(128, 0, 0, 64)`.
- **Preflight reported acceptances it never obtained.** The counter counted
  attempts, so a run in which every `check_uploadable` failed at the transport
  printed "all accepted" and exited 0. Accepted, refused, missing and
  *not checked* are now counted apart (`collection_preflight.py`). A refusal or
  a missing file exits 1; an unreachable Telegram exits 3, whether it happened
  to one file or to all of them — exit 1 is what "Telegram said no" means, and
  answering a dropped connection with it sends someone editing artwork that was
  never the problem.
- **A stale panel tab spoke for the whole catalog.** `/api/save` carries the
  full selection, so a page opened before another change re-included rows it had
  never seen — and answered `{"ok": true}`. The request now states the scope it
  was showing; a page too old to say gets 409 and one reload.
- **A Lottie timeline that was not a number passed validation**, because every
  comparison against NaN is false. Repainting also silently skipped a keyframed
  colour, and overwrote a gradient's opacity ramp with colour when the stop
  count was inferred as `len // 4` instead of read from `g.p`.
- **A set that closed mid-run was still appended to** on the next item, and the
  blank-media check received the family's format rather than the item's, so
  `--mixed` bypassed it.
- **The Worker accepted an unvalidated publish body**, and a note over the
  chunking limit escaped it.

Existing catalogs are **not** migrated implicitly: the corrected video decoder
changes what a content key IS, so `scripts/identity_repair.py` reports the
affected rows and moves them only with `--apply`, refusing outright on a
collision or an undecodable file. Ten new test modules cover the above, plus
`tests/test_panel_browser.py`, which drives the real panel in headless Chromium
(`requirements-dev.txt`). `panel.py` was over the 800-line ceiling after the
save-scope fix; its view model moved to `panel_view.py`.

### Changed - the curate panel is a virtual grid, and it zooms

The panel built every card up front. With six published packs open that was
1 068 cards, and every drag step re-laid-out all of them (14-17 ms per pointer
move, measured), while 56 `<video>` elements each held a media player from the
moment the page loaded. Now only the rows within half a screen of the viewport
exist; two spacers carry the height of the rest, every card is one fixed
height, and every row offset is integer arithmetic, so a render is a binary
search plus a hundred node moves whatever the catalog holds. Drag edits the
order as you drag and re-projects the grid — the drop records, a cancel
restores — and a video card gets its source only while it is near the viewport.
Measured in headless Chromium on that catalog: drag frames 24-46 ms → 16-24 ms
with none over two vsyncs, cold-scroll long tasks 802 ms → 113 ms, 11 743 DOM
nodes → 714.

**Zoom**: `−` / `100%` / `+` in the header, Ctrl+wheel, Ctrl+plus/minus/0.
Out fits more emoji per screen (below 75 % the text under each thumbnail is
dropped so rows pack tighter); in enlarges one. The level is remembered and
the emoji at the top of the screen stays there across a change.

The behaviour moved out of `assets/panel.html` into `assets/panel-grid.js` and
`assets/panel-actions.js`, served from `/static/` under a content-hash `?v=`
so an edited script is never served stale from the immutable cache.
`scripts/panel_sandbox.py` now forwards `--all` / `--with-pack N` to the panel
and clones the publish state, so a sandbox can show published packs too.

### Changed - the roster page keeps the panel's view-only controls, and gates its media

Stripping "everything that mutates" from the roster page took the header
controls with it. Top, Bottom, the backdrop cycle and the animation switch
change no data - they are how you SEE a grid of a thousand emoji, and the
backdrop in particular is what makes a black or a white emoji visible at all.
They are back, in the panel's own markup. Everything that writes is still
absent: no drag, no tick, no selection, no save, no form field.

The page also gates its media the way the panel does, which is the real answer
to "heavy": every animated card now inlines BOTH a still and the animation,
starts frozen, and only what is on screen is swapped to the moving version.
Scrolling freezes everything until 180 ms after it settles, a hidden tab
freezes everything, and `Animation: Off` freezes everything permanently. Before
this the page decoded 156 animations at once on load.

Thumbnails also dropped from 96px/12fps to 88px/9fps: a roster is for telling
emoji apart, not for admiring the motion, and fps is the single biggest lever on
the inlined weight. The largest page went from 10.3 MB to 7.4 MB.

**A misdiagnosis worth recording.** The gating measured as never starting an
animation, and the first explanation - that `content-visibility: auto` stops a
child from producing IntersectionObserver records - was wrong. The test browser
reported `visibilityState: "hidden"` with `innerHeight: 0`, where nothing
intersects anything and no animation *should* start. The observer now watches
the card rather than the image, which is defensible on its own terms, but the
freeze half is the only half that environment can prove.

### Changed - the roster page is the curate panel, minus everything that mutates

`packs/<set>.html` now uses the panel's own visual language: the same grid, card,
per-format accent (static cyan, animated violet, video green, logo amber),
position pill, format badge and checkerboard thumbnail with its format outline.
The two read as one product because they show the same objects.

What it deliberately does NOT carry is every control that changes something -
no `draggable`, no tick, no selection state, no save, no form field at all. It
is a record, and a control that looks live but saves nothing is worse than no
control. A test asserts that on the rendered markup rather than on the source,
because the stylesheet comment names the very attributes the page leaves out.

Each card now shows BOTH ids, labelled and individually click-to-copy: **this
pack** and, where the emoji came from someone else's, **original pack**. The
second one is what an external map may still be pointing at, so it is as
load-bearing as ours and gets the same affordance.
