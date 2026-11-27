# Remap recovery, durable paths and destination feedback re-audit

Follow the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit
chain before integration. The inspected public tree has no scoped Rules,
AGENTS.md, CLAUDE.md, Spec Kit constitution or configured project hook. Read
the owner's actual environment instructions; do not invent or bypass them.

## Reviewed baseline and finite scope

This pass starts at main `18392484f0e79feb3af3317ed322befff9c94eff`.
PR #112 was merged by the owner. The owner integrated PR #113's exact
`07834279777d53d44981b3a92f10ada100c4e0de` commit through `304a4c9`, applied
follow-up `1839248`, closed that PR and retired the old branches. At the start
of this pass only `main` remained remotely and no PR was open. Findings R25
through R31 remain repaired; they are not reported again as new defects.

The review examined the follow-up and its adjacent runtime, panel and Worker
contracts, reran the retained suites and used ordinary production CLI, browser
and workerd execution. Independent review checked the combined repairs. This
audit may open a concrete implementation PR but must not merge, close or
delete remote items. No production Telegram send, deployment or remote D1
write is part of its tests.

## R32 — A matching pack digest can conceal an incomplete cache (P1)

**Observed failure.** Cache a complete pack `[a, b]`. Let the live pack change
to `[a, c]` while downloading `c` fails. Reconciliation retires `b` but retains
the last complete `[a, b]` digest. If the live pack returns to `[a, b]`, the
digest-only fast path skips the absent `b` signature. The ordinary calibrated
`coin-remap-ids --apply` command can report success and replace the canonical
map with an incomplete result. Removing and restoring a configured pack has
the same failure. An unresolved current error with an existing signature can
also become permanently nonretryable.

**Repair.** A reusable manifest must have both the matching digest and a
usable signature for every current custom-emoji ID, with no unresolved error
for that ID. The shared `signature_ready` predicate verifies an encoded
1024-byte signature and absence of a current error. The digest shortcut,
per-ID reuse and final completeness check all use that predicate. Missing or
invalid signatures and unresolved current errors must be fetched again.
Failure remains incomplete and candidate-only with exit 3; success clears the
error and durably checkpoints the proven cache. An unchanged complete cache
still performs no unnecessary download or write.

**Regression contract.** Seven new guarded tests in
`runtime/tests/supplemental_coin_remap_recovery.rs` cover restored packs,
reverted live manifests after a failed replacement, unsuccessful recovery
and later success, unchanged-cache reuse, overlapping packs, current-error
retry, and invalid/null/wrong-length signatures through public reconciliation.
The ordinary CLI reproduction matrix had only its clean control pass before
the fix: 1 of 6 expected outcomes. Cache deletion, forced digest resets or
silently suppressing incomplete counts do not satisfy this repair.

## R33 — Implicit backup and lock paths can corrupt durable data (P1)

**Observed failure.** The old collision check compared only explicitly named
paths. Selecting the implicit `map.prebroken.json` as cache or state allowed
exit 0 while replacing the recovery backup with another artifact. Selecting
it as candidates overwrote a valid backup even when partial application was
refused. Selecting a family-lock path as state let lease metadata overwrite
the state; selecting the canonical-lock path as output put lease metadata in
the backup. A malformed existing backup or a directory at that path could
also be accepted while the canonical map changed.

**Repair.** Validate all five durable roles together before lease acquisition,
network traffic, cache/state writes or canonical publication: state, cache,
canonical map, candidates and the implicit backup. Use the existing physical
path and canonical-existing-prefix checks, including Windows case spelling.
Reserve the project's `.locks` storage. Apply the same output/candidate/backup
invariant to the public direct-persistence entry point. Require an existing
backup to be a regular JSON object and preserve it byte-for-byte. Revalidate
the backup under the canonical lease before publishing a replacement.

**Independent-review correction.** Different names alone do not establish
different files. An initial candidate still corrupted state/output through
hard links to lock files; two isolated actual-CLI cases proved that gap.
The final check also compares existing durable-file identities with regular
files in lock storage. Unix uses device and inode; Windows uses volume plus
the full 128-bit `FILE_ID_INFO` through the existing windows-sys dependency.
Identity-query failures refuse the operation. Harmless output-to-backup hard
links remain supported: atomic output replacement preserves the backup inode.
No general ban on hard links was introduced.

**Regression contract.** The path harness covers backup aliases, all explicit
roles against both lock coordinates, public persistence for canonical and
candidate decisions, invalid backup objects/directories, alternate spellings
and symlinks, and byte preservation. Its hard-link test covers all five
durable roles against both lock files and the direct persistence seam. The
positive hard-linked backup case must continue to pass. Every refusal must
leave the pre-existing state, cache, canonical map, candidates and backup
unchanged, without a provider request.

## R34 — A state object without `sets` panics (P2)

**Observed failure.** Passing the otherwise valid JSON object `{}` as the
remap state produced exit 101. The recovered state is a map, so indexing a
missing `sets` key panicked before the intended validation error.

**Repair.** Use `get("sets").and_then(Value::as_array)` and return the existing
ordinary validation error, yielding CLI exit 2. Do not treat malformed input
as an empty inventory. The path harness includes missing, null and numeric
inventories and verifies exact preservation of durable data. Malformed member
indices remain ordinary errors. An early controlled error is the required
behavior; hiding the panic with a broad exception boundary is insufficient.

## R35 — Destination feedback persists after leaving the grid (P3)

**Observed failure.** During a real native drag the panel advertised
`Drop -> Pack 2` after the pointer moved to the header or Holding. Releasing
over the header correctly cancelled; dropping in Holding correctly retained
the original pack-7 stamp. The model was correct but the displayed operation
was misleading. The prior R31 destination-selection repair remains correct.

**Repair.** Clear only the destination indicator in the existing document
`dragover` handler when the current target is outside `#grid`. Keep aimed pack,
provisional order, capacity checks, drop rules, undo history and saves intact.
Re-entering a valid grid target restores the existing feedback naturally.

**Regression contract.** The retained curation test gains native header
leave/re-entry/cancellation and Holding scenarios, including empty/hidden
feedback before release, exact snapshots, no cancelled save, Undo/Redo and
the final Save payload. Its new assertion failed on unchanged baseline assets.
The full panel remains 156 identities with the original one-worker, zero-retry
and timeout settings. No original assertion is removed or weakened.

## Maintenance and unchanged scopes

The fresh official-registry check found no newer stable version among 39
distinct direct Cargo/npm packages. Rust 1.100.1 was released on November 27
at 19:31:41 UTC and fixes an Iterator inference regression. This PR updates
the toolchain, every actual workflow compiler selection, native developer
helper admission/environment/help, current guides and strict version tests
together. Historical records and illustrative language fixtures keep their
original values. Manifest/lockfile dependency graphs do not change.

Weekly Dependabot already covers all manifests and Actions. The README's
release badge already follows GitHub tags and links to this repository's tags.
Primary CI already runs on each push and PR and verifies the triggering source
revision. These correct configurations are preserved.

All 607 commits reachable at audit start used the required human author and
committer email, `Kiaro.Sama.Dev@gmail.com`, apart from authentic bot identities.
Every new human commit must retain that identity. No history rewrite is needed
for the examined history.

The complete unchanged Worker suite passes 248 tests in 13 files. Additional
actual workerd probes pass all six transport scenarios, including a stalled
response body and the unmodified 10-second default timeout, and ten actual
route/D1 requests using both production migrations and a loopback Telegram
fixture. Redaction, fan-out accounting and automatic mirror suppression remain
correct. Wrangler dry-run packaging and typechecking pass; there is no new
Worker defect or tracked Worker change in this pass.

## Required acceptance and closure

Local pre-publication results are concrete and bounded:

| Check | Result |
| --- | --- |
| Rust 1.100.1 release executables | Built; unchanged-source confirmation passed |
| Actual cache recovery CLI matrix | 6/6 |
| Actual durable-path CLI matrix | 10/10 |
| Lock aliases and valid backup hard-link control | 3/3 |
| Malformed-state CLI matrix | 7/7 |
| Previous R30/R29 actual CLI controls | 4/4 and 16/16 |
| New Rust harnesses and strict compiler-contract harnesses | All four compiled; execution belongs to guarded CI |
| Original/supplemental inventory | 1516 original IDs; 1950 wired Rust functions; 0 errors |
| Language policy | 1157 candidate files; 0 violations |
| Full panel on frozen final panel assets | 156/156; 0 skipped, flaky or retried |
| Worker suite / native transport / native routes | 248/248, 6/6 and 10/10 |

The 46 CLI scenario executions use one final Rust 1.100.1 binary with SHA256
`5603e8d51810930cce6aa2dc22b50a6e184cddf55f19be815d3218c5bd0e4ec5`.
The missing-state scenario intentionally occurs in both path and input matrices.
The local panel's frozen binary precedes the later runtime-only hard-link
refinement; final panel source and all 156 identities match the published
candidate. The final aggregate browser and native gates still belong to CI.

The accompanying PR verification comment and downloadable handoff record the
final source SHA, actual CLI matrices and all completed workflow results.
They must not count discovery, compilation or a prior revision as execution
of the final candidate. Local protected native admission cannot enumerate its
private interface in this host (`EPERM`); the guard must remain intact, and
the ordinary four-platform CI supplies native execution evidence.

Run unfiltered native tests, format, Clippy, original/supplemental inventory,
language policy and packaging on Linux/Windows x64/arm64. Execute all 14 new
guarded regressions, 156 panel cases and 248 Worker cases. Require visible
primary CI on the final published and then integrated commit. Keep every
first failure and fix a demonstrated cause before a new candidate run; do not
increase retries/budgets, replace expected data or disable admission.

The owner-authorized integration agent must review every open PR and secondary
branch under the owner's Rules, integrate verified work directly when suitable
or manually implement equivalent complete repairs before closing superseded
PRs. Finish discovered defects in the same session, then commit/push and
establish fresh final-head CI. Remove task-owned caches, temporary probes and
obsolete generated artifacts after preserving proof. Keep operator data,
unaccepted recovery backups, licenses and immutable reference evidence safe.
The audit agent itself leaves the new PR open and performs no merge or closure.

## Primary references

- [PR #113 integration record](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/113#issuecomment-7228408523)
- [Rust 1.100.1 release](https://github.com/rust-lang/rust/releases/tag/1.100.1)
- [Unix file identity metadata](https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html)
- [Windows FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/desktop/api/winbase/ns-winbase-file_id_info)
- [Windows file-information API](https://learn.microsoft.com/en-us/windows/desktop/api/winbase/nf-winbase-getfileinformationbyhandleex)
- [Playwright native drag actions](https://playwright.dev/docs/input#drag-and-drop)
- [Cloudflare request-context AbortSignal guidance](https://developers.cloudflare.com/changelog/post/2026-08-19-abort-signal-global-scope/)
- [Cloudflare Fetch API](https://developers.cloudflare.com/workers/runtime-apis/fetch/)
- [Official Spec Kit](https://github.com/github/spec-kit)
- [GitHub tag badge](https://shields.io/badges/git-hub-tag)
- [Upstream refresh record](../upstream-refresh.md)
