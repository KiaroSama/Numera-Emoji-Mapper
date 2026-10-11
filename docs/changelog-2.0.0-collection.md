### Added - the glyph, in both grids

Every card in the roster page AND in the curate panel now shows the glyph its
sticker carries, under the artwork. Telegram never displays it - a custom emoji
renders as its picture - so these two grids are the only place the label can be
checked against the art it claims to describe, which is exactly how 21 wrong
glyphs were found. A sticker with no glyph shows an em dash rather than a gap.

Verified while adding it, across every live pack: no row with a missing or
non-numeric id, no gap in any pack's slot sequence, and no id appearing in two
packs.

### Added - `packs/`, the roster of what is actually in every published pack

`pack_manifest.py --refresh` writes, per live set, `packs/<set>.json` (machine),
`packs/<set>.md` (human) and `packs/<set>.html` (a self-contained page showing
every emoji, animation included), plus `packs/index.json` and `packs/README.md`
over every live pack, general and coin alike.

Each row carries the emoji's `custom_emoji_id`, its position numbered from 0
(so the brand logo is emoji 0) alongside the 1-based slot Telegram shows, its
format and glyph, a name, and - for anything taken from someone else's pack -
the id it had THERE. `index.json` also carries two flat lookups, `by_current_id`
and `by_source_id`, so "which of ours replaced that one" is one dereference.

**The order and the ids come from Telegram, never from the state file.** A pack
can be reordered live, and replacing a sticker mints a NEW id; a roster rebuilt
from the recorded plan would drift silently, which is the exact failure this
file exists to prevent.

Two details that were wrong in the older `collection/manifests` and are right
here: the brand logo now has its id (it has no catalog row, so anything walking
the catalog cannot name it - the live set can), and the "emoji 0 is the logo"
note is suppressed for the coin family, whose bot is exempt from the logo.

`packs/` is git-ignored: it is derived from Telegram on demand, and one refresh
rewrites ~79 MB of pages and cached thumbnails.

### Added - `pack_manifest.py --check`, which says whether the roster is stale

A roster is only worth having if it is never stale. `--check` exits 3 when
`packs/` stops describing its inputs - a new download, a replaced or recoloured
sticker, a reorder, a coin remap - with four stat calls and a small JSON read,
no network. The staleness test lives in this ONE place, so anything that runs
it (your own automation included) cannot disagree about what stale means.

### Fixed - three things wrong with the curate grid

**Drag and drop moved the card somewhere you had not aimed.** The drop handler
did `splice(from,1)` and then `splice(to,0,…)` with a `to` measured BEFORE the
removal. Removing the card shifts everything after it down one, so dragging
DOWN landed it one slot past the tile you released on while dragging UP landed
it exactly there - one gesture, two behaviours - and nothing showed where it
would go until it was already there.

The card is now MOVED as you drag, so the grid is the proposal: it sits
translucent (`opacity:.35`, dashed outline) in the slot it would take, and the
drop only adopts the DOM order. Preview and result cannot disagree because
there is only one of them, and no index is computed at drop time. Which half of
a tile the pointer is on decides before-or-after, so the last slot of a row is
reachable. A cancelled drag puts the node back, because `ITEMS` never changed
and a grid showing an order it would not save is worse than no preview at all.

**The tick offered a hand for a drag.** `cursor` inherits and the card is
`cursor:grab` because it is also the drag handle, so the one control that reads
as a switch showed the wrong affordance. `.tick` is `cursor:pointer` now.

**Animation stayed heavy.** This grid holds 76 animated previews (median 36
frames each) and 15 videos. Two changes, both about not decoding what nobody is
looking at: the IntersectionObserver band drops from `120px` to `0px` (it was
animating roughly a row above and below the viewport), and everything holds
frame 0 while you scroll, thawing 180 ms after it settles. Scrolling is the one
moment the work is pure waste - the compositor is already busy and the frames
go past too fast to read. The still is the same immutable-cached URL, so
freezing and thawing costs no request.

### Fixed - the grid drew no boundary between two published packs

`--with-pack 2 --with-pack 5` returned both packs as one unbroken run of 192
cards. The splits were computed purely from capacity: count included items,
start a new pack every `PER_SET - 1`. That is right for the pack being BUILT,
where a flat candidate list really is sliced into equal packs. It cannot work
for packs that already exist: pack 2 holds 95 and pack 5 holds 96, neither is
199, so counting to capacity found no seam at all and `starts.length < 2`
returned before drawing anything.

An already-live emoji knows which pack it is in - `--with-pack` resolves that
through the publishers' own state files - so `packs_named()` now returns
`{name: index}` instead of just names, `build_view` tags each already-live card
with `pack`, and `renumber()` splits on a CHANGE OF PACK whenever any card
carries one. Capacity arithmetic still runs for a grid of pure candidates,
which is the case it was written for.

A candidate carries no pack, so a mixed grid groups them under "Not in a pack
yet" rather than claiming they belong to the last one. A dict is still a
container of names, so every membership test on `keep_sets` reads as before.

### Added - `--tint`, baking Telegram's repaint into the asset ourselves

`needs_repainting` cannot be set on an existing pack - the Bot API exposes it on
the `Sticker` object (read-only) and on `createNewStickerSet` (whole-set, at
creation) and nowhere else, it is not a field of `InputSticker`, and it is not
stored in the file. So the flag is out of reach. The *look* is not: a repainted
sticker is just its silhouette filled with one colour, and we can do that.

`fetch_emoji_ids.py --tint '#RRGGBB'` flattens every REPAINTABLE emoji in the
batch to that colour and leaves everything else alone. It answers
`--repaintable` on its own - once the repaint is baked there is nothing left to
warn about - and records `tint:#RRGGBB` on the item beside its `premium-id:`,
because a recoloured asset does not resemble its source and nothing else says
why.

`media.repaint_in_place()` does the work, next to `reencode_in_place()` and
under the same rule: it runs BEFORE the fingerprint, so the content key
describes what is on disk.

- **animated** goes through the Lottie tree, not the raster - flattening frames
  would throw the animation away. Solid fills, strokes and gradient stops are
  all rewritten; gradient OFFSETS are kept, so the ramp's shape survives.
- **static** fills through the original alpha. Anti-aliased edges survive, and
  so do cut-outs: the tick inside a verified badge is a HOLE, not a dark shape,
  which is why it still reads correctly against any background.
- **video** is refused rather than silently passed through, since a no-op here
  would publish untouched art under a name that claims otherwise.
- The .tgs size cap is re-checked afterwards; over it, the original is kept.

### Fixed - the repaintable warning claimed the art is black; measured, it often is not

`--repaintable` told the operator that a repaintable emoji's "stored art is
usually flat black" and "will look black" in one of our packs. That was
generalised from one sample, Telegram's built-in `TopicIcons`. Two ids from
`vector_icons_by_fStikBot` tripped the gate on a real ingest and were skipped on
that advice; rendering their Lottie showed a **blue** verified badge (mean RGB
62,162,222) and a **purple/pink** star (133,102,240). The flag does not mean the
art has no colour, it means the client OVERRIDES whatever colour is there - so
the source pack is precisely where you cannot see what you would be publishing.

The gate now says that, and says to look first. Nothing about its behaviour
changed: `ask` with no terminal still skips, because skipping is still the
reversible half.

Re-verified against the live Bot API while correcting this: `needs_repainting`
appears in exactly two places, the `Sticker` object (read-only) and
`createNewStickerSet` (whole-set, at creation, custom-emoji sets only). It is
**not** a field of `InputSticker`, so `addStickerToSet` cannot carry it, and no
setter exists. It is not a property of the file either - it cannot be injected
into an asset before upload.


### Fixed - dHash read RGB from underneath transparent pixels

`_dhash` called `img.convert("L")` on an RGBA image. **That conversion discards
alpha and reads the raw RGB**, and RGB beneath a fully transparent pixel is
undefined - every encoder rewrites it. The same trap owner rule 1 meets with
`exact=True`, and the one `_premultiplied` was written for; `_dhash` simply did
not use it.

Caught by a real upload, not by reasoning: a logo measured **10** dHash bits
from Telegram's re-encode of *itself* (tolerance 6), which failed the
upload-verification gate and stranded a sticker that had in fact landed. Its
alpha channel was byte-identical and its premultiplied colour delta was 1.23 -
deep inside the calibrated same-image range of 0.02..2.35. Premultiplied first,
the distance is **0**, and a genuinely different image still measures **23**.

- A fully opaque image is unaffected: premultiplying by 255 is the identity.
- 492 of 550 stored catalog hashes changed, because emoji art is nearly all
  transparent - those values had been describing encoder noise. They were
  self-consistent among files this pipeline produced, which is why near-duplicate
  search never looked broken: it just could not match one logo against another
  source's copy of the same picture.
- Three tests pin it: invisible pixels cannot move the hash, genuinely different
  art stays far apart, and an opaque image hashes exactly as before.


### Added - `sync_order.py --pack N`

Publishing **appends**; it cannot move a sticker that is already live. So after
arranging a pack in the curate panel and publishing into it, the live order
still has to be applied with `sync_order.py` - and without a filter that reorders
the WHOLE family. Arranging pack 2 and syncing found 43 moves there, but also 28
in pack 5 and 1 in pack 1, neither of which the owner had looked at.

`--pack N` (repeatable) limits the run. An unknown number is a usage error, not
a silent no-op that reports success having reordered nothing.

`setStickerPositionInSet` moves an existing sticker, so no `custom_emoji_id`
changes - verified after the live run: all 105 ids the bot inventories name
still resolve.


### Added — `--repaintable`, a gate on emoji Telegram paints itself

A repaintable emoji carries no colour of its own: the client paints it with the
text or accent colour, so the stored asset is typically flat black. Republished
into one of our sets — created without that flag, and the Bot API has no method
to add it afterwards — it arrives black. `5354899958329784877` from Telegram's
built-in `TopicIcons` went through exactly that and had to be replaced sticker
by sticker.

- `fetch_pack.py` and `fetch_emoji_ids.py` now take
  `--repaintable {ask,skip,keep}`, default `ask`. The check runs **before any
  download**, once per run, and names the ids.
- **The flag is on the Sticker, not the StickerSet.** `getStickerSet` on
  `TopicIcons` answers `None` at the set level while all 160 of its stickers
  answer `True`, so a gate that read the set would pass every one through.
- With no one to answer, the default is **skip**, not proceed: a skipped emoji
  is one re-run away with `--repaintable keep`, while one already published has
  to be replaced in a live set.
- **`isatty()` alone was not enough, and only a real run showed it.** Under Git
  Bash, `fetch_emoji_ids.py … < /dev/null` still reports a tty; `input()` then
  raised `EOFError` and killed the whole ingest with an uncaught traceback. A
  question nobody answered is now a no. The fakes missed it because they inject
  the prompt — the live run is what caught it.


### Added — `--repaint`, so a monochrome mark can look right

- Telegram's own marks (TopicIcons and friends) are stored BLACK and painted by
  the client, because their set carries `needs_repainting`. Republished into an
  ordinary pack they simply look black. `--repaint` sets the flag when a new set
  is created.
- **Creation-only and whole-set, both enforced by the API not by us:** the field
  exists on `Sticker` and on `createNewStickerSet` and nowhere else, so an
  existing pack can never gain it, and turning it on flattens every full-colour
  emoji in the same set. Repaintable art needs its own family.
- Absent by default, not `"false"` — a test pins that, because no existing pack
  may start claiming a value it never had.

### Changed — the last two files over 800 lines, and the worker's toolchain

- `coins/fetch_paprika.py` 801 → 703, with the CoinPaprika HTTP/search half in
  `coins/_paprika_api.py`. Chosen because it is the one block with no stake in
  the state paths the tests redirect; `QUOTA_EXHAUSTED` is an identity sentinel,
  so importing the name keeps `is` comparisons working, and a test asserts it.
- `tests/test_resume_safety.py` 807 → 467, with the unknown-live-state cases in
  `tests/test_unresolved_mutation.py`.
- `wrangler` 4.125.0 → 4.127.1 and `@cloudflare/workers-types` → 5.20260831.1;
  the `^` ranges already allowed both, so the manifest floors were raised to the
  versions actually tested. Worker typecheck clean, 40/40 vitest.


### Removed — one blank-image rule instead of four, and a constant that was a trap

- **`emojikit.media.is_blank_image` is now the only definition.**
  `make_emoji_pngs._is_blank` and `coins/_dedup_plan.is_blank` each carried
  their own copy with their own re-declared `> 10` / `<= 8` — the thresholds
  media names `VISIBLE_ALPHA` and `BLANK_MAX_VISIBLE` — and the coins one was
  the slow per-pixel form media had already moved away from. Both delegate now;
  `_video_is_blank` reads the named constants instead of two more literals. The
  pipeline's central quality gate had four definitions that could drift apart.
- **`logsetup.LOG_RETENTION_DAYS` deleted.** Its own comment said nothing that
  acts on the retention window may read it, because `load_env()` has not run at
  import — a constant kept only so a test could assert on the parsing it did.
  `log_retention_days()` is the accessor. With no parse at import there is no
  import-time failure left, so `LogRetentionParsing` went with it; every value
  it checked is already covered by `TestRetentionIsReadWhenItIsUsed` against the
  call-time path, and the one guarantee it held alone — a garbage value must not
  break the module, not merely return the wrong number — is folded in there.
- **`make_emoji_pngs._pick_sources` deleted**: no production caller. It was
  `[g[0] for g in _source_groups(...)]`, and the two tests now read
  `_source_groups` directly.

Suite 760 → 756: exactly the four methods of the deleted class.


### Changed — six files over 800 lines split along real seams

No behaviour change: 760 tests before, 760 after. Every cut runs one way, so
none of the new modules imports the one it was cut from.

- `emojikit/media.py` 978 → 713, with identity in `emojikit/identity.py`.
  Making a file and *identifying* one are different jobs, and conversion never
  called a hash. **`identity` imports the media MODULE, not its names**: a bare
  `from ... import _run` binds once at import, so a test patching `media._run`
  to count ffmpeg launches would no longer be seen — and that count is exactly
  what `fingerprint()`'s one-pass guarantee is pinned by.
- `build_collection.py` 1495 → 782, layered under `collection_state.py` (plan,
  resume state, brand logo) and `collection_reconcile.py` (what is live in a
  set, and whose key each sticker is). The reconcile block turned out to be a
  clean leaf: it called nothing defined below it.
- `build_pack.py` 1692 → 696, with `telegram_api.py` (the Bot API client, its
  errors and Telegram's caps), `packstate.py` (state-file shape, atomic write,
  pack-family lock) and `announce.py`. The client depended on exactly one name
  from the engine, `api_base`, which went with it.
- `tests/test_build_collection_state.py` 1250 → 627, with the CLI half in
  `test_publish_cli.py`; `tests/test_coin_providers.py` 858 → 467, with the
  unverified-upload recovery path in `test_coin_recovery.py`.
- `tests/test_panel.py` 1392 → 641, plus `test_panel_page.py` (assertions on
  the served document), `test_panel_guard.py` and `test_panel_server.py`.
  `MutationGuard` travels whole: it owns nine `test_*` methods and is
  subclassed twice, so sharing it would inflate the count, not move it.

`coins/fetch_paprika.py` (801) and `tests/test_resume_safety.py` (807) were
left alone: one and seven lines over is the small overage the size policy
exempts, and cutting either would produce a fragment, not a seam.

Two traps worth recording, both caught by the suite:

- **`str.splitlines()` is not `split("
")`.** It also breaks on U+2028/U+2029
  — which `test_panel.py` embeds deliberately, because escaping them is what
  `InertItemJson` tests. Every line number past them was off by four.
- **A monkeypatch must name the namespace the CALLER reads.** Retargeting
  `patch.object(build_pack, "Telegram")` to `telegram_api` left the real client
  in place, and the suite spent five real retry sleeps dialling the discard
  port. `announce_via_worker` is the mirror case: its only caller is
  `announce.announce_packs`, so it belongs on `announce`.


### Added — `--into-pack N`, so any pack with room can be topped up

- **Publishing always appended to the newest set**, so once a later pack
  existed, the room left in an earlier one was unreachable — a pack stopped at
  21/200 could never be filled again. `--into-pack 1` aims the run at that pack.
- **It fails loudly instead of falling back**, because filling some *other*
  pack would still look like success: an unknown number, a full pack, or one
  holding a sticker this publisher cannot identify each stop the run. The last
  is not fussiness — appending past an unattributable sticker is what hands a
  new key someone else's `custom_emoji_id`.
- `--new-set` and `--into-pack` together are rejected as contradictory.
- **The real hazard was bookkeeping, not upload.** The live count and the
  recorded key order were written to `fmt_sets[-1]`, which equals the target
  only while filling the newest pack; aiming at a middle one would have
  credited the upload to the wrong record — state describing a set the sticker
  never entered, the exact drift `reconcile_set` exists to catch. Both now
  follow the set actually written to, pinned by a test that fails (`4 != 0`)
  when the old expression is restored.

### Changed — the curate page is an asset, not a string literal in `panel.py`

- **`panel.py` 1622 → 737 lines.** 902 of them were one HTML/CSS/JS string
  literal, so the module only ever held 720 lines of Python. The page now lives
  in `assets/panel.html` and is read at import. Not a module split: no boundary
  moved, no import changed, and `panel.PAGE` is byte-identical (42 156 chars,
  same SHA) — verified by comparing the loaded value before and against after.
- Ruff stops linting a JS blob as opaque, editors highlight it as HTML, and a
  UI change no longer churns the diff of a Python module.
- **The page is resolved `ROOT`-relative, never absolutely**, and
  `ThePanelPageActuallyShips` pins it: the asset exists inside the repo, the
  loaded page still opens `<!doctype html>` and closes `</html>`, and all five
  `__PLACEHOLDER__` tokens the handler substitutes survive. An absolute asset
  path is exactly how the "mandatory" brand logo once vanished on every machine
  but one with no test noticing, so a truncated or missing file now fails loudly
  rather than serving a blank page.

### Fixed — an upload check that called every one of its own uploads a stranger

- **`Telegram._sticker_matches` confirmed an upload by exact content key.** That
  key is a SHA of exact pixels and Telegram re-encodes what it stores, so the
  comparison could never hold for our own upload — and the caller reads a
  negative as "a foreign sticker landed". A 449-emoji publish stopped on a
  sticker that had landed correctly. The path is rare (it needs a network
  failure to trigger the probe), which is why it took a 449-item run to surface.
- **`media.same_image` now owns the decision for every caller.** Exact key
  first, then a dHash *and* a colour check, both of which must agree.
  `build_collection._same_image` keeps only the fetch and delegates: two answers
  to "is this the same picture" drifting apart is how one caller starts
  accusing what the other accepts.
- **Neither signal is safe alone**, which the suite proved before this shipped:
  dHash survives the re-encode but is grayscale, so a stranger's green square
  sits 4 bits from our red one and a structure-only check reported it as ours.
  Tolerances measured over 30 known-same and 30 known-different pairs across
  three live packs — dHash 0–3 vs 12–47 bits, mean channel delta 0.02–2.35 vs
  35.46–188.22.
- A false negative costs a halted publish and is recoverable; a false positive
  attributes a stranger's sticker to our item and is not. Anything that cannot
  be compared — animated is vector, so there is no raster hash — is reported as
  undecidable rather than negative, and callers reconcile instead of accusing.

### Added — `panel.py --with-pack N`, to arrange a half-full pack

- **Un-hides one published set** so its emoji can be arranged beside the new
  candidates going into it: `panel.py --with-pack 5` shows pack 5 and the
  unpublished items together, and nothing else. Repeatable.
- `--all` was the only way to do this and is the wrong tool: it also returns
  every finished pack, which on a grown catalog is hundreds of cards that
  cannot change.
- The index is resolved through the publisher's own `publish_*.json` rather
  than by rebuilding `<base><n>_by_<bot>`, because the state file already
  records the exact name. An index that matches no published pack is an error,
  not an empty grid — silence there is indistinguishable from "that pack is
  empty".
- Here, and only here, the lookup asks *which* set an item belongs to, so a
  publication row with no recorded `set_name` stays hidden: unknown-where is
  not answered with a guess. The plain filter still asks only *whether* a row
  exists.

### Added — the curate panel shows where each published pack starts and ends

- **Pack boundary markers.** When the selection spans more than one set, a
  full-width marker carrying the brand logo now sits at the head of each pack,
  labelled with the grid range it covers. The logo really is the first emoji of
  every set and costs one of the 200 slots, so this is where it will land.
- The splits are counted from **included** items only — an unticked card never
  ships and so cannot push the next emoji into the following pack — which is
  why selection changes now recompute them and not just the counter.
- A marker is a `.packsep`, deliberately not a `.card`: the drop handler
  resolves its target with `closest('.card')`, so a marker carrying that class
  would swallow a drop aimed past it and do nothing. Markers live only in the
  DOM — never in `ITEMS`, never saved — so reorder and save are unaffected.
- **↑ Top / ↓ Bottom** buttons in the header. They scroll the document rather
  than calling `scrollIntoView`, which aligns an element with the top of the
  viewport — behind the sticky header — and so stopped a header-height short of
  both ends.

### Fixed — the curate panel kept showing an abandoned pack's emoji

- **A published pack that is not full no longer appears in the grid.** The panel
  hid a pack only once it was *full*, on the theory that a set still being
  filled is still the pack being built. `--new-set` ended that theory: a pack
  can now be left half-empty deliberately, so "full" stopped meaning "finished"
  and a 21/200 pack's emoji kept coming back while curating the next one. The
  test is now simply whether the item is published — `is_published` skips it at
  publish time regardless, so showing it only invited pruning that changed
  nothing. `panel.py --all` still brings them back, which is how you reorder a
  live pack. Drops the `publish_*.json` read and the capacity arithmetic that
  went with the old rule.
- The check asks whether a publication row exists, **not** whether it records a
  set name: a row with a NULL `set_name` is still published, and reading the
  name would turn "I do not know where it went" into "never published" and
  offer a live emoji up to be republished.

### Added — `--new-set`, for starting the next pack before this one is full

- **`build_collection.py --new-set`** rolls the run into a fresh set and leaves
  the current one at whatever size it has. Until now set *N+1* opened only when
  set *N* reached `--per-set`, so a pack deliberately stopped early had no
  supported way forward. Both workarounds were wrong: a smaller `--per-set` caps
  every *later* set at the same wrong size, and a second `--base` starts a family
  whose publication table is empty, so the whole catalog re-uploads into it.
  The flag reuses the existing closed-set roll rather than adding a second way to
  open a set, and is read once per run — later items fill the new set normally.

### Fixed — a stopped publish locked its own pack family for six hours

- **A killed run could not be resumed.** The pack-family lock was only
  reclaimable once it was six hours old *and* its process was gone. The liveness
  check already refuses to touch a live holder at any age, so the six hours had
  nothing left to protect — it only stood between the owner and resuming a
  publish they had stopped themselves, with a lock file to delete by hand as the
  workaround. The grace is now two minutes, which is all that is needed to cover
  claiming being `O_CREAT|O_EXCL` followed by a separate write: in that window
  the record is empty and its pid reads as 0, i.e. "dead".

### Changed — the curate panel

- **Animated is light violet** (`#c4b5fd`). The violet it started as sat too
  close to the dark background for the thumbnail outline to read.
- **The sticky header no longer blurs its backdrop.** `backdrop-filter` re-reads
  and blurs everything behind the header on every scroll frame, across its full
  width; with 201 cards scrolling under it that was the largest per-frame cost
  left. The header background is opaque instead, which needs no blur to stay
  readable. Off-screen cards were already free — they pause their video, swap an
  animation for its first frame, and skip rendering entirely
  (`content-visibility`).

### Changed — dependency upgrades (four merged, one rejected)

- **TypeScript 5.9 -> 7.0.2** and **vitest 2.1 -> 4.1.11** in `worker/`. Both
  verified locally: `tsc --noEmit` clean and all 34 worker tests pass on the new
  pair. Vitest 4 drops the `basic` reporter, which the project never used —
  `npm test` and the CI job both run a plain `vitest run`.
- **`actions/setup-node` v4 -> v7.** Its three breaking changes do not touch
  this workflow: the action now runs on Node 24 (fine on `ubuntu-latest`),
  automatic caching is limited to npm (npm is what we use), and the dummy
  `NODE_AUTH_TOKEN` export is gone (no registry publish here, and no
  `registry-url`). `node-version: "22"` is unaffected.
- **`rlottie-python >=1.3 -> >=1.3.8`** — the pinned floor now matches the
  version the suite has been running against all along.
- **`numpy >=1.26 -> >=2.5.2` was NOT merged: that release does not exist.**
  The newest published numpy is 2.4.6, so the bump would have made
  `pip install -r requirements-coins.txt` fail outright
  (`No matching distribution found for numpy>=2.5.2`).

### Changed — "port already in use" now names the process holding it

- The advice was "press Ctrl+C in the window running it", which helps nobody
  when the holder was started detached and has no window — which is how every
  stray one so far got there. The message now carries the pid and the exact
  `taskkill` line. The lookup is best-effort and can never raise: it exists only
  to improve an error message.

### Fixed — the log eviction read the whole table on every write

- The 10 MB budget is enforced with a window function that sums every row. It
  ran on **every insert**. Measured on the live database: 77 rows today, so
  harmless — but the cost grows exactly as the log fills, which is its normal
  state. At the cap the table holds roughly 163 000 rows, and D1's free tier
  allows 5 000 000 row reads a day: about **thirty log lines**.
- It now runs once every 250 rows, keyed off the `last_row_id` the insert just
  returned. Not a counter in the isolate — a Worker isolate is short-lived, so
  that counter would reset before reaching its threshold and the eviction would
  never run at all. The budget stays exact when it runs; between runs the table
  may sit up to 250 rows over it, and one row is capped at 2000 characters.

### Changed — the curate panel moved to port 9450

- 8765 is a busy neighbourhood and a stray listener there made the launcher's
  panel step fail with a bare WinError 10048.
- **`panel_sandbox` now IMPORTS the panel's port** (`panel.DEFAULT_PORT`) and
  serves on it + 1, instead of both files spelling the number out. The sandbox
  exists to stay off the real panel's port; two copies of that number would have
  drifted the moment one moved — and pointing an automated drag at the REAL
  catalog is how three hours of manual ordering were lost once. A test pins it.

### Fixed — saving from the filtered grid could re-include hidden emoji

- `set_inclusion` re-includes every key it is not given, and the new filter
  meant a save posted from the grid carried only the VISIBLE keys — so any
  hidden emoji that had been deselected would have been silently re-included.
  The save now carries every hidden exclusion through. The panel already
  documented an earlier variant of this bug; the filter reintroduced it.
- The save toast says `in the catalog` when a filter is active: its counts come
  from the catalog, not the grid, and "213 included" under 15 visible cards is
  alarming until you know that.

### Changed — the curate panel shows the pack being built, not the finished ones

- **Emoji in a FINISHED pack are hidden from the grid** — finished meaning the
  set is full, so nothing can be added to it again. The header reports how many
  are hidden and `panel.py --all` brings them back; a filter nobody can see is
  indistinguishable from having lost the items.
- The first version of this test was "already published", which hid the 14
  emoji of a half-empty second pack along with the 200 of the finished first
  one and left an empty grid. Fullness comes from the publishers' state files,
  not from counting catalog rows: the brand logo takes a slot without being a
  row, so counting calls a full set one short. An item whose set cannot be
  resolved stays visible — unknown is not finished.
- Hidden, never deleted. Those rows are what dedup recognises a re-download by,
  what maps a source premium id to ours, and what `sync_order` reads to re-sort
  an already published set. `set_order` keeps unlisted items in their relative
  order, so arranging the new pack cannot scramble a published one.

### Changed — a tidier repository root

- `CONTRIBUTING.md` and `SECURITY.md` moved to `.github/`, where GitHub reads
  them just the same.

### Fixed — reordering a pack left it unpublishable

- **`sync_order.py` moved the live stickers but never rewrote the order the
  publisher recorded.** That record is checked position by position, by
  identity, before anything is added to a set, so a reorder made the whole
  family unpublishable: the next publish stopped on "position 117 now holds a
  sticker this publisher cannot identify". The record now follows the reorder.
- It is rewritten **even when nothing needs moving**, because a set can already
  be in the right order while the record of that order is stale — which is
  exactly the state an earlier reorder leaves behind, and the one that blocks
  publishing. A report-only run still writes nothing.

### Added — the bot answers in both directions

- **Send ids, get the emoji.** The bot already turned premium emoji into ids;
  it now takes ids as plain text and replies with the emoji beside each one, in
  one message. A single id, newline-separated, comma-separated and
  `, `-separated all parse, and a repeated id is answered once.
- The whole message must be ids and separators, so prose containing a long
  number — a chat id, a timestamp — is ignored rather than answered.
- Ids are resolved with `getCustomEmojiStickers` before rendering: a
  `<tg-emoji>` tag shows its placeholder for an id that does not exist, so an
  unchecked id would make a typo look exactly like a success. Unresolvable ids
  are named. Resolving also supplies each sticker's own emoji as the tag's
  fallback, replacing a fixed star wherever the tag cannot render
  (notification previews, copied-out text, older clients).
- Implemented in both the Worker (live) and `emoji_bot.py`, which the Worker's
  extractor is documented as a faithful port of.

### Fixed — the provider path wrote set records without their live count

- **`coins/fetch_paprika.py` appended set records with no `live` key**, while
  `rebuild_dedup.py` always writes one. Twelve of the 29 coin sets were recorded
  that way, so the state file on disk was wrong about them. Nothing crashed only
  because the rebuild refreshes every set from Telegram before it subscripts
  `state["sets"][-1]["live"]` — the two writers agreeing is what makes that safe
  rather than lucky. Both provider writers now record `live`.
- A test scans the set-record literals in the provider and fails if one omits
  the field.

### Fixed — the manifest reported the catalog's row count, not the pack's size

- **It said "199 emoji" for a 200-emoji pack.** The brand logo is the set's
  first sticker but not a catalog item, so the key list is one short of what is
  live. The manifest is the file someone opens to see what a pack contains, and
  it now counts the pack: the logo is row 1 and the catalog items follow from 2.
  A set published without a logo is unaffected.

### Added — a preflight that asks Telegram before anything is published

- **`build_collection.py --preflight`** offers every queued file to
  `uploadStickerFile`, which runs the same validator as `addStickerToSet` and
  touches no set. A file Telegram will refuse is now found in about a minute
  instead of at whatever point of the publish it happens to reach — one `.tgs`
  surfaced 46 minutes in, after 99 uploads and two flood waits. A refusal stops
  the run, so nothing is left half-published.
- Launcher **B3** runs it between the dry run and the upload, so the default
  path is covered without anyone having to remember the flag.
- A transport failure is reported as "could not check", never as a bad file:
  a dropped connection must not send someone editing artwork that was fine.
- The publisher, the dry run and the preflight now share one `pending_keys`
  filter, so all three answer "what is queued" the same way.
