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
- `static-convert.json`: the same four inputs through pinned
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

Original executable sources remain in Git history. Fixture regeneration must
retrieve and verify that exact source and runtime before executing a hermetic,
bounded source oracle. Never regenerate expected values from the target to
make a failing parity test pass. Complete native generation/drift tooling and
full source-case mapping remain migration requirements before source retirement.
