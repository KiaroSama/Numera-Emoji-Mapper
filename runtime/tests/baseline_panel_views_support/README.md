# Panel model and preview fixtures

All runtime test code is Rust. `fixtures.json` records the original source and
fixture SHA-256 digests.

`similarity-inputs.json` captures the original `SimilarityOrder._items` models
for seeds 1, 1234, and 99999: Python's 150 static 64-bit hashes, six repeated
hashes, 60 byte-sized video hashes, four unhashed animated items, and the
original shuffle. Each input has its original post-shuffle identity. Expected
identities come from the original independent greedy reference walk, including
the first-minimum tie break. Production Rust does not generate these expected
orders.

`preview-motion.tgs` is the exact `LOTTIE` object from
`tests/test_panel_preview.py`, JSON-encoded and gzip-compressed with mtime zero.
It has a red 80-by-80 solid moving across a 512-by-512 canvas at 30 fps.

`preview-poster.webm` uses the original 100-by-100 RGBA image
(240, 20, 40, 128) looped for 0.1 seconds. `preview-clip.webm` uses ten original
100-by-100 RGBA frames `(25*i, 200-15*i, 90, 128)` at 10 fps. Both use the
original `libvpx-vp9`, `yuva420p`, one-thread, `auto-alt-ref=0` recipes.
The real Rust codec decodes and renders these files in the tests. Python,
Pillow, and an external fixture encoder are unnecessary at test time.

Script inventory comes from production `asset_version::PANEL_SCRIPTS`;
expected asset versions are independently computed SHA-1 digests of actual
script/icon bytes.
