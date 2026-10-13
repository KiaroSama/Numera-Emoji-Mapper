# First-party native migration fixtures

These expected values come from the pinned source revision
`1a36a968b006236bd0665a1da65c541938c06061`, not from the replacement Rust code.
They cover an initial migration slice, not the whole source test inventory.

- `video-fingerprint.json`: three pinned nonuniform 30 fps, 15 fps and genuine
  33 ms single-frame clips. Source dHash values prove the first frame belongs to
  the frozen 10 fps stream rather than native frame zero; the short clip records
  an actual successful-empty sample followed by native-frame retry.
- `gallery-codec.json`: five pinned gallery outputs: 88px static LANCZOS/lossy65,
  animated still/moving 88px/9fps/lossy65, raw video copy and source animation-info
  booleans. This is distinct from the panel preview's BICUBIC/lossless policy.
  Animated gallery output compares to the independently pinned renderer source
  on the same platform; committed Windows bytes are retained, not replaced.
- `video-operations.json`: actual pinned source VP9 conversion pixels/probe fields,
  lossless stream-copy preparation and WebP preview bytes. Container bytes are not
  deterministic across muxer runs; decoded frames and exact same-platform preview
  codec bytes are the observable contract. External-version differences still need
  independent source verification, never target-derived replacement expectations.
- `video-validation.json`: twelve source metadata refusal/order observations and
  four actual alpha-sample blank checks; metadata-only variants do not prove full
  real audio/container/oversized corpus acceptance.
- `video-timing.json`: eleven pinned byte-level timing grammar observations.
  Unrelated non-UTF-8 metadata does not invalidate ASCII frame records; malformed
  indices/field order, missing timestamps and nonfinite timing remain refusals.
- `video-timeline.json`: four pinned real clips and four source comparison pairs,
  including equal sampled lookup keys with a differing native frame. Native RGBA,
  presentation timestamps and duration are independent source observations;
  VFR boundary quantization and full consumer/cancellation coverage remain pending.
- `video-codec.json`: five pinned real VP8/VP9 byte fixtures over opaque,
  translucent, extensionless and single-frame inputs. Probe fields, alpha-safe
  decoder arguments, frozen 10 fps RGBA hashes and content keys are source
  observations. Encoded fixtures are synthetic artwork, not the final real-media
  acceptance corpus. Timeline comparison, decoder/cache failure recovery and
  conversion remain outside this initial seam.
- `video-probe-error.json`: pinned source refusal for an actual truncated WebM,
  with input/source hashes. Probe failure is not inferred from a raster decoder
  failure: FFprobe can recognize a damaged PNG container without decoding pixels.
- `media-format.json`: 22 pinned byte-header/extension detections and eight
  collision-key observations. Magic bytes take priority over extension hints;
  JSON without a hint stays unknown. This is not video decoding or local CLI
  target-format selection. Collision keys retain the final sampled lookup bucket.
- `static-codec.json`: Pillow 12.3.0; original
  `emojikit.identity.fingerprint` and `emojikit.media.is_blank_image` outputs.
  Input hashes protect the four PNGs. The public-logo input is an unchanged copy
  of the repository's public artwork; other inputs exercise opaque pixels,
  invisible RGB under transparency and alpha boundaries. Source revision/hash
  are recorded in the fixture. This does not cover SVG, Lottie or VP9.
- `static-convert.json`: the four raster inputs and a synthetic gradient SVG through pinned
  `emojikit.media.to_static_png`, with exact PNG-byte and decoded-RGBA hashes,
  size/mode and blank-image refusal. Output preservation is checked separately
  by the native test. These are Pillow 12.3.0 Windows source outputs, not target hashes.
  PNG compression bytes can differ across external platform codec builds even
  when decoded pixels are identical. During migration, the hosted checkpoint
  regenerates the pinned source result on its runner, verifies unchanged RGBA
  hashes/refusals and compares target bytes to that independent same-runner
  source oracle. It never replaces the committed expectation with target output.
- `provider-publish.json`: eight pinned source top-up traces at controlled
  live/hash/upload/proof/promotion/persistence seams. Includes full-set rollover,
  rejected/ambiguous/unknown mutation, verified absence, unknown readback and
  current-map skip. Durable write ordering and exact intent fields are source
  observations; this is not complete actual publisher or restart acceptance.
  Native effects additionally exercise one real loopback multipart upload, exact
  supplied media bytes, content readback, mapping/tally persistence and a second
  mapped-ticker skip. Full ambiguous mutation/process-death coverage stays open.
- `provider-proof.json` / `provider-recovery.json`: nine independent source
  content-proof traces and seven interrupted-upload recovery/order traces.
  External download/dHash/live state/promotion/persistence are controlled seams;
  these prove policy only, not actual Telegram, codecs, durable restart or
  complete original recovery-case migration. Recorded source identity is reused;
  unknown/duplicate/unreadable evidence refuses, maps save before promotion and
  provider tally is idempotent/separate from rebuild order. Additional native
  staging tests use existing actual image bytes, private directories, mismatched
  promotion preservation and retained pending sources. Initial durable-recovery
  integration reads live sticker/download bytes only from a bounded loopback
  server, persists mapping/tally and proves no second add/create request; it is
  not the complete source crash/restart matrix or live service acceptance.
- `keyword-writer.json`: four source byte-exact quoted UTF-8/CRLF CSV observations
  and four actual image-admission checks. Earlier names survive empty lookups;
  sorted rows and PNG decode/alpha-bbox admission differ intentionally from the
  provider-logo <=8-visible-pixel rule. Native atomic replacement additionally
  protects the previous CSV when malformed input fails.
- `gecko-names.json`: six source market-page/long-tail/duplicate-symbol/partial
  lookup traces with virtual page waits. No live CoinGecko requests; arbitrary
  primitive coercion and full public fetch/build command parity remain pending.
- `provider-image.json`: seven pinned source provider-logo observations: 0/8/9
  visible pixels, alpha 10/11/128, hidden RGB and corrupt bytes. Blank detection
  runs before fitting; accepted 100x100 PNG/RGBA hashes come from source. The
  hosted source oracle checks unchanged pixels/refusals and supplies same-platform
  PNG bytes. Exclusive native output preservation is an additional safety check;
  real provider downloads and public publishing acceptance remain separate.
- `cmc-logo.json`: eight source metadata/logo lookup traces, including missing,
  non-object payload, wrong ID, empty logo and 64-to-128 URL selection. Waits are
  virtual; actual status/authentication/JSON/logo bytes are tested on bounded
  numeric-loopback HTTP, never paid CMC calls. `cmc-map.json` adds six explicit
  ordered-query/source merge traces, unique full/base symbol sets, transient
  aggregation and stable duplicate-ID position. Python set iteration is unstable;
  this ordered seam does not certify source process-randomized query precedence
  or arbitrary mixed-type IDs. Public CLI/persistence parity remains pending.
- `paprika-search.json`: seven source two-query name/ticker fallback traces,
  preserving quota stop, partial-not-used and transient recovery with virtual
  metered waits. It does not contact the provider or certify a live quota.
- `paprika-resolve.json`: five source resolver traces with per-coin saved snapshots,
  quota stop, unknown-not-cached, cached misses and explicit retry-unmatched.
  Fake provider outcomes exercise policy, not live HTTP/publication acceptance.
  Native durable-cache tests replay those same traces on owned files, fresh
  readback, competing OS ownership, malformed-cache preservation and controlled
  search/save failure; required existing fixtures are reused, not target-generated.
- `inventory-parse.json`: twenty-four exact source block-parser observations,
  including Unicode/control whitespace, CRLF, unanchored headings, greedy header
  backtracking and cross-line ticker quirks. No real operator inventory is used.
- `coin-state.json`: twenty source resume-shape/cursor/order/in-flight refusal
  observations, including booleans rejected as counts and cursor/order mismatch.
  These pure invariants are not persisted restart or live publication acceptance.
- `coin-http.json`: nine independent source retry traces using virtual time:
  immediate success, quota stop, transient recovery, exhausted retries and
  Retry-After clamping/defaults. Actual native loopback transport is tested
  separately; redirect, provider CLI and durable cache parity remain pending.
- `provider-policy.json`: seven source candidate-precedence observations and
  fourteen Paprika/CMC status outcomes. Quota, transient, missing and rejected
  credentials stay distinct. Transport/backoff/cache/CLI integration is separate.
- `coin-plan.json`: four source-observed deterministic image-group plans and four
  current-map alias replays. Inventory representative preference, merged keyword
  order, duplicate ticker names and unapproved shared groups remain explicit.
  This pure string-ID seam does not cover provider/network/durable map workflows.
- `coin-aliases.json`: CPython 3.11.9, Unicode 14.0.0; pinned `coins/alias_map.py`
  AST `DROP`, `norm` and `main`, with in-memory map/CSV/missing-row I/O seams.
  Expected map order, messages, ambiguity and errors are source observations.
  The fixture covers 12 normalization, eight grouping and six error cases.
  Locks, atomic file writes, inventory parsing/refill and the CLI are not
  exercised by this pure-policy fixture.

- `static-reencode.json`: source lossless WEBP re-encoding with `exact=True`,
  including RGB under fully transparent pixels and idempotent repeat. Hashes
  cover source-encoded bytes and every decoded RGBA byte; only owned copies are
  modified by the test.
- `static-preview.json`: four pinned source observations for public artwork,
  alpha edges, invisible RGB and corrupt files. Byte/pixel hashes and aspect/no-
  enlargement behavior preserve the source's default BICUBIC thumbnail contract;
  gallery thumbnails use a different filter and remain outside this slice.
- `static-tint.json`: four pinned source observations over opaque, alpha-edge,
  invisible RGB and corrupt raster inputs. Exact lossless WEBP/RGBA/alpha hashes,
  repeat return values and corrupt-byte preservation are verified on owned copies.
- `static-compare.json`: pinned `emojikit.identity.same_image` results for
  equal/near/different colour, invisible RGB and corrupt inputs. Unknown remains
  null, not false. This static contract must not be applied to video timelines.
- `animated-key.json`: 11 source observations with verified source/runtime hashes:
  canonical ordering, duplicate keys, Unicode, nonfinite numbers, negative zero,
  large integers and corrupt JSON/UTF-8 raw-byte fallback. Non-object roots,
  expansion limits and truncated gzip errors propagate rather than becoming keys.
  Nonfinite JSON files intentionally exercise CPython's non-standard JSON support.
  No raster phash is invented for vector artwork.
- `lottie-file.json`: seven pinned real-file validation observations and six
  vector comparison pairs. Size and gzip magic precede decoding; truncated gzip
  is distinct from malformed JSON. The oversized malformed header is stored as
  UTF-8 Base64 and decoded into owned scratch with its original byte hash checked.
  Canonical equality is true, vector mismatch stays unknown, and only `compare`
  has the byte-identical shortcut.
- `tgs-prepare.json`: five pinned source file observations: Unicode re-gzip,
  colour tint, corrupt-input preservation and compressed size-cap refusals for
  both re-encode and tint. Repeat return values and output bytes are checked;
  only test-owned copies are rewritten. The larger synthetic JSON is required
  to exercise a genuine 64 KiB compressed limit, not a mocked encoder result.
- `tgs-package.json`: pinned source Unicode/deterministic gzip package bytes,
  wrong canvas and nested subtract-mask refusal. Native publication additionally
  preserves existing output and refuses to publish failed candidates.
- `tgs-load.json`: seven pinned source cases over actual gzip files and plain
  JSON. Includes corrupt gzip, malformed UTF-8/JSON, non-object roots and an
  8 MiB expansion refusal. `.tgs` inputs are binary gzip fixtures, including
  intentionally malformed binary data; they must never be transcoded as UTF-8.
  Their hashes and expected decoder errors verify this binary-format boundary.
  The malformed header is stored as UTF-8 Base64 (`corrupt.tgs.b64`) because
  repository text classification cannot recognize its truncated binary header;
  the test decodes it into owned scratch and verifies the original byte hash.
  No encoding exception or hook bypass is used.
- `coin-inventory.json`: source normalization, safe ticker aliases and text
  refill/count/newline behavior with string IDs. Header parsing, non-string ID
  formatting, file writes and locks remain outside this verified slice.
- `lottie-preview.json`: actual pinned source renders through rlottie-python 1.3.8
  and Pillow 12.3.0: still, moving circle and one-native-frame clips. Records
  WebP bytes, decoded frames and durations. These are synthetic artwork inputs,
  not full real-world animation coverage. CI regenerates an independent source
  oracle on its own platform and compares target bytes to that source; committed
  Windows observations remain unchanged. The short clip must retain its native
  rate rather than producing an empty sampled frame list.
- `lottie-validation.json`: pinned `validate_tgs` policy with already-decoded
  document and file stat/magic/load seams; 84 source cases cover timeline,
  numeric coercion, required fields and nested subtract-mask refusal. This is
  not gzip, JSON decoder, renderer or exception-class parity.
- `lottie-tint.json`: pinned `emojikit/repaint.py` pure source AST under
  CPython 3.11.9; tint parsing/errors, boolean numeric quirks, solid/keyframed
  colours and gradient opacity preservation. This does not cover animation
  rendering, gzip packaging, file replacement or runtime cancellation.

Original executable sources remain in Git history. Fixture regeneration must
retrieve and verify that exact source and runtime before executing a hermetic,
bounded source oracle. Never regenerate expected values from the target to
make a failing parity test pass. Complete native generation/drift tooling and
full source-case mapping remain migration requirements before source retirement.
