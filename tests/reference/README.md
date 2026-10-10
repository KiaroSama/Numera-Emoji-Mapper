# Test-only legacy backend references

This package is the bounded feature013 T051 catalog/migration/panel foundation,
not a production fallback or permission to remove Python applications. Import modules
explicitly under `tests.reference`. Production originals stay intact until full
parity, rollback and cutover gates pass.

## Provenance

Baseline: `d173116`. Before copying, all six original source files had no diff
against that baseline. Catalog/ingest SHA-256 also matched `git show` output;
original hashes for the migration cohort were recorded from the unchanged sources.

| Reference | Original source | Original SHA-256 | Adaptation |
|---|---|---|---|
| `catalog.py` | `emojikit/catalog.py` (678 lines) | `be0db6f7a510c88ece2ad8348cafc20d8f287c97671cdf5f3b2a4b86d9ebe7a0` | Keep `.ingest` local; explicitly import retained `emojikit.media_paths`, `emojikit.maintenance.writer` and `emojikit.similarity.require_native`. No catalog logic changed. |
| `ingest.py` | `emojikit/ingest.py` (134 lines) | `d2e7740602bd6993e51ab6fd0c0244764ca71e47b9dbafa2c6bdd5895050d0b6` | No byte changes; its original absolute imports already name retained runtime leaves. |
| `migration_bundle.py` | `emojikit/migration_bundle.py` (314 lines) | `8b2648956047e95fe5fcc1e779e85041366de917f1290faeb7b1d6c0e6e040c8` | Relative shared-leaf imports become explicit `emojikit.maintenance`, `media_paths`, `sqlite_snapshot`, `state_artifacts`; bundle logic unchanged. |
| `collection_migrate.py` | `emojikit/collection_migrate.py` (592 lines) | `4cb0211b690af6960a59e6c5ee851f64dc376aa611712fb938f6a87ed8ec8a68` | Local reference bundle/catalog imports and both lazy collection-state lock-path imports; all shared runtime leaves remain production imports. |
| `collection_state.py` | `emojikit/collection_state.py` (334 lines) | `f8c6e96a1ea7c2fa34484ab5bd7c5cdcda4b7099d86f793acd4464e7284faf99` | Both catalog imports point locally; `ROOT` uses `Path(__file__).resolve().parents[2]` to retain original repository coordinates. |
| `identity_repair.py` | `scripts/identity_repair.py` (214 lines) | `544f267ab1d893d4583989cd580379c80ba9ea4e67aae54fe27bd6e01bea78e5` | Local reference migration import, repository-root `parents[2]`, remove script-only `sys.path` insertion/unused `sys` and obsolete E402 annotations. CLI/report logic and output text unchanged. |

These are **adapted, importable references**, not immutable oracles. The original
algorithms, SQL, validation, lifecycle, logger name and error messages are retained.
`__init__.py` only identifies the package; it exports or redirects nothing. All
sources fit below the 800-line ceiling, so no responsibility split is needed.

`tests/oracles/` is a separate immutable, hash-pinned source corpus used by the
AST-based fixture generators. All its bytes and generated fixtures are untouched.
Do not update either kind of reference merely to hide a native mismatch. Any later
reference adaptation must document its source and exact change here.

## Dependency and coordinate boundary

The first cohort is catalog plus ingest. The approved next cohort adds only
migration_bundle, collection_migrate, collection_state and identity_repair. Source
imports, including function-local imports, were inspected before copying.
Collection migration needs collection_state for `_lock_path`; that module's full
original implementation is retained with local catalog imports, not replaced by a
hand-reimplemented helper. Identity repair is required by `_migration_process`.
The approved panel cohort below adds only its eight named responsibility files.
No publisher, collector, archive or unrelated CLI application is copied.

- Catalog imports local ingest and retained `media_paths`, `maintenance`, `similarity`.
  Maintenance imports retained `packstate`; similarity loads `emojikit._native`.
- Ingest imports retained `identity`, `errors.MediaError`, `packstate.write_json_atomic`
  and `similarity.near_indices`. Identity imports retained media/video decoding;
  media imports its codec helpers and lazily uses `cli_env.safe_int_env`.
- These leaves are not copied. Declared development consumers include the exact
  media bridge (media/identity/errors/packstate), sandbox clone (media_paths/
  maintenance/packstate), coin tools (packstate and shared media) and native
  installation/similarity tooling (similarity). Their eventual runtime inventory
  and any wider consumer cutover remain the integrator's responsibility.
- Catalog/ingest/migration_bundle/collection_migrate do not derive roots from
  `__file__`. Collection_state and identity_repair now use `parents[2]`, preserving
  the same repository root from their extra-deep `tests/reference/` location.
  Shared `emojikit.media_paths.PROJECT_ROOT`, `emojikit.packstate.ROOT`/`LOCK_DIR`
  and logging paths still resolve from original runtime locations, **not** `tests/`.
  Data-prefixed `./` paths remain catalog-relative; unmarked legacy relative paths
  remain repository-root-relative. Existing detached-root test patches still
  target the shared media-path and lock defaults deliberately.
- `MediaError`/`UndecodableVideo` retain their `emojikit.errors` identity;
  ownership failures retain `emojikit.packstate.LockBusy`, and backup failures
  retain `emojikit.sqlite_snapshot.BackupTimeout`. Maintenance thread-local
  ownership remains one shared module. Original collection_state defines its own
  application-only `StateError` and `SetDrift`; reference callers must use those
  reference types, not production collection_state's distinct class objects.
- Migration bundle keeps retained `media_paths`, `sqlite_snapshot`,
  `state_artifacts` and `maintenance`; sandbox_clone is a declared development
  consumer of all four. Identity repair keeps runtime identity and logsetup,
  which itself imports retained cli_env. No migration/report decision delegates
  back to a production application.

There are no `sys.modules` aliases, import hooks, `sys.path` insertion or
`emojikit.__path__` changes. Normal `tests.reference` imports still execute the
existing `tests/__init__.py` credential/network guard. This is namespace isolation
for retired application code, not a standalone sandbox or independent codec/native
compute oracle: retained shared leaves remain explicit runtime prerequisites.

## Consumers and verification handoff

The catalog cohort rewires `test_native_catalog.py` and
`test_native_real_catalog.py`. The storage test patches `tests.reference.catalog.catalog_identity`,
where `Catalog.add` actually looks up the binding; production catalog is not
patched. The real-catalog test owns its existing repository-root expression instead
of importing `ROOT` from `test_native_local_cli`, which would transitively load the
production catalog. Assertions, scratch cleanup, native calls and optional private
fixture behavior remain unchanged. No private fixture or operational data was read.

The migration cohort rewires only `test_native_migration_cli.py` and
`_migration_process.py`. Direct source migration/bundle/catalog/repair imports now
name explicit references, including both lazy collection_state imports. The native
migration test owns its same BINARY/ROOT expressions instead of importing the
production-backed local-CLI test. Existing native fixture-root package copying,
real-VP9 generation, command assertions and cleanup are unchanged.

The generated-video scenario now imports `encode`, RED and BLUE from the
integrator-owned `tests._video_fixtures` extraction instead of importing the
production-backed video test module. The integrator preserves that existing
real-VP9 helper's exact behavior; no helper duplication or application alias is
added here. Helper availability and integrated verification belong to that change.

An import-isolation ceiling intentionally remains outside this dependency cohort:
`_migration_process`'s lazy hold-publisher/archive/ingest/panel branches still test
original applications; only hold-catalog and migration/repair bindings switch
here. Those probes are not certified independent of production modules.

Other consumers also still import production applications. Existing tests remain
the runnable checks; no duplicate suite is added. Optional real-data scenarios
still require explicit local fixtures and are not required CI coverage.

No test, build, target-module import or runtime process was executed by this agent.
The integrator reported selected catalog tracer/Ruff success after the first
cohort; that is not migration-cohort or final integrated verification evidence.
The integrator must verify migration-reference imports without production
catalog/ingest/collection_state/collection_migrate/migration_bundle/identity_repair,
repository-root resolution, shared exception/ownership identity, existing replay/
rollback cases and oracle byte preservation. T051 stays incomplete and production
cutover waits for the full ledger.

## Panel reference extension (2026-10-10)

Source imports and function-local imports were inspected before this cohort was
copied. Seven `panel*.py` responsibilities plus `script_json.py` reuse the existing
reference catalog/collection_state and explicitly retained runtime leaves. No
production source, immutable oracle, asset or native browser harness is changed.

| Reference | Original source | Source SHA-256 | Adaptation |
|---|---|---|---|
| `panel.py` | current `emojikit/panel.py`, 644 lines | `d670ffecc40f2e31631376160d6475ab1951bdc3840f98ed1e564de09ebc143f` | Local reference application imports; `ROOT=parents[2]`; both lazy `build_pack.load_env` imports name the actual retained `cli_env.load_env`. |
| `panel_plan.py` | `d173116:emojikit/panel_plan.py`, 196 lines | `4c6b43a35ea5fa5907cc1933524b1a5c79214d12bb13cccc269e34d7ae38b1e4` | Byte-exact copy; packstate/state_artifacts imports already name retained leaves. |
| `panel_view.py` | `d173116:emojikit/panel_view.py`, 199 lines | `8935c34372dea19ec47d586369344390eecd44f0fc9b65a24f9581f13be5cd27` | Local Catalog import only; shared operator_config/similarity remain runtime. |
| `panel_save.py` | `d173116:emojikit/panel_save.py`, 121 lines | `243488961d6d5b1626a361fd431f9b4872c3747cc56f0168938d95c196306391` | Catalog, collection_state, panel_plan and panel_view imports become local references. |
| `panel_instance.py` | `d173116:emojikit/panel_instance.py`, 85 lines | `ec4ae6f7cab4ca817f885648ed1ac8a497ff800854f384f1f9b2c9024331d0b2` | Byte-exact copy; stdlib only. |
| `panel_logging.py` | `d173116:emojikit/panel_logging.py`, 55 lines | `e0ca46502251fbc326a46948f68c46f041d4f786b7ea1fe4f11cee527b327959` | Byte-exact copy; stdlib only. |
| `panel_preview.py` | `d173116:emojikit/panel_preview.py`, 196 lines | `832548b52fdc4862ca02d8e7d4af9eae84751ef590f333c67f1dd483e9d26024` | Byte-exact copy; retained media/video_decode codecs. |
| `script_json.py` | `d173116:emojikit/script_json.py`, 20 lines | `b9afcea858593c9e532ee3690038433e48eefc00910af603b6b1e8b90a3e56e4` | Byte-exact copy. |

The current source panel differs from `d173116` only in the previously approved
DEFAULT_PORT binding to retained `cli_env.PANEL_PORT`, rather than repeating9450.
That current binding is preserved and not misrepresented as an immutable baseline
snapshot. `build_pack` directly reexports the identical `cli_env.load_env` function;
using its actual leaf definition avoids loading builder/announcement applications.
The original Telegram getMe lookup still uses retained `emojikit.telegram_api`,
which retained coin tools also consume. Its call/retry/exception behavior is not
reimplemented or silently replaced. Operator configuration remains the shared
runtime leaf; no operator values/defaults are introduced.

Reference panel assets are still the original repository `assets/`, not a second
copy under tests. Reference preview cache filenames/locks, HTTP/security/body
validation, saved-intent semantics, telemetry schema, startup/reuse and source
outputs are unchanged. `PlanError` belongs to reference panel_plan throughout
panel/save/test consumers. Shared runtime media/ownership exceptions keep their
identity. These source reference classes are not interchangeable by class identity
with production counterparts, and no alias makes them appear so.

Source consumers in `test_panel*.py` now import explicit panel/catalog/state/plan
references. `test_panel_intent_persistence` also uses the already-existing reference
migration/repair/signature cohort for its composed rollback scenario. The source
panel process test launches `python -m tests.reference.panel` instead of the
production module, preserving the same scenario and ensuring the normal tests
package guard executes. Existing test assertions, cleanup, timeouts and real-media
helpers remain intact. Native browser suites keep their installed native server
and owned process harness; only their source model/JSON/catalog bindings in
`_panel_browser_fixtures.py` change to references. No Python fallback is introduced.

The seven panel responsibility files and script helper all remain below700 lines,
so no splitting is needed. No tests, module imports, servers, codecs or builds were
run by this agent. The integrator must verify source/reference import isolation,
asset-root resolution, composed plan/migration exceptions, unit/process parity and
that browser suites still launch the native backend. Wider source consumers,
including `_migration_process`'s unrelated writer probes, remain separate closure
work before any production removal.

Fresh live Python3.11 import docs sections5.7/5.8 confirm single-dot package sibling
resolution and that `-m` supplies the module spec. Public GitHub upstream query
`repo:python/cpython "relative import" "-m"` returned old implicit-import proposals,
not permission to use import tricks. Usage query `"from ." path:Lib/test/test_importlib/import_ repo:python/cpython`
and current repository search were refreshed; versioned maintained
[package __main__](https://github.com/python/cpython/blob/b37291ec09b4b75a451735d6d356e4c2e27c7e69/Lib/test/test_importlib/import_/__main__.py)
uses an ordinary sibling import. No external implementation was copied.

## Import research

Checked 2026-10-10 against current official Python 3.11 documentation and versioned
CPython source. Regular-package imports execute parent initializers; explicit
relative imports select the named sibling rather than production package modules.
Patching must target the namespace where the binding is looked up.

- [Python import system](https://docs.python.org/3.11/reference/import.html),
  regular packages, module cache and package-relative imports.
- [unittest.mock: where to patch](https://docs.python.org/3.11/library/unittest.mock.html#where-to-patch).
- [CPython 3.11.14 relative-import tests](https://github.com/python/cpython/blob/b37291ec09b4b75a451735d6d356e4c2e27c7e69/Lib/test/test_importlib/import_/test_relative_imports.py),
  maintained official reference, PSF license; inspected, no external code copied.

Public upstream query `repo:python/cpython "relative imports"` surfaced proposal
issues #74115/#126926, not a reason to adopt implicit imports. Real-use query
`"from ." path:Lib/test/test_importlib/import_ repo:python/cpython` found the
versioned official test above; repository search `repo:python/cpython` confirmed an
active, non-archived reference. No dependency or runtime version change is part of
this test-only slice.

## Integrator CLI references (2026-10-10)

These four sources are adapted executable references, not new production commands
and not immutable oracle files. Only the documented import/root changes differ.

| Reference | Original SHA-256 | Adaptation |
|---|---|---|
| `build_pack.py` | `315bed309ca6c07518a5639a9c9d3234673061a49b37ef4cf1b612b90f1fe1d9` | Repository root uses parents[2]; repaint gate is a local reference. |
| `make_emoji_pngs.py` | `8f19d62d444ae0dd50420d24e5619372d2876eab9220c5e91a851a54b09f396d` | Repository root uses parents[2]; exit helpers import their retained cli_env definition. |
| `plan_status.py` | `11439c5fdc303263d95923df1307795008d196378b241fd578c308450b85310e` | Repository root uses parents[2]; retained cli_env and local reference panel_plan. |
| `repaint_gate.py` | `7df425ea375f98e25bcf736b24ae471076317791b330ded03be6dc83967c9c9f` | Byte-exact source copy. |

Integrator AST comparison confirms equality to current source after precisely these
adaptations. Root/exception import-isolation tracers ran with production catalog,
migration, panel and builder counterparts blocked; selected native catalog and
migration/source-restore cases passed. Those are selected seams, not the full
cutover gate. Runtime originals and immutable `tests/oracles` remain preserved.

## Integrated behavior integrity checkpoint

`behavior.json` records canonical Git source SHA-256 (LF text, not mixed checkout
line endings) and source-derived normalized AST
behavior digest for each production-module reference. Canonical AST JSON avoids
version-dependent `ast.dump` formatting; empty `type_params` fields introduced in
Python 3.12 are omitted, while nonempty fields remain significant. The normalization ignores
imports and only ROOT/OFFSET_FILE assignments; it does not ignore function bodies,
validation, exception handling, SQL, outputs or control flow. The integrator checked
source/reference equality before creating these pins. Later reference edits must
not regenerate pins merely to hide a backend mismatch. Identity-repair relocation
remains separately documented because it also drops the old script sys.path setup.

`tests.test_native_consumer_boundary` verifies those pins, all twenty immutable
oracle hashes, and fresh-child imports with retired production module names blocked.
The references remain tracked CI inputs and excluded from the production wheel.
These checks preserve a runnable rollback oracle after production retirement;
selected passes do not grant permission to bypass full cutover acceptance.
