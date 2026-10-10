# Remaining application reference and consumer closure

Feature013 T051 source-only cohort, 2026-10-10. This extends the previously
reviewed catalog/migration/panel cohorts. These are adapted executable references,
not immutable oracles, production fallbacks or permission for production removal.
Existing README provenance and parent-owned references remain untouched.

## Original source identity

All seventeen originals below were read completely before copying and had no diff
against `d173116`. SHA-256 is for the original, not the adapted reference. Line
counts are original source counts (including a terminal line without newline).

| Reference / original `emojikit/<name>.py` | Lines | Original SHA-256 |
|---|---:|---|
| add_media | 171 | b709419724e29c176e8fd779da4c38d0f144bf1317f9774c8dc74f27b3b9d94b |
| fetch_pack | 260 | dfd50abec4ac45aef4cadb1b109307d492a5936db264a27d98d2fa3296bd62cc |
| fetch_emoji_ids | 310 | 965b8bbf1f0a8343eee5856fc2d988c56c62d6706fcb08b3a75ce68597ee5c42 |
| build_collection | 666 | f5215ab80ebecf2d31dd5443c802c532b47b5ef4fbe4460dbf4003f3eacbc494 |
| collection_names | 59 | 94599d3cb61316236c3577d1f967a0ad5778eab91d9dd9f59229e025dadd5f2a |
| collection_notify | 75 | 256185937b786d14fca62082a9e157d2afeb2c393836383c8760512e59e39007 |
| collection_media_check | 55 | 78743e72e413c0eb7f73e0f7a8d6306df46cb6e81e32b03b261c946115dcc209 |
| collection_preflight | 115 | 2821e30d219dbf4cb4df8ff08f97f697e6f89bf79cca1d5a2ae572000dc5d965 |
| collection_reconcile | 551 | d22315d90e504d7acd622c02111eb9f2b9b3f33889fd483aa068cfe2b71d3dac |
| plan_apply | 462 | 73da0b3c0e3a606f7bcdce49fcf1abddb4bc3c18fc6f59faa9344b739ccdefa3 |
| sync_order | 221 | 2770888d95f980a3a7d08e639c9d058948bd3f10055861cc6fee47788bc439e1 |
| status | 100 | ee534afec67664ab7bea27499232df04fb0b9cbd80c97beb4259150dda7f3b7b |
| pack_export | 100 | 1af5b4c013806130786b94130f31e00c92b6c6f2924e46c0f67e109fa39ee6f2 |
| emoji_bot | 561 | e7ac5c65647d1e3e7b74e7fa8f7b027814439733aef1e1d0baab5033f53b175c |
| pack_archive | 390 | 312e32d30bd0b8cdd905b93654a28b28131ce32f768fefd26e9d12aecd7f1c20 |
| pack_manifest | 561 | f335b1e8ce4f508a40c20d13cffca0cb6c8506fbeb9b0363b53acd094d86ec13 |
| pack_gallery | 400 | 3a1460e846edb4bb75675f6d4d78d9e7d05ab75c7c61ec287d810a57cf5c9158 |

## Exact adaptations and dependency closure

All retired application imports now name reference siblings, including lazy
imports. The existing parent-created build_pack, make_emoji_pngs, plan_status and
repaint_gate references were not changed; their dependencies now exist in this
package. No new forwarding wrapper, dispatcher, abstraction or test suite added.

- Catalog/ingest and collection_state imports are reference-local throughout
  collectors, publishers, reconciliation, preflight, naming and notifications.
  Publisher imports reference helpers; applier imports reference publisher,
  reorder, panel_plan and plan_status. Application-specific exceptions therefore
  have one reference identity across these consumers.
- Status/export import reference archive/manifest; manifest imports reference
  gallery; gallery imports reference script_json. Archive imports export lazily
  in its existing main branch, retaining the original mutual dependency without
  creating a new eager circular import.
- Every original `Path(__file__).resolve().parent.parent` coordinate becomes
  `parents[2]` at this deeper location. This includes emoji_bot.OFFSET_FILE;
  archive/manifest derivative data/roster/coin/thumb constants still point to the
  repository root. build_collection inherits ROOT from reference collection_state.
- Other source logic, comments, output/help strings, SQL, retries, guards, delays,
  codec behavior, payloads and durable-state contracts are unchanged. Re-exports
  from reference build_pack remain the same shared objects or reference gate.
- Actual retained runtime leaves remain under emojikit: media/media_lottie,
  repaint/video_decode, identity/errors, cli_env/logsetup, packstate/maintenance,
  media_paths/sqlite_snapshot/state_artifacts/sandbox_clone, announce/telegram_api,
  operator_config/pack_rows/similarity. No blanket package copy or replacement
  exception/lock/identity implementation is introduced.

Largest new reference is build_collection at666 lines. All copied responsibilities
remain below700, so no arbitrary source split is needed. Production originals and
`tests/oracles/` remain untouched. Ordinary package imports retain the test package
credential/network guard; no aliases, sys.modules registration, import hooks or
package-path manipulation are added.

## Existing consumer changes

This cohort changes only existing top-level test/helper files for retired
application imports and the corresponding single string patch target. Assertions,
real fixtures, scratch ownership, cleanup, timeout/skip behavior and runtime
leaf tests are preserved. Source catalog/ingest imports are explicit references.
Mixed imports split retained identity/media/pack_rows from reference applications.
The previously production-backed `_migration_process` publisher/archive/ingest/
panel writer probes now use reference applications too.

Existing source entrypoint discovery scans actual reference executable files,
keeps the minimum-count/expected-module/root assertions and still imports retained
runtime media_bridge plus unchanged coin tools. Launcher/CLI documentation checks
continue parsing original production launcher/docs/source declarations, but execute
reference module help rather than importing retired production implementations.
Source messages still describe the original public commands; those messages are
parity data, not an executable production fallback.

Consumer files changed by this final cohort (paths relative to tests/):

- _bc_fixtures.py, _migration_process.py, _panel_page_fixtures.py.
- test_brand_logo.py, test_build_collection_state.py, test_build_pack_announce.py,
  test_catalog.py, test_catalog_order.py, test_check_all_packs.py, test_cli_docs.py,
  test_coin_providers.py, test_coin_ticker_map.py, test_contracts_shared.py,
  test_emoji_bot.py, test_entry_point_contracts.py, test_entry_points.py.
- test_fetch_emoji_ids.py, test_fetch_pack_ids.py, test_fetch_pack_limit.py,
  test_identity_migration.py, test_ingest_retention.py, test_launcher_contract.py,
  test_logsetup.py, test_make_emoji_pngs.py, test_media_bounds.py,
  test_media_fitting.py, test_media_paths.py, test_migration_lifecycle.py,
  test_migration_provenance.py, test_native_identity_cli.py,
  test_native_similarity.py.
- test_pack_archive.py, test_pack_export.py, test_pack_locks.py,
  test_pack_manifest.py, test_pack_rows.py, test_plan_apply.py, test_plan_status.py,
  test_preflight_outcomes.py, test_publish_cli.py, test_publish_contracts.py,
  test_publish_dedup.py, test_publish_invariants.py, test_rebuild_dedup_plan.py,
  test_rebuild_dedup_resume.py, test_rebuild_dedup_state.py,
  test_reconcile_identity.py, test_reconcile_uniqueness.py, test_remap_ids.py,
  test_repaintable.py, test_resume_safety.py, test_sqlite_snapshot.py,
  test_state_artifacts.py, test_status.py, test_sync_order.py,
  test_telegram_client.py, test_unresolved_mutation.py, test_verify_logos.py,
  test_video_collision_ingest.py, test_video_identity_fidelity.py,
  test_video_timeline_comparison.py.

Parent-owned protected files, existing README provenance, production files,
native/scripts/coins/manifests/workflows and local memory were not edited. Parent
owns additional native-report consumer edits and helper extraction already present
in the working tree; those diffs are not claimed as this cohort's work.

## Remaining boundaries and verification

No concrete missing reference dependency was found in the inspected closure.
A literal search of top-level tests after rewiring still sees a production Catalog
import in parent-owned test_sandbox_lifecycle.py; it was deliberately not edited.
The immutable oracle import strings remain by design and their AST fixture loading
must not be changed to ordinary application imports. Other parent-owned runtime
boundary tests may intentionally name blocked production modules; those are
negative assertions, not fallbacks. Static source scans/production package copies
in packaging/codec fixtures are not themselves target imports and were preserved.

No test, build, target import, codec, server, child process, live-data read, commit
or push was performed. Parent must run AST/body equivalence, isolated import and
source regression/native parity checks on the integrated tree; static edits are
not evidence of passes. T051 and production removal/cutover remain parent-owned
and gated by full parity/recovery/rollback/platform acceptance.

Codebase Memory search and source confirmation preceded closure discovery; prior
coverage had changed metadata and is not completeness proof. Fresh official import
research and versioned CPython usage from the preceding same-day reference cohorts
apply here: ordinary sibling imports and module execution via -m, not import
aliases. Sources: https://docs.python.org/3.11/reference/import.html sections5.7/5.8;
https://github.com/python/cpython/blob/b37291ec09b4b75a451735d6d356e4c2e27c7e69/Lib/test/test_importlib/import_/__main__.py .
No external code or dependency was adopted.
