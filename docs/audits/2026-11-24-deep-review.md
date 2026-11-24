**First, follow every owner-provided Rule, AGENTS.md instruction, hook, and required Spec Kit step. Never disable, bypass, rewrite, or silently replace a required gate to make this work pass.**

# Deep audit and repair integration instructions

| Field | Value |
| --- | --- |
| Repository | [KiaroSama/Numera-Emoji-Mapper](https://github.com/KiaroSama/Numera-Emoji-Mapper) |
| Audit started | 2026-11-24, Asia/Tehran |
| Reviewed main | `c0b8368e9f34a8c02ce678d3b9b649172de10839` |
| Tracking issue | [#104](https://github.com/KiaroSama/Numera-Emoji-Mapper/issues/104) |
| Open implementation PR | [#105 — deep audit repairs](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/105) |
| Repair branch | `audit/deep-review-20261124` |

## 1. Purpose and authority

This is an implementation-backed audit, not a request to rediscover and rewrite the same repairs. The repair branch contains the production changes and additive regression coverage described below. Review the associated repair PR and its exact-head results, then finish integration under the owner's Rules. The audit agent is authorized to open Issues and PRs; it must not merge or close them. The owner's subsequently authorized integration agent performs that disposition.

The repository's `.github/CONTRIBUTING.md`, `tests/README.md`, `assets/tests/README.md`, `CONTEXT.md`, existing architecture decisions, and historical audit records were reviewed. No tracked `AGENTS.md`, owner `Rules` directory, `.specify` workspace, or configured repository hook path was found at the reviewed baseline. This does not waive rules installed in the owner's working environment. Locate and read those rules before integration; retain the owner's required documentation location and Spec Kit sequence.

Use the existing repair branch for corrections. Do not create another branch unless isolation, repository protection, or the owner's Rules genuinely require it. If another branch becomes necessary, integrate it completely before ending the integration session. Do not leave an extra branch, unresolved review, failing acceptance gate, or partially implemented finding as the next session's work.

## 2. Findings and implemented repairs

### NEM-01 — Invalid publication identities become durable progress (P1)

**Affected code:** `runtime/src/publication.rs`, especially `Publisher::reconcile_checked`; the durable effect is visible through `Catalog::uploaded`, publications, and pending selection.

**Failure:** an unrecorded live tail item can be correctly identified by its file identity while its `custom_emoji_id` is missing, null, or empty. Previously the reconciler called `uploaded()` first, marking the item uploaded and inserting its publication row, then rejected the unusable ID during a later pass. The caller saw an error and unchanged set membership, but reopening SQLite showed durable publication progress. A later shorter, valid live observation could succeed while the refused item remained excluded from pending work. Distinct attributed keys could also acquire the same CID. Wrong JSON types were already rejected by the catalog before a transaction; those variants are preservation controls, not additional demonstrated durable-corruption cases.

**Repair:** admit a nonempty string CID before each row's durable publication write. Validate the recorded prefix and track the IDs admitted within the observed live set, including a usable logo ID. Refuse a repeated CID before attributing another key. Keep existing duplicate-key refusal and positional identity checks. Do not change the broad legacy nullable-ID catalog API or add speculative catalog-wide uniqueness constraints.

**Critical compatibility requirement:** preserve earlier independently proven row commits when a later row is refused. An all-or-nothing rollback of the whole live tail would break the original recovery contract. The new code commits each valid tail row only after its own validation, and refuses the failing row without suppressing its pending eligibility.

**Regression:** `supplemental_publication_admission` checks seven malformed CID shapes, actual HTTP observations and SQLite state, reopening the database, a second observation, pending eligibility, preserved valid preceding progress, and duplicate IDs behind both newly committed and recorded prefixes. Its three tests failed for the intended durable-state assertions on the regression-only baseline. The original reconciliation, resume, publication and recovery suites remain mandatory.

**Closure:** malformed/unproven rows never become published; valid preceding progress survives; a retry can still select the refused item; duplicate identities cannot be silently attributed to distinct positions in one live set.

### NEM-02 — Supported unbranded packs cannot archive correctly (P2)

**Affected code:** `runtime/src/archive_rows.rs`, `archive_sync.rs`, `archive_check.rs`; operator guidance in `docs/guide-cli.md`.

**Failure:** `--no-brand-logo` persists `logo:false`, but archive synchronization still required `BRAND_LOGO_PATH` and copied `001_logo.png`. Offline checking always required that file, and an unidentified first slot was described as branding. A complete, valid unbranded archive therefore failed a normal offline check; supplying an unrelated logo could add a false artifact.

**Repair:** use one shared branding decision. Explicit `logo:false` means no logo requirement, copy, adoption, or invented first-slot label. A missing flag retains the historical branded default. Preserve unrelated files already on disk and report them through existing stray-file diagnostics rather than deleting operator data. Explain branded and unbranded slot behavior in the CLI guide.

**Regression:** `supplemental_archive_branding` contains four tests, including real 200-item full packs, no configured logo, an unrelated configured logo, an unidentified first item, and explicit/legacy branded controls. Success asserts every media byte/path, all history identities and slots, the archive's exact file set, and a fresh offline check. Baseline: three expected failures and one positive control.

**Independent product-command evidence:** the ordinary `pack-archive --check` command on an owned synthetic unbranded archive changed from exit `3` and `missing 001_logo.png` to exit `0` and `fresh`. Explicit branded and legacy missing-logo controls still return `3`; a branded archive with its logo still returns `0`.

### NEM-03 — Template substitution corrupts legitimate panel data (P2)

**Affected code:** `runtime/src/panel_http.rs::get_page` and its template renderer.

**Failure:** sequential global replacements inserted serialized item JSON and then scanned it again for later HTML markers. Catalog labels or emoji text containing `__TOKEN__`, `__PER_SET__`, `__HIDDEN__`, `__ASSET_VER__`, or `__ICON_VER__` were silently rewritten. `__STALE__` and `__PREVIEW_TIERS__` introduced unescaped quotes and made the embedded item JSON invalid, preventing normal panel initialization. Testing `script_json` alone did not expose this later corruption.

**Repair:** find and substitute markers only in the unconsumed original template. Append each already-escaped value directly to the output without rescanning it. Preserve the existing escaping of HTML delimiters, script close sequences, and Unicode line separators. Do not encode arbitrary data into replacement directives or relax the panel's existing HTTP/token protections.

**Regression:** two `supplemental_panel_http` tests inspect complete HTTP responses, exact literal labels and emoji strings, JSON parsing, and retained script/HTML/Unicode escaping. The existing F07 browser identity adds a real Rust-rendered-page substep with literal placeholder labels and usable selection/undo behavior; it does not substitute prepared item JSON into the response. Both native tests failed on the baseline.

**Independent evidence:** the existing owned browser Harness was used without Chromium to fetch the normal production panel. The unchanged binary reproduced literal rewrites and invalid JSON. The rebuilt binary passed the same ordinary, literal-marker, and JSON-valued-marker cases. Fixture tokens and full page bodies were not logged.

### NEM-04 — Draft roundtrips move numbered pack logos (P2)

**Affected code:** `assets/panel-draft.js`; regression substeps in the existing curation roundtrip browser case.

**Failure:** importing an exported multi-pack draft collected every logo at the beginning. `[logo1, a1, b1, logo2, a2, b2]` became `[logo1, logo2, a1, b1, a2, b2]`. The actual `packStartsFor` output changed from runs `[1, 2]` to `[1, 2, 1, 2]`; an unchanged draft therefore changed visible logo order and pack-run grouping. Ordinary-item save order excludes logos, so this unchanged-roundtrip reproduction is not evidence of corrupted durable ordinary-item order.

**Repair:** only the unassigned/global brand logo stays fixed at the beginning. Numbered live-pack logos retain their position in the imported snapshot. Keep existing missing-key reconciliation, duplicate handling, and state application semantics.

**Regression:** an actual-production-function Node VM reproduced the baseline failure and then passed unchanged multi-pack, edited within-pack, and global-logo controls. Browser substeps verify an unchanged import does not create an unnecessary dirty/save state, an edited import saves through the real HTTP endpoint, committed SQLite order and the pack plan match the intended grouping, and the global logo remains fixed. Existing browser identities and original assertions remain intact.

### NEM-05 — Failed Bot API lookups falsely claim IDs do not exist (P2)

**Affected code:** `runtime/src/emoji_bot.rs::labels`, `typed_reply`, and optional display enrichment in `reply`.

**Failure:** `labels()` swallowed `getCustomEmojiStickers` errors and accepted malformed result shapes as empty/partial maps. Typed-ID handling then told users that Telegram did not know those IDs. A failed second batch after 200 successful IDs could also produce an unjustified partial absence claim.

**Repair:** return an explicit result from lookup. Require an array of requested, nonempty, unique string CIDs; validate any supplied emoji field as a string. Propagate every failed/malformed identity-lookup batch before any typed-ID reply. A successful, well-formed response may legitimately omit unknown IDs, and that case still produces the existing explanation.

Established IDs extracted from actual custom-emoji entities remain useful when optional glyph lookup fails. Log that display failure at WARNING and retain the star fallback. A successfully established typed identity also survives failure of the separate display lookup. Preserve the original two-lookup behavior rather than changing unrelated bot contracts in this repair.

**Regression:** `supplemental_bot_resolution` covers initial rejection, a later batch failure, malformed results/rows/IDs/emoji/duplicates/unrequested IDs, successful empty results, entity fallback, a valid ID with omitted emoji followed by display failure, and complete 201-ID success with all requested identities rendered once. On the initial five-case baseline, three failure-path cases failed and two controls passed.

**Independent product-command evidence:** the normal bot command used a synthetic authorized user and an owned numeric-loopback fake API. Baseline rejected/malformed lookups each sent a false absence message; the rebuilt binary sends neither. The successful empty result still produces its intended reply. No real bot token or live Telegram service was used.

### NEM-06 — Standard Windows npm is silently undiscovered (P2)

**Affected code:** native contributor command discovery and web-step construction under `runtime/src/developer/`.

**Failure:** the generic executable lookup checked `npm.exe` on Windows. Standard Node/npm installations expose `npm.cmd` and `node_modules/npm/bin/npm-cli.js`; the developer check consequently treated npm as absent and skipped installed Worker/browser checks. Installing Node and both committed npm trees did not guarantee those checks were planned.

**Repair contract:** represent an npm invocation as executable plus literal argument prefix. Preserve a directly executable npm where available, and support the standard Windows layout by running Node with the actual npm CLI JavaScript path. Preserve PATH selection order, support the installation's Node or a valid PATH Node, retain argument boundaries, and use the existing owned child-process/time-limit machinery. Do not concatenate a shell command or introduce a `cmd.exe /c` evaluation boundary.

**Regression:** the baseline-compatible Windows test `standard_windows_node_installation_enables_all_installed_web_checks` first proves the hosted Node installation has `node.exe`, `npm.cmd`, and npm's CLI, then requires discovery and all four installed web steps. Additional resolver/process cases cover absent and incomplete installations, supported layouts, precedence, spaces and shell metacharacters, literal arguments, exit status, and timeout cleanup. Both Windows architectures must execute the real-installation test; a skip is not acceptance.

**Baseline evidence:** both [Windows x64](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45467886364/job/138707567046) and [Windows ARM64](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45467886364/job/138707567524) passed the real-installation prerequisite assertions and then failed the intended npm-discovery assertion. Each focused execution reported one failed test, with the other library cases filtered for this diagnostic step. The final unfiltered suite executes those other cases as well.

### NEM-07 — Windows logo verification accepts invalid/foreign resources (P2)

**Affected code:** `runtime/src/package/binary_check.rs` and the bounded parser in `runtime/src/package/pe.rs`.

**Failure:** the old check found RT_ICON and RT_GROUP_ICON labels in a PE-shaped section but did not prove their resource offsets, group references, image bytes, or expected source logo. The ordinary verifier accepted a 224-byte fabricated file containing dangling icon offsets, even when the requested `--logo` file did not exist. A complete tree containing different icon pixels also passed.

**Repair:** validate the requested PNG against the compiled approved source PNG, parse the PE32+ resource data directory by RVA, and traverse exactly Type/Name/Language levels with checked arithmetic and a bounded entry budget. Reject out-of-file, overflowing, ambiguous, duplicate, malformed, and unsorted numeric resources. Resolve group ID 1 to all expected images and compare dimensions, image metadata, references, and PNG payload bytes against the repository-approved ICO. Accept the resource compiler's observed planes normalization from `0` to `1`. Reject unreferenced application icon images. Linux's existing exact embedded-PNG contract remains intact.

This verifies the repository's approved PNG/ICO pair and package architecture contract. It is not a replacement for package signatures, Authenticode, general executable validation, or a free-form PNG-to-ICO converter. No new parser dependency or unsafe code is needed.

**Regression:** `supplemental_binary_artifact` includes independently assembled complete x64/ARM64 PE fixtures with seven approved icon payloads, both planes variants, the actual native executable, dangling offsets, changed pixels, missing/changed expected PNGs, and unsorted directory entries. Baseline: three intended failures and one successful valid-artifact control. Independent review found the unsorted-entry variant before completion, and it was added to the same repair.

**Real artifact evidence:** baseline Windows x64 and ARM64 executables were downloaded from successful GitHub Actions artifacts, their ZIP digests checked against GitHub metadata, and their bytes inspected without executing them. Both use group `1`, language `1033`, and seven PNGs with normalized planes. The new verifier accepts those actual binaries and rejects wrong architecture, mutated icon data, dangling resource data, and missing/foreign expected logos.

### NEM-08 / NEM-09 — Misleading CI badge and obsolete panel hint (P3)

The README previously used a static green `CI-GitHub Actions` Shields label. It is replaced with GitHub's actual `ci.yml` badge scoped to `branch=main&event=push`, linking to that history. A badge is a summary; final acceptance must still inspect checks attached to the latest source SHA.

The tag badge already uses the dynamic `img.shields.io/github/v/tag/KiaroSama/Numera-Emoji-Mapper` endpoint and links to `/tags`. It was retained. Tags `v2.0.0` and `v3.0.0` exist at the audit baseline; do not replace the dynamic badge with a hard-coded version. A future tag should update the badge through its service's normal cache refresh, with no README version edit.

The hidden-items help text referred to removed `panel.py --all`; it now names the current `numera-emoji panel --all` command. This is a small adjacent operator-facing correction.

### NEM-10 — Bounded Windows process monitoring aborts on an unsettled inventory (P2)

**Affected code:** `runtime/src/process_owner.rs` and `runtime/src/process_owner/windows.rs`.

**Independent existing failure evidence:** reviewing dependency PR #100 revealed two native failures in [run 45406584449](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45406584449). [Windows x64](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45406584449/job/138488841249) stopped during the migration crash/recovery scenario with `assigned=4, listed=3, slots=8192, attempts=8`. [Windows ARM64](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45406584449/job/138488841301) failed the bounded capture/process-policy scenario with `assigned=3, listed=2`. Its cleanup completed before the test reported the earlier wait error. The relevant process-owner blobs are identical in #100, reviewed main, and the first repair candidate `53767ec`; this is not an npm regression or a superseded pre-#99 failure.

**Cause:** the inventory sampler makes eight immediate queries separated by scheduler yields, then propagates any remaining mismatch as a fatal error. Yielding does not guarantee an elapsed settlement interval. Consequently a bounded operation can fail even though a later complete query and cleanup succeed within its existing allowance. The precise Windows-internal reason for these count differences is not proved by the logs, and the failure is not evidence that a child escaped containment. The counts are far below the fixed buffer capacity; a partial list still cannot establish complete membership.

**Repair contract:** preserve the eight-query round and strict completeness rule, but distinguish an in-bound count mismatch from an actual query, bounds, or pinning error. Bounded wait and cleanup callers treat that one result as pending, return to their existing poll cadence, and retry only within their original budgets. Pending must not become progress, reset wall/idle clocks, permit root reaping, certify quiescence, discard unproven pinned handles, or authorize scratch deletion. Independently signaled handles may still be pruned using their existing terminal proof. A later complete observation resumes the normal proof path only while its original budget remains valid; a late first complete sample cannot reset an already expired idle allowance. Real inspection errors remain immediate failures. Persistent pending reaches the existing wall/idle or ten-second cleanup limit and retains unproven state.

The strict low-level adapters and unbounded interactive `wait_console` contract remain unchanged. Preserve elapsed-time comparisons: supported `u64::MAX` duration values must not be converted into an overflowing `Instant + duration`. The change does not raise the per-round query limit, caller budgets, or test timeouts; it uses the bounded caller's existing poll loop. Do not rerun unchanged failed CI jobs until a green run happens.

**Regression and closure:** six additive cases in `supplemental_process_inventory.rs` retain all existing eight-query, bounds, unknown-query, process-handle, migration-crash and descendant-capture assertions. Four cross-platform cases cover observations spanning multiple incomplete rounds, prohibited progress/reaping/cleanup while pending, unchanged wall/idle/cleanup limits including late first-complete observations, and immediate query/bound/pinning errors. Two Windows-only cases use actual owned children, real native queries with a test-only completeness-withholding counter, retained kernel handles, strict console refusal, and eventual verified cleanup. Both Windows architectures must pass the complete process/custody cohorts on the final head. If either original failure recurs, inspect its new evidence and fix the cause before integration; do not suppress it or call an earlier green run sufficient.

## 3. Verification strategy and evidence boundaries

The immutable baseline has 1,516 mapped identities: 1,360 native, 84 browser behavior, 13 browser performance, and 59 served-source contracts. Supplemental native and Worker suites are additional. The complete browser discovery remains 156 identities. Do not rename, delete, skip, weaken, rebaseline, or generate expected historical fixture outputs from the implementation under test.

The additions comprise 22 cross-platform boundary tests, seven cross-platform npm tests, four cross-platform custody tests, two additional Windows npm tests and two Windows custody tests: 33 new native cases on Linux and 37 on Windows. Browser regression substeps extend existing identities without changing the 156-case inventory. Independent review also tightened same-length icon-payload coverage and corrected Windows fixture path expectations before final acceptance. The first repair candidate's two Windows archive failures exposed mixed-separator strings in the new fixture; joining native path components corrected those expectations while preserving exact catalog-path, content and file-set assertions.

Regression-only commit [`6ba9f7a4ba386346ee5f75090d3591fcd6c67fbe`](https://github.com/KiaroSama/Numera-Emoji-Mapper/commit/6ba9f7a4ba386346ee5f75090d3591fcd6c67fbe) was published before the production repairs. In [CI run 45467886364](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45467886364), [Linux x64 job 138707567658](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/45467886364/job/138707567658) executed 18 new tests: 14 failed for the expected product assertions and four positive controls passed. This is intentional reproduction evidence, not the repair's acceptance result.

Three early diagnostic CI steps run the installed-Windows-npm case, the five boundary targets, and the process-ownership cohort. They use the existing isolated runner and test build profile. They do not replace the later unfiltered suite. CI still runs on every push and pull request, checks out the explicit source revision, and retains Linux x64/ARM64, Windows x64/ARM64, Worker and browser jobs. Packaging and security workflows remain required.

The local environment built all four standalone release binaries, ran Worker typechecks and all 181 Worker tests, ran all 59 served-source contracts, typechecked panel JavaScript, and performed the owned product-command and production-JavaScript probes described above. Native test targets compiled. The protected native runner refused before test bodies because private-interface enumeration returned EPERM. Desktop Chromium could not create its singleton socket; the official locked headless-shell download returned HTTP 403. Neither refusal was counted as a behavioral failure/pass, and no admission guard was bypassed. Hosted native and actual browser execution are the acceptance path.

The associated PR description and the delivered handoff record final source SHA, run URLs, results, and any genuine external blockers. Keep those results tied to the tested commit; documentation-only edits after a green run still create a new head requiring its applicable checks. Avoid an endless commit-to-record-its-own-SHA cycle: put final run receipts in PR metadata or the external handoff.

### Scope reviewed beyond the changed boundaries

| Area | Review and required acceptance |
| --- | --- |
| Catalog, WAL, publication, reconciliation, resume, intent recovery | Durable-state and restart inspection; original and supplemental guarded suites |
| Provider fetching, coin rebuild/replacement, ticker recovery | Transport, identity and state-transition review; fake-service and recovery suites |
| Static/animated media, TGS, WebM/video timelines, retention | Codec/probe/limits review; immutable pixel/timing oracles and real FFmpeg/FFprobe |
| Panel HTTP, security boundaries, preview, save queue, curation, holding, history | All panel modules read; real HTTP, SQLite save assertions, actual Chromium and performance suite |
| Worker delivery, retry/dedupe and malformed API outcomes | Current TypeScript and 181-case Vitest suite; no deployment or live announcements |
| Packaging, installation, signing, platform/architecture verification | Four platform builds/package jobs, real Windows artifact inspection, strict logo verification |
| Developer runner, process ownership, test inventory, launcher, CI and dependencies | Existing isolation retained; unfiltered inventory, Windows launcher contracts, security scans and current PR review |

Live production publishing, deployment, operator credential use, destructive repair of a real catalog, and a physical Explorer desktop session are not simulated by claiming a green unit test. Use the repository's controlled fixtures and hosted platform jobs for this integration; obtain any separately required live acceptance under the owner's Rules.

## 4. Instructions for the authorized integration agent

1. **Read Rules and hooks first.** Re-read the current repository guidance and the owner's environment Rules. Follow the required Spec Kit chain in that environment: establish/reuse the constitution, specify the observable outcomes, clarify only material unknowns, plan, generate tasks, analyze consistency, implement/review, and run every required acceptance gate. Use the owner's actual command names and artifact locations. Do not create ceremonial duplicate plans or a second parallel repair backlog.

2. **Research and record decisions.** Read the changed modules and their callers, the linked historical audits, the associated Issue/PR, existing review threads, and current official documentation. Search the web, especially upstream GitHub repositories, for relevant supported behavior and compatibility changes. Record sources, versions, decisions and rejected alternatives in the documentation location required by the owner's Rules. The reference list below is a starting point.

3. **Enforce the owner's Git identity.** Every past and future human-authored or human-committed published commit must use `Kiaro.Sama.Dev@gmail.com`. Set local `user.email` and inspect both author and committer fields across all relevant branches/tags before publishing. Dependabot and genuine automation/bot identities are exempt. The fetched baseline history and this repair's published commits were checked; no human-email correction was needed. If another human email is found, reconstruct the affected history with the required email, preserve content and topology, verify the rewritten refs, and update remote refs with the owner's required coordination/protection and `--force-with-lease` where applicable. Re-run checks on rewritten heads. Never change the account's email-privacy setting as a workaround or misclassify a human commit as a bot.

4. **Review the actual repair PR.** [PR #105 is already open](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/105) and is the implementation vehicle linked from Issue #104 and branch `audit/deep-review-20261124`. Review each finding's code, regression, baseline failure and fixed outcome. Repair any missed edge case in the same session. Do not simply merge because a summary says the code is fixed, and do not recreate patches that already exist in that PR.

5. **Finish all PR dispositions under the owner's Rules.** At audit time the other open PRs were [#100](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/100) (Wrangler), [#101](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/101) (workers-types), [#102](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/102) (zip), and [#103](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/103) (literal PowerShell launcher arguments). Refresh the full open-PR list, including any newer PRs. Review each under Rules; merge it directly if correct and permitted. If not directly mergeable, resolve conflicts or implement the correct change manually, verify that it is included in the destination, then close the superseded PR with the required evidence. A duplicate, invalid, or superseded proposal may be closed only with the Rules-required rationale. Do not leave any PR indefinitely open, and do not close a valid bug fix while its required behavior remains missing.

6. **Treat dependency PRs as compatibility changes.** Dependabot already covers all three Cargo workspaces, browser npm, Worker npm, and GitHub Actions weekly. Preserve that coverage. Review changelogs, lockfiles, Rust MSRV, Node/Worker compatibility and licenses/security signals before integrating #100–#102. Run locked installs, Worker/browser checks, native/package checks as affected, and the final complete gate on the combined result. Do not add redundant Dependabot configuration or accept a dependency merely because it is newer.

   The reviewed #100 head is `1bf420c5e7da68eeb319bee5cc890ccab5f4b42d`; its Worker checks pass, but both Windows native jobs fail as described in NEM-10 and its browser job is skipped. Do not call that PR green. #101 at `7e8759e19db96c1ae169b345c049c6136e1ee342` and #102 at `045afb0910dbd51a6a60cb89dad83593072c08b6` have passing individual CI/package/security runs, which still do not establish a combined integration result.

   Complete repair acceptance first. A practical order is #102, then #101, then refresh #100 against the integrated fixes. #100 already resolves Workers types `5.20261121.1` while retaining the older manifest floor; #101 raises that floor to `^5.20261121.1`. Preserve both intended manifest changes and the correct lockfile rather than choosing one lockfile wholesale. Retain the existing Sharp override and explicit Worker compatibility date `2026-08-18`. The proposed Wrangler/miniflare requirements are compatible with the repository's Node 24 line. Zip #102 retains the selected feature set while updating the exact pin to `9.8.1`; verify package/archive roundtrips after integration.

7. **Preserve launcher scope.** PR #103 already repairs arguments that resemble PowerShell common options. Review its actual process tests and integrate it under Rules. This audit does not duplicate that patch. Run the Windows x64 and ARM64 launcher contracts on the final combined head, including literal quotes, empty strings, option-looking arguments, working-directory independence, exit codes, and bounded cleanup.

8. **Use the full acceptance commands.** Prefer PowerShell 7 on the owner's Windows environment. The default project location is `G:\Program Files\Portable\Scripts\Numera-Emoji-Mapper`; new installations belong under `G:\Program Files` unless Rules specify otherwise. Use the pinned Rust toolchain and committed lockfiles. Build the guarded runner before all native tests. Never run protected binaries directly to work around a refusal.

| Gate | Command from repository root |
| --- | --- |
| Release executables | `cargo build --manifest-path runtime/Cargo.toml --locked --release --bins` |
| Rust formatting | `cargo fmt --manifest-path runtime/Cargo.toml --check` |
| Rust lint | `cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings` |
| Complete isolated native suite | `cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release --no-fail-fast -- --test-threads=1` |
| Inventory tool tests | `cargo test --config .cargo/runtime-tests.toml --manifest-path tools/test-inventory/Cargo.toml --locked --release` |
| Rust identity inventory | `cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- --mode rust` |
| Worker locked install/typecheck/tests | `npm ci --prefix worker`; `npm run typecheck --prefix worker`; `npm test --prefix worker` |
| Panel JavaScript typecheck | From `worker`: `npx tsc -p ../assets/jsconfig.json --noEmit` |
| Browser locked install and actual execution | `npm ci --prefix assets`; `npx --prefix assets playwright install chromium`; `npm test --prefix assets` |
| Windows launcher process contracts | `node --test assets/tests/launcher-process.js` |

Run each command separately and stop/report a nonzero native exit code before proceeding to dependent work. Also run the repository's language-policy/tooling checks, package/install/signature tests, similarity parity/benchmark, security scans, and all four platform jobs exactly as defined in the current workflows. A selected regression run or inventory-only pass does not replace unfiltered execution.

9. **Keep latest-head checks visible.** Core CI must run and be displayed against the most recent published source commit. Do not use path filters, skip markers, a previous SHA, an unrelated merge commit, or the static README badge as acceptance. After any code, dependency, history, rebase, conflict-resolution, or documentation commit, confirm the actual head SHA and applicable check runs. After integration, verify the destination's latest commit too. Keep the dynamic tag badge linked to this repository's tags.

10. **Finish, do not defer.** If another reproducible bug is found while integrating, capture the failing scenario, fix the cause immediately, add an independent regression, and complete its affected and full required gates in the same session. If a failure repeats, stop rerunning the unchanged attempt: collect new evidence, identify the unmet invariant, and change the implementation or fixture only for a demonstrated reason. Do not weaken assertions, retries, timeouts, or isolation to manufacture success. Genuine external access/infrastructure blockers must be named precisely with their failed action and evidence; do not call that state DONE.

11. **Keep scripts and logs compliant.** English code comments; timestamped INFO, WARNING, ERROR and DEBUG as the primary logging levels; project categories mapped to those levels; no passwords, tokens, session secrets or credential-bearing URLs in logs. Use bounded retries and owned process cleanup. Preserve existing protections. Prefer GPU acceleration only for a measured relevant workload with numerical parity and a safe CPU fallback; these correctness repairs do not justify adding a GPU dependency.

12. **Complete the publication and closure sequence.** Review the candidate and pass its local checks, commit with the required email, and push. Require every applicable hosted gate to pass on that exact published head before merging approved branches/PRs under Rules, then verify the final destination's latest commit. If manual integration was required, prove the change exists before closing its PR. All PRs must end merged or validly closed under Rules, all related Issues/tasks must be resolved with evidence, and the final report must list their dispositions, final SHA, tests and remaining external blockers if any. Do not stop after committing locally or leave unpushed corrections. The audit agent itself has not performed those merge/close actions.

## 5. Optional improvements for owner/agent evaluation

These are proposals, not additional confirmed failures or prerequisites invented after the fixes. Decide on each explicitly under Rules; implement an accepted proposal completely in the same integration session, or record that it was considered and declined without leaving it as an implied unfinished repair.

- **Explain optional local-check prerequisites.** A structured check summary could state which web checks ran and which are unavailable, with the exact missing prerequisite. Preserve mandatory CI failure behavior and avoid a summary that mistakes a skip for success.
- **Publication invariant diagnostics.** Consider a read-only report for malformed historic publication rows or conflicting observed identities. It should identify repair candidates without silently rewriting a real catalog or erasing valid recovery history.
- **Reduce duplicate CI compilation.** Measure shared build time and consider cache/profile improvements that retain exact-source provenance, all four platforms, the full suite and release acceptance. Never trade gate coverage for a faster green badge.
- **Typed API boundaries.** Consider narrowly typed Telegram response validation reused by callers that make absence or mutation claims. Preserve the distinction between authoritative identity lookup and optional display enrichment, rather than applying one failure policy to every caller.
- **Portable operator acceptance.** A documented Windows desktop checklist for Explorer launch, paths containing spaces/non-ASCII text, and intended GPU/media configuration would complement the existing process and hosted tests. Keep manual observations labeled separately from automated evidence.

## 6. Primary references

- [Telegram Bot API: getCustomEmojiStickers](https://core.telegram.org/bots/api#getcustomemojistickers) and [Sticker fields](https://core.telegram.org/bots/api#sticker): identity response contracts.
- [npm's upstream Windows launcher](https://github.com/npm/cli/blob/latest/bin/npm.cmd): standard Node/npm CLI layout and native launcher behavior.
- [Microsoft PE/COFF format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format), especially resource directories: RVA/file offsets, fixed resource hierarchy and sorted numeric entries.
- [Windows Job process ID lists](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_process_id_list), [Job objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), and [Rust scheduler yields](https://doc.rust-lang.org/std/thread/fn.yield_now.html): completeness, containment and bounded inspection policy.
- [GitHub workflow status badges](https://docs.github.com/en/actions/how-tos/monitor-workflows/add-a-status-badge): branch/event-scoped badge semantics.
- [GitHub required status checks](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks): checks must correspond to the current proposed commit.
- [Shields GitHub Tag badge](https://shields.io/badges/git-hub-tag): dynamic tag source and sorting/filter options.
- [GitHub Spec Kit](https://github.com/github/spec-kit): current upstream process and command documentation; the owner's Rules determine the required chain.
- [Rust release notes](https://doc.rust-lang.org/releases.html) and [Rust installation](https://rust-lang.org/install): pinned compiler support and official tooling.
- [Wrangler 4.166.0](https://github.com/cloudflare/workers-sdk/releases/tag/wrangler%404.166.0) and [zip 9.8.1](https://github.com/zip-rs/zip2/releases/tag/v9.8.1): upstream dependency changes to review with #100 and #102.
- Repository evidence: [contribution requirements](../../.github/CONTRIBUTING.md), [tests](../../tests/README.md), [prior recheck](2026-11-24-recheck.md), and [revision-specific CI records](../rust-ci-status.md).
