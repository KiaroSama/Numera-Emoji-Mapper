# Pinned backend source oracles

These twenty Python files are **byte-exact snapshots of `d173116`**, not production
modules and not hand-written expected implementations. Never edit a snapshot to
make a Rust mismatch disappear. `tests/backend_source_oracle.py` verifies SHA-256
for every snapshot before compiling any selected AST definitions. Intent, resume,
steps and view fixture expectations come exclusively from those definitions.

## Reproduce fixtures

From the repository root, with the project interpreter:

```powershell
.venv\Scripts\python.exe -m tests.backend_oracle          # check all four fixtures
.venv\Scripts\python.exe -m tests.backend_oracle --write  # explicit regeneration
```

The check is read-only and fails on source/fixture drift. Regeneration does not
change snapshots or hashes. Review input changes and generated fixture diffs;
never edit `expected` or `error` by hand. These commands execute source slices:
run them only under the project's bounded verification policy. They were not run
by the generator-authoring agent; final verification belongs to the integrator.

## Source identity

| Snapshot | SHA-256 |
|---|---|
| collection_state.py | f8c6e96a1ea7c2fa34484ab5bd7c5cdcda4b7099d86f793acd4464e7284faf99 |
| packstate.py | 74a52b8a334a2d78f49d8cd26c9e60b566620f0ef398e31694433b6d461b7ac3 |
| panel_plan.py | 4c6b43a35ea5fa5907cc1933524b1a5c79214d12bb13cccc269e34d7ae38b1e4 |
| panel_view.py | 8935c34372dea19ec47d586369344390eecd44f0fc9b65a24f9581f13be5cd27 |
| plan_apply.py | 73da0b3c0e3a606f7bcdce49fcf1abddb4bc3c18fc6f59faa9344b739ccdefa3 |
| telegram_api.py | 59f5c958a2f7a2f7ed32b93669861066bf3a3f6a6a859623f7456405b5c56aa3 |
| ingest.py | d2e7740602bd6993e51ab6fd0c0244764ca71e47b9dbafa2c6bdd5895050d0b6 |
| collection_reconcile.py | d22315d90e504d7acd622c02111eb9f2b9b3f33889fd483aa068cfe2b71d3dac |
| emoji_bot.py | e7ac5c65647d1e3e7b74e7fa8f7b027814439733aef1e1d0baab5033f53b175c |
| pack_manifest.py | f335b1e8ce4f508a40c20d13cffca0cb6c8506fbeb9b0363b53acd094d86ec13 |
| pack_gallery.py | 3a1460e846edb4bb75675f6d4d78d9e7d05ab75c7c61ec287d810a57cf5c9158 |
| script_json.py | b9afcea858593c9e532ee3690038433e48eefc00910af603b6b1e8b90a3e56e4 |
| pack_rows.py | 1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121 |
| pack_archive.py | 312e32d30bd0b8cdd905b93654a28b28131ce32f768fefd26e9d12aecd7f1c20 |
| status.py | ee534afec67664ab7bea27499232df04fb0b9cbd80c97beb4259150dda7f3b7b |
| pack_export.py | 1af5b4c013806130786b94130f31e00c92b6c6f2924e46c0f67e109fa39ee6f2 |
| state_artifacts.py | 3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001 |
| collection_migrate.py | 4cb0211b690af6960a59e6c5ee851f64dc376aa611712fb938f6a87ed8ec8a68 |
| migration_bundle.py | 8b2648956047e95fe5fcc1e779e85041366de917f1290faeb7b1d6c0e6e040c8 |
| collection_preflight.py | 2821e30d219dbf4cb4df8ff08f97f697e6f89bf79cca1d5a2ae572000dc5d965 |

Hashes and byte equality against `git show d173116:emojikit/<file>` were checked
before generator edits. No Git command or external process is needed by the
fixture generators or drift tests. Snapshots stay under 800 lines individually.

## Slice and stub inventory

| Corpus | Exact source definitions | Explicit boundary bindings | Coverage ceiling |
|---|---|---|---|
| intent.json (8 cases) | panel_plan: PlanError, build_plan, target_map, overlay_targets, merge_plan | isolated datetime namespace fixed at 2026-10-07 UTC | pure build/merge/targets/overlay only; no read_plan, save route or filesystem writes |
| resume.json (8 cases) | collection_state: _validate_state | Path, StateError type, FMT_TAG static/video/animated, MIXED=mixed, PER_SET=200 | collection validator only; no load_json/load_state, static packstate validator or restart/network behavior |
| steps.json (6 cases) | plan_apply: Refusal, Steps, compute_steps; panel_plan: target_map | dataclass/field, MIXED=mixed, PER_SET=200 | pure action planning only; no actual deletion/upload/journal/retry or report rendering |
| view.json (3 cases) | panel_view: copy_id_for, build_view | Path, anchored premium-id regex, LOGO_KEY; read-only catalog projection; operator config returns no brand | seed_order=False and unbranded only; no greedy seed, brand filesystem/logo cards, pack-name discovery or HTTP |

The view stub records the exact called catalog methods per case. It has no
get_meta/set_order/set_meta fallback; an accidental seeded path fails instead of
pretending seeding was tested. Expected card/path/hidden outputs are generated
by source `build_view`, not reconstructed in the stub. The case's `branded`
flag or nonempty logo_path cannot silently be ignored by this generator.

Step results use `dataclasses.asdict` plus JSON-only normalization of tuple rows,
integer map keys and sorted moved_in sets; counted/pending are read from source
properties. No native module or expected-result fixture is consulted. Inputs are
fresh copies, so source default insertion/mutation cannot rewrite later cases.

Full source ASTs are parsed, but **only listed definitions and original future
imports** are compiled. Application imports/module-level startup/config/logging
are not executed. Missing/duplicate requested definitions and source hash drift
fail before execution. Existing intent fixtures retain their exact timestamp,
case names and metadata shape; resume defaults/errors still come from source.

## Durable drift and false-green checks

- `test_native_state.py` retains intent/resume source regeneration comparisons.
- `test_native_plan_steps.py` regenerates steps, compares native results and
  complete literal refusal messages, verifies source-hash tampering fails, and
  rejects empty/duplicate/unannotated/double-annotated/wrong-source corpora.
- `test_native_panel.py` regenerates view and all four corpora while production
  modules/native extension are blocked in sys.modules; comparisons must work
  without the backend under test providing its own oracle.
- All corpora require exact case counts (8/8/6/3), unique nonempty names and
  exactly one generated expected result or meaningful error. No parity skips.

The new checks are pure, bounded in-memory work plus six small source reads, no
sleep, network, child process or database. Heavy suites remain the integrator's
single final pass. This generator work does not imply source drift tests passed
until that pass actually executes them.

## Ingest decision extension

`python -m tests.backend_ingest_oracle` checks `ingest.json`; `--write` explicitly regenerates it.
Eight cases execute the pinned `catalog_identity` definition with explicit tri-state
comparison/collision evidence and the separately pinned pure Hamming oracle. They assert
callback order as well as identity/refusal results through `NativeCatalog.identify`.
The collector and this public binding use the same native asynchronous decision path.
No real decoding/storage/quarantine/media-deletion behavior is claimed by these inputs.

## Roster, bot and gallery extensions

- `tests.backend_bot_oracle`: six pure extraction/payload cases from pinned emoji_bot; no polling/runtime proof.
- `tests.backend_reconcile_oracle`: eight nomination/tri-state callback cases; no decoded-media proof.
- `tests.backend_roster_oracle`: four exact `_row` identity/history cases.
- `tests.backend_roster_pack_oracle`: four source `build_pack`/`render_markdown` cases, plus `_keep_other_family` and `render_index_markdown` index case. Prefetched Sticker objects, fixed source UTC clock, actual shared `pack_rows` renderer; no native output used as expected data. Family merge uses one explicit read-only old-index binding and known family input keys.
- `tests.backend_gallery_oracle`: two full source HTML cases with explicit codec-result tuples, plus one actual public PNG thumbnail/full HTML reference through pinned `_thumb_file`, `_still_file`, `_uri`, `thumb_uri`, `render`, and safe-script JSON. Incumbent CSS/JS literals are parsed from pinned source; no backend imports. The public PNG codec case is static-only, not video/Lottie decoding coverage.

Each generator supports read-only checking and explicit `--write` regeneration, through the bounded runner. Native `gallery_parity` and `roster_pack_parity` integration tests compare exact outputs. `test_native_roster_cli` exercises detached-root loopback refresh/history/family stale state and refuses partial live reads before rewriting files. Full native cutover still depends on the complete feature contract ledger.

## Archive extension

`tests.backend_archive_oracle` supplies source `archive_name`, `_folder_name`, `_rows`, `render_history_md`, `render_manifest_md` with actual shared Markdown definitions and explicit catalog/live evidence. Names, logo/known/unidentified rows and complete metadata text are generated, not hand-translated. `archive_parity` compares them. `test_native_archive_cli`, `test_native_export_cli` and `test_native_status_cli` exercise actual detached-root commands, published paths, per-file failure recovery, ZIP entry/CRC/bytes/order, no partial replacement and read-only pending status. These selected checks do not cover every crash, filesystem or live contract; final integration still owns complete acceptance.

## Identity reference extension

`tests.backend_state_artifacts_oracle` generates three cases from the actual pinned `state_artifacts.remap_plan` and `collection_migrate._remap`. The contracts differ deliberately: panel plans rewrite only structured keys, while publisher documents recursively replace every matching string value (including a title equal to a key). Do not substitute a guessed common policy. `native/tests/state_artifacts.rs` compares each full document; `ownership.rs` and `sqlite_snapshot.rs` separately test canonical nesting/exclusion and a real uncheckpointed WAL backup. These are foundational seams, not evidence that complete migration/restore is implemented.

## Version2 SQL and replay extensions

`tests.backend_migration_database_oracle` generates two transactional reference/hash cases from pinned `migration_bundle.rewrite_database`. Swaps preserve distinct rows and their CID/FUID/labels; null and signed high-bit perceptual hashes retain source storage. `native/tests/migration_database.rs` compares full source-generated projections.

`tests.backend_signature_oracle` generates two original logical SQL signatures and exact Python repr row order from pinned `database_signature`. Cases include duplicate rows, quoted/control/Unicode text, BLOB hex, signed 64-bit boundaries, table ordering and exact path normalization. The native printability table is generated only on CPython3.11/UCD14.0.0; actual REAL/nonfinite signature inputs deliberately refuse before writes until compatible formatting is verified. Other producer Unicode versions are not certified. `test_native_roster.py` checks regeneration of both corpora; native tests consume independent expected results.

`test_native_migration_cli` runs actual native commands on a detached root. A generated real-format three-frame VP9 file requires FFmpeg, not private input, and covers version2 apply/source verification, native/source restoration, immutable-ID recovery, source-origin and native-origin interrupted-stage replay, later-edit refusal, and different-active-bundle refusal. The replay result is compared with the retained source result. Crash states are constructed deterministically; this is not forced process-death coverage. The optional private real-video copy remains a separate local-only scenario, not a substitute for required CI coverage. Existing production Python remains available until the complete contract ledger passes.

## Collection preflight report

Pinned `collection_preflight._report` supplies exact accepted/refused/missing/empty report and exit expectations to `test_native_publish_cli`. The actual native command uses loopback getMe plus uploadStickerFile only for the existing-file cases; no set mutation occurs and no frozen plan/publisher state is saved. Reports preserve separate accepted/refused/missing/unknown outcomes and never call an empty queue validated. Transport-unknown wire and retry pacing remain separate required contracts.

## Known missing parity cases

Brand-logo cards, seed-once greedy order, formats/unhashed grouping, bad view
inputs and HTTP/cache/session/security are not covered by the three view cases.
Steps lack per-format refusal, absent/excluded catalog targets, skipped files,
empty family and explicit logo-slot overrides. Resume lacks full load/state/IO,
static builder state and replay. Intent lacks filesystem read/write behavior.
Those are feature013 contract tasks, not reasons to weaken these existing cases
or to claim full backend migration/cutover. New inputs must regenerate their
outputs through pinned source, with an explicit stub/dependency inventory.

## Research and verification record

Authoring refreshed official Python unittest source/docs for exception matching
and dictionary patch restoration. `assertRaisesRegex` searches a regex: literal
source messages therefore use `re.escape` plus `\A`/`\Z` anchors. Source docs:
https://docs.python.org/3.11/library/unittest.html#unittest.TestCase.assertRaisesRegex
and pinned implementation:
https://github.com/python/cpython/blob/v3.11.14/Lib/unittest/case.py.
Public usage query `"assertRaisesRegex" "re.escape" repo:python/cpython` located
stdlib regression tests using the same literal-escaping pattern. No dependency
or third-party code copied. `unittest.mock.patch.dict` restores patched mappings
on exit; the existing project's unittest guard remains authoritative. Matched
usage was read in context at:
https://github.com/python/cpython/blob/6c25c98e08c250ed063e051b99135b8080d2d0d5/Lib/test/test_structseq.py.
Checked UTC 2026-10-07 during this request. Static compilation and project-rule
`ruff check` passed for the five changed Python files; no generator, parity test,
application or heavy suite was executed by this agent. Existing fixture JSON and
all pinned snapshot bytes were left untouched.
