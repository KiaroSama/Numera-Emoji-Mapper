# Original pack tests in Rust

`case-map.json` maps each of the 95 original unittest identities to exactly one
named Rust test body. Both independently discovered original baselines contain
this same set; there are no inherited additions. The originals remain available
for review until migration acceptance.

The integration target contains 70 cases. Another 25 cases are private children
of the actual plan-apply, sync-order, roster CLI and archive rows modules. The
private cases call the same implementation as the public command, with concrete
loopback Telegram clients and the originally mocked document/fingerprint
collaborators. Test-only hooks and private module declarations are excluded from
production builds. Helpers have no `#[test]` attributes.

Fixtures own SQLite databases, publication state, media, logs, HTTP listeners and
processes under `logs/tmp/baseline-packs`. They never read operator credentials
or real Telegram endpoints. The after-delete case exits an actual test child at
the boundary between a successful wire deletion and durable settlement, then
reopens the real catalog and journal in the parent to verify recovery.

`fixtures/provenance.json` records the independent original input recipes and
SHA-256 digests. Family PNGs retain Python's original seeded random cells; the
WebM retains the original three-frame VP9 recipe. Rust tests consume frozen input
bytes and run the real native codecs; Python and ffmpeg are not fixture builders
at test runtime. Gallery assertions inspect embedded media, still frames and
unchanged cache timestamps after repeated rendering.

Run test executables through `numera-runtime-tests`, which enforces the owned
process, credential and network boundaries. Do not execute protected binaries
directly to work around a runner admission failure. Build and runner commands,
actual outcomes and any explicit native API adaptations are recorded in the
migration verification report.
