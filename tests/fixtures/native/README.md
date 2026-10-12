# First-party native migration fixtures

These expected values come from the pinned source revision
`1a36a968b006236bd0665a1da65c541938c06061`, not from the replacement Rust code.
They cover an initial migration slice, not the whole source test inventory.

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
- `static-compare.json`: pinned `emojikit.identity.same_image` results for
  equal/near/different colour, invisible RGB and corrupt inputs. Unknown remains
  null, not false. This static contract must not be applied to video timelines.
- `animated-key.json`: 11 source observations with verified source/runtime hashes:
  canonical ordering, duplicate keys, Unicode, nonfinite numbers, negative zero,
  large integers and corrupt JSON/UTF-8 raw-byte fallback. Non-object roots,
  expansion limits and truncated gzip errors propagate rather than becoming keys.
  Nonfinite JSON files intentionally exercise CPython's non-standard JSON support.
  No raster phash is invented for vector artwork.
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
