# Delivery, provider completeness and curation audit

Follow the repository owner's Rules, hooks and prescribed Spec Kit chain before
integration. The public tree inspected for this pass contains no Rules,
AGENTS.md, CLAUDE.md, Spec Kit constitution or configured project hook; this
record does not claim to have read unavailable local instructions.

## Source and integration boundary

The reviewed released baseline is `79cffbd93c78d23eae183867af6f7ae6da223a00`
(`v3.4.0`). Open [PR #112](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/112)
at `a4eb37d5423e26e5bdf23e029ae753b062edb51f` already repairs log-discussion
mirrors, `/log-test` body-read rejection and the Windows quiet fixture. Those
are retained as the ancestor of this follow-up; they are not newly discovered
findings. This audit opens a reviewable follow-up only. It performs no PR
merge, closure, tag creation, deployment or live Telegram delivery.

The four new findings below were reproduced against unchanged production
behavior before their fixes. Existing original test identities, expectations,
timeouts, process ownership and admission guards remain intact.

## R28 — One failed administrator discarded later channel deliveries (P2)

**Cause and impact.** `worker/src/handle.ts` awaited each administrator's reply
inside one uncaught loop. A blocked administrator, transport failure, or later
message-chunk failure escaped the loop. `worker/src/index.ts` logged the error
and acknowledged the webhook with HTTP 200, leaving the remaining
administrators without their copy. Telegram does not retry a successful
webhook acknowledgment.

**Repair.** Catch per-recipient failures, continue attempting the other
administrators, then throw one aggregate exception with the first failure as
its cause. The existing webhook error path performs redaction and records one
error. Do not retry a recipient or replay already delivered chunks. A partially
failed fan-out must not emit the ordinary channel-forwarded success event.

**Proof.** A synthetic 403 for the first administrator originally produced only
that recipient's attempt. The expanded two-bot matrix produced 10 expected
failures and 2 passing success controls. All 12 new tests pass after repair;
the complete Worker suite is 198 cases on the released baseline plus this fix,
and 244 when combined with PR #112. Tests cover first/middle/all recipient
failure, ambiguous transport failure, a second-chunk failure, successful
delivery, redaction and one aggregate D1 error. See
`worker/test/channel-fanout.test.ts`.

## R29 — Malformed market replies falsely reported complete retrieval (P2)

**Cause and impact.** `coin_gecko::names_with` and
`coin_gecko_fetch::fetch_with` combined array conversion and empty-array
detection in one `let ... else { break; }`. A successful HTTP response
containing an error object, string, Boolean or number therefore ended paging
with `complete = true`. Both relevant CLI workflows could exit 0 after an
incomplete provider read.

**Repair.** Separate envelope validation from legitimate termination. Mark a
non-null, non-array market payload incomplete. Preserve the frozen legacy
null marker and empty-array contract. Preserve earlier market-name precedence,
the long-tail request, cached artwork and the normal reconciliation of prior
names. Return the existing incomplete exit code 3 through the established CLI
path; do not expand network retries or change the retained invalid-image skip
policy.

**Proof.** The unchanged ordinary CLI failed 12 of 16 expectation checks;
the four positive empty-array/null controls already passed. The repaired CLI
passes all 16, including malformed first/later pages and exact artwork/name
preservation. Four additive guarded tests are in
`runtime/tests/supplemental_coin_market_payloads.rs`.

## R30 — Errors for removed stickers permanently blocked remap (P2)

**Cause and impact.** `coin_remap_cache::download` found stale IDs only in the
signature map and removed errors for those same IDs. A failed image has no
signature, so its error remained after that sticker disappeared from the live
packs. `coin_remap::run` counted that obsolete error and kept a fully calibrated
`--apply` invocation at exit 3/candidate-only indefinitely.

**Repair.** Build the stale set from the union of signature and error keys,
relative to the complete current live inventory. Retire and checkpoint both
maps only after every required set listing and manifest validation succeeds.
Keep current-image failures retryable. An unavailable or invalid live listing
must return before retirement; never infer deletion from an incomplete scan.

**Proof.** An ordinary production CLI fixture originally failed the removed-ID
and empty-inventory outcomes while its clean and unavailable-listing controls
passed. All four pass after repair. Successful apply preserves the previous
canonical map as an exact backup; unavailable listings retain prior canonical
bytes and unresolved errors. Four additive guarded tests are in
`runtime/tests/supplemental_coin_remap_cache.rs`.

## R31 — Dragging beside an unstamped card skipped its nearest pack (P2)

**Cause and impact.** `aimAt(j)` intentionally calls `runPackAt(j + 1, carried)`
so the backward search starts with the aimed card. The forward search in
`assets/panel-holding.js` also added 1, skipping the immediate next card.
For `[carried pack 7, unstamped target, pack 2]`, an actual browser drag assigned
no pack; adding a later pack 3 incorrectly selected pack 3. The wrong stamp
reached the explicit Save request.

**Repair.** Start the forward search at `index`. Preserve backward precedence,
held/carried exclusions, capacity checks and no-stamp behavior when no eligible
neighbor exists. Do not change drag movement arithmetic or invent a new pack.

**Proof.** The additive real-browser regression failed against unchanged
production assets with expected pack 2 versus actual null. The repaired
regression covers six scenarios, plus native cancellation, Undo/Redo and saved
payloads, through `assets/tests/fixtures/curation-destination.js`. It extends an
existing mapped case without replacing its original assertions or changing the
156-case inventory.

## Dependency, repository and test review

The official registry review covered 39 distinct direct packages and found six
new stable versions. The three Cargo manifests and lockfiles now use SHA-2
0.11.2; runtime also uses zip 9.8.2 and find-msvc-tools 0.1.16. Worker uses
Workers types 5.20261127.1, Vitest 5.1.2 and Wrangler 4.169.0. The existing sharp
override, TypeScript version and compatibility date are unchanged. Updated
lockfile clean installation, Worker and panel typechecks, all Worker tests,
zero-finding npm audit and Wrangler dry-run packaging pass. No deployment is
performed. See [upstream review](../upstream-refresh.md) and the updated license
inventory.

Weekly Dependabot already covers runtime, both Rust tools, assets, Worker and
GitHub Actions. The README's tag badge already uses the dynamic GitHub tag
endpoint and links directly to this repository's tags. CI already triggers on
every push and PR and records the exact triggering source revision. These
working configurations are not rewritten gratuitously.

All 603 commits reachable from the initial two remote branches were checked
for author and committer identities. Every human identity uses
`Kiaro.Sama.Dev@gmail.com`; authentic Dependabot and GitHub web-flow bot
identities are exempt. New human commits must use that email for both fields.

## Verification record and limits

Local native builds and the 20 ordinary synthetic CLI scenarios succeeded.
The eight new guarded tests compile. The required protected runner could create
namespaces but stopped before test bodies because private-interface enumeration
returned `EPERM`; no admission bypass was attempted.

The local full browser run initially encountered a full scratch filesystem;
that failure was retained and storage was repaired. The subsequent unfiltered
run passed 155/156 cases, including the new regression, but Chromium itself
crashed during the existing explicit-logo unhold partition matrix. The actual
Chrome for Testing version matches Playwright's expected 159.0.8154.6. No memory
cause was proven, no assertion was relaxed, and this local run is not counted
as complete acceptance. Required native and browser execution therefore must
be obtained from the ordinary full CI on the final published commit.

An independent source review found no blocking or important issue in the four
fixes. It does not substitute for the final CI. The PR verification record and
downloadable handoff must identify the final SHA, first-attempt results and any
material correction before stating that acceptance is complete.

## Finite integration acceptance

1. Review and integrate PR #112, then this dependent follow-up under the owner's
   Rules. If direct integration is unsuitable, implement the same verified
   behavior manually and close the superseded PR only after equivalent tests
   pass. The audit agent itself does not merge or close anything.
2. Run unfiltered native tests, format, Clippy, original/supplemental inventory,
   language policy and package smoke tests on Linux and Windows, x64 and arm64.
   Execute all eight new Rust tests, all 156 browser cases and all 244 Worker
   cases. Require the final commit's visible ordinary CI, security and package
   checks; previous-source green checks are insufficient.
3. Preserve every first failure. Fix a reproduced cause before retrying a
   failed code path; do not hide failures with skips, retries, larger budgets,
   fixture changes, dropped identities or altered process guards.
4. Give every PR and secondary branch a reviewed merged/closed/DONE disposition
   according to the owner's Rules, including bugs discovered during integration.
   Keep working in the same session until required acceptance is complete.
5. Remove task-owned temp/cache/build artifacts after retaining necessary
   evidence. Keep operator data/configuration, recovery backups until accepted,
   source provenance, licenses and the immutable baseline fixtures intact.

## Primary references

- [Telegram setWebhook acknowledgment semantics](https://core.telegram.org/bots/api#setwebhook)
- [Telegram sendMessage](https://core.telegram.org/bots/api#sendmessage)
- [CoinGecko's official market response types](https://github.com/coingecko/coingecko-typescript/blob/main/src/resources/coins/markets.ts)
- [Playwright drag and drop](https://playwright.dev/docs/input#drag-and-drop)
- [GitHub reusable workflows](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows)
- [GitHub tag badges](https://shields.io/badges/git-hub-tag)
- [zip changelog](https://github.com/zip-rs/zip2/blob/master/CHANGELOG.md)
- [SHA-2 changelog](https://github.com/RustCrypto/hashes/blob/master/sha2/CHANGELOG.md)
- [Wrangler commands and dry runs](https://developers.cloudflare.com/workers/wrangler/commands/#deploy)
