# Complete remap artifacts and display-name logging re-audit

Follow the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit
chain before integration. The inspected public checkout has no scoped Rules,
AGENTS.md, CLAUDE.md, Spec Kit constitution or configured project hook. Use the
owner's actual environment rules and installed command syntax; do not invent,
replace or bypass them. This audit opens a concrete implementation PR. It does
not authorize its author to merge, close PRs or delete remote branches.

## Baseline and bounded review

Reviewed main: `8aa6f5cae47b981813b7e08ca438900e94f438ae`, release `v3.5.0`.
The owner had merged [PR #114](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/114)
and retired its branch. Only `main` existed remotely and no PR was open when this
pass started. Since the prior accepted fix `660f1b3`, production changes consist
of Worker display-name annotations, the coin keyword refresh and version metadata.

The review rechecked all Worker production modules and both D1 migrations, the
seven curation scripts and their native request/save paths, remap reconciliation,
matching, persistence, lock identities, the refreshed catalog, CI, dependency
manifests, reachable commit identities and the published release. Existing
baseline CI and the actual signed Linux executable were independently inspected.
This is a finite contract review with preserved failure evidence, not a claim
that every possible input or live deployment has been executed.

## Remaining R33: hard links between durable data roles (P1)

**What remained incomplete.** The previous R33 repair checked resolved pathname
uniqueness and hard links to lock storage. It did not compare physical identities
between state, cache, canonical output, candidates and the implicit backup. This
is missing coverage in the earlier repair, not a new regression caused by the
owner's merge. The prior audit's completion statement did not establish this
additional invariant.

**Actual failure.** Make `map.prebroken.json` a hard link to either the state JSON
or cache JSON under a different pathname. Invoke a calibrated
`coin-remap-ids --apply --max-distance 1` with one exact local image and a complete
provider set. The released executable returns `0`, makes three provider requests
and replaces the canonical map. Its supposed recovery backup contains state/cache
data rather than the old canonical bytes. Valid JSON alone cannot establish a
backup's role. Existing direct-path and lock-storage controls already refuse.

**Implemented repair.** `runtime/src/coin_remap.rs` introduces explicit
`DurableRole` values and groups existing files by the shared file-identity helper.
Unix compares device/inode; Windows compares volume and the complete 128-bit file
ID. Every previous role in an identity group is retained and checked. Only the
symmetric `Output`/`Backup` hard-link pair is permitted; atomic output replacement
preserves that old inode as the backup. Equal pathnames remain forbidden even for
that pair. A third state/cache/candidate alias must still refuse.

The shared helper checks all five roles before lease acquisition, network access
or durable writes. Direct persistence checks its three known roles through the
same helper. Canonical-prefix, alternate spelling, symlink, lock-storage and
backup-object checks remain intact. Identity-query failures fail closed. There
is no array-position exception, blanket hard-link ban, backup deletion or cache
reset.

**Closure criterion.** All ten unordered role pairs are represented: nine refuse
in both apply and dry-run modes, and output/backup succeeds. All three possible
third-role additions refuse. Direct persistence covers canonical and candidate
destinations. Refusals assert zero provider requests, no created lease directory
and exact preservation of every role's initial bytes. Compare against the initial
snapshot: an invalid input backup may already contain unrelated JSON.

## R36: presentation transforms precede display-name redaction (P2)

**Actual failure.** The new missing-username annotation added after PR #114
normalized whitespace in `handle.userFields`, then shortened the display name to
48 UTF-16 code units in `logging.formatLine`, before final log sanitization. The
existing redactor matches configured credential literals. Shortening a long
literal, cutting across its boundary or normalizing a tab/edge whitespace removes
the match. Both bot routes can consequently log a recognizable fragment or
normalized value to console and the log channel.

The demonstration uses only synthetic bindings and local HTTP fixtures. It
requires credential material already present in user profile metadata; it does
not extract an unknown environment value. An access-denied private-chat event
exercises the path without administrator privileges. These events retain
`store: false`, so this is not a newly introduced D1-write path.

**Implemented repair.** Preserve complete nonblank raw name parts in
`worker/src/handle.ts`. In `worker/src/logging.ts`, call
`sanitise(rawFields.user_name, env)` before normalization, trimming and the
existing surrogate-safe length cap. Retain final sink sanitization. Ordinary
field values already follow this ordering; valid username precedence, reused
numeric-ID annotations, HTML escaping, log size limits, throttling, authorization
and mirror suppression remain unchanged.

**Closure criterion.** The additive `worker/test/log-name-redaction.test.ts`
covers all five credential bindings, long/boundary-crossing strings, tab/edge
whitespace and joined name parts, Unicode, HTML, valid/reused username handles,
blank names, both webhook routes and both log sinks. It executes real handlers
and the real formatter; only the external Bot API transport is substituted.
The 43 new cases initially produced 39 expected failures and four passing
presentation controls. All 43 pass after the repair, alongside all 249 old cases.

Actual workerd runs use the unchanged production entrypoint and compatibility
date, both real local D1 migrations and loopback Telegram transport. The long-name
and whitespace fixtures each improve from 8/10 to 10/10 expected outcomes.
Authenticated publication and ordinary log-test masking establish that the
synthetic configuration itself is usable. No deployment, real Telegram message
or remote D1 mutation is part of this audit.

## R37: unreadable local artwork permits an incomplete apply (P1)

**Actual failure.** Start with a canonical map containing `aaa` and `broken`,
one valid `aaa.png` and one corrupt local PNG. With a complete live set and a
calibrated apply, the release emits a warning, returns `0`, replaces the canonical
map with only `aaa` and produces no candidate artifact. The analogous remote
image failure correctly blocks publication. `local()` discarded decoder errors
after logging, while the policy received only retained remote errors.

**Implemented repair.** Local signature collection returns a named
`LocalSignatures` result containing tickers, signatures and the exact failed-image
count. The existing policy receives local failures plus retained remote failures.
An otherwise matchable incomplete apply now returns `3`, publishes candidates
for review and preserves exact canonical and existing backup bytes. Logs expose
`local_failed` and total `failed_images`. Hard read failures continue to return
errors; they are not converted to successful partial input.

**Closure criterion.** Cover one/two corrupt local files, absent/preexisting
backup, combined local/remote failures and valid backup hard links. Preserve
dry-run `0` without output/candidate/backup writes, unusable-input `4`, remote
incompleteness `3`, and complete unique success `0` with the old canonical backup.
Removing bad files, deleting the cache, weakening completeness or catching a
panic is not an acceptable replacement for this repair.

## Verification and preserved first failures

The signed baseline Linux executable has SHA256
`3087c2912936c4339ddfb0e21048f892b839a22ee61d883d756bbb533a78303c`.
The patched local release executable has SHA256
`9b52eecb59d5380654ff2e6f44a64c1b09710030c26f19a0b58bb0f8805f6c25`.
The ordinary 37-case CLI matrix ran once on that patched executable and passed
all scenarios. Its explicit receipt UTC start is `2026-11-28T13:43:33.289085Z`.

| Check | Observed result |
| --- | --- |
| Original asserted remap reproduction | 7 controls pass; 3 expected failures |
| Patched ordinary remap CLI | 37/37; real hard links, local files and HTTP fixtures |
| New guarded native target | Compiled; local admission refused before any test body |
| Existing Worker baseline | 249/249 across 13 files |
| New Worker regression on unchanged code | 39 expected failures; 4 controls pass |
| Complete patched Worker suite | 292/292 across 14 files |
| Formatter probe | 2/4 before; 4/4 after |
| Actual workerd name fixtures | 8/10 before and 10/10 after, for each of 2 fixtures |
| Worker and panel typechecks; Wrangler dry-run | Pass |
| Rust format and strict all-target Clippy | Pass for runtime, CI tool and inventory tool |
| Original/supplemental source inventory | 1516 original IDs; 1956 wired Rust functions; 0 errors |
| Complete browser/source identity inventory | 156 identities; 0 errors |
| Language policy | 1160 candidate files; 0 violations |
| Independent candidate source/test/CI review | No concrete blocker |
| Fresh local full browser suite | 154/156 pass; 2 unchanged-budget timeouts |
| Baseline exact-main CI browser artifact | 156/156; no skipped/flaky/unexpected cases |
| Refreshed catalog | All 5960 rows validated; 8/8 ordinary CLI controls |
| Published release integrity | 16 asset digest/size checks; 4 archive signatures and tamper controls |

The six native tests cover 18 forbidden pair iterations, six third-role
iterations, six direct-persistence refusal iterations, four direct-persistence
positive iterations, four local-failure/backup iterations and five completion
controls. They retain the protected test admission module and independent PNG
fixture. The early four-platform CI boundary step now explicitly includes
recovery, paths and completeness targets. The full unfiltered native suite and
existing original identities, assertions, fixtures, guards and budgets remain.

The new native target compiled successfully in 3m05s. Its ordinary protected
invocation then returned `2` because this host could not enumerate private
network interfaces (`EPERM`); the runner explicitly reported that no test
started. No direct-harness bypass, unchanged retry or admission change was used.
The 37 ordinary application scenarios are separate practical evidence. Actual
execution of all six guarded tests belongs to the final four-platform CI gate.

The inventory tool's first executable compilation exhausted this shared host's
disk space. Removing only this audit's completed runtime-debug and CI-tool build
caches restored 928 MiB; compilation then succeeded and all inventory checks
passed. A separate discovery-log parser initially combined npm's proxy warning
on stderr with valid Playwright JSON. The already successful discovery output
was parsed separately without rerunning tests. Neither harness problem required
a project-source or policy change; the original failure logs are retained.

The local browser run used the matching official Chromium revision after the
normal CDN download returned 403. It ran once with one worker, zero retries and
the original 60000 ms limits. Capacity and exhaustive Unhold composites timed
out; no assertion mismatch, browser crash or server exit occurred. Earlier
substeps passed, but interrupted later substeps are not counted as locally
verified. Host contention is plausible, not established. No native compilation
overlapped that browser run. The exact-main CI artifact independently completed
all 156 cases in 94.570 seconds. Final candidate CI must provide its own fresh
156-case result; baseline success does not convert the local run into a pass.

The native-workerd harness also retains its initial admission failures. Current
Miniflare uses a newer configuration schema. Its documented conversion adapter
and supported inline bundle option ran the unchanged production application;
no production check was disabled to repair the local harness.

## Catalog, release and maintenance

The catalog is sorted and case-insensitively unique, with required headers,
nonempty keywords and safe expected artwork paths. Compared with the previous
fix it has 145 additions, 12 removals and 305 changed rows. The release's embedded
seed exactly matches the tracked CSV. Full-catalog, fallback, new-alias,
ambiguous-symbol, same-ID alias, chain alias, missing-header and invalid-UTF8
ordinary CLI controls all pass.

All 16 published assets match their GitHub digest and size metadata. The pinned
public key/certificate agree with `docs/signing/publisher.json`; signed checksum
inventory and detached signatures validate. One-byte changes to each of the
four archives are rejected. Every archived file matches its internal manifest
(32 files per Linux archive, 30 per Windows archive), with version `3.5.0`, the
reviewed revision and correct platform metadata. This is independent signature
and package inspection, not a claim of local Windows execution. The earlier
tag/package-version mismatch at `3fc5c2a` was already repaired by `8aa6f5c` and
is not a new defect.

The registry check at `2026-11-28T11:24:46Z` examined 39 distinct direct packages
and the existing sharp override, with zero lookup errors. It found three newer
stable versions, now adopted: zip `9.9.0`, libwebp-sys `0.14.5`, Workers types
`5.20261128.1`. Targeted lock changes preserve other package versions and feature
selections. Byte comparisons preserve complete license text while notice
references are refreshed. Compatibility details and primary research links are
in [upstream-refresh.md](../upstream-refresh.md).

Rust `1.100.1` and used Actions release lines remain current. Weekly Dependabot
already covers all three Cargo manifests, both npm manifests and Actions. The
README badge already derives its version from GitHub tags and links to this
repository's tags. No static badge, extra updater, arbitrary transitive major
upgrade or new release tag is introduced.

## Integration gate and completion rules

All 612 commits reachable at audit start have the required human author and
committer identity, `Kiaro.Sama.Dev@gmail.com`, with genuine bot identities
exempted. Keep that identity for every new human commit. If a later history
inspection finds a different human email, apply the owner's controlled rewrite
procedure, preserve concurrent work and bot provenance, and repeat CI on the
resulting heads. Do not toggle GitHub email privacy settings.

The PR's final verification comment must identify the actual published commit
and its ordinary push/PR runs, including first failures and attempt numbers.
All four native platform suites, complete browser and Worker suites, tooling,
packaging and security workflows must be visible on that latest commit. No
unchanged retry, timeout extension, filtered replacement for acceptance, ignored
new failure or skipped safeguard can establish completion. Any new corrective
commit restarts the applicable final-head gate.

The owner-authorized integration agent must follow the actual Rules and Spec
Kit chain, review the existing implementation PR and satisfy every contract
above in the same session. Resolve any concrete defect found while integrating.
Enumerate every PR and secondary branch and record its final Rules-compliant
disposition. Merge reviewed, green changes; manually integrate/fix nonmergeable
work and close its superseded PR only after proving equivalent code and tests
are present. Finish every PR as DONE through verified merge or justified closure.
Retire secondary branches only after preserving their unique required work.

Prefer the existing branch. The audit branch is necessary solely to satisfy the
owner's explicit PR-review requirement. Commit and push the completed work, then
require visible primary CI on the resulting default-branch head. Remove owned
temporary fixtures, caches, dependency installations, generated bundles and
obsolete duplicates after preserving final evidence. Retain operator data,
valid recovery backups, licensed assets and immutable original test fixtures.
Do not erase useful audit provenance to make the workspace appear clean.

Optional improvements for owner/agent assessment are a pinned workerd CI smoke
using local D1/HTTP fixtures and clearer operator-facing completeness diagnostics
using the new counters. These are proposals, not additional unimplemented defect
claims. Do not introduce GPU acceleration into filesystem/redaction work without
an actual relevant measured bottleneck.

Primary references: [Spec Kit](https://github.com/github/spec-kit),
[Telegram User fields](https://core.telegram.org/bots/api#user),
[Miniflare testing](https://developers.cloudflare.com/workers/testing/miniflare/core/standards/),
[Rust 1.100.1](https://github.com/rust-lang/rust/releases/tag/1.100.1),
[zip 9.9.0](https://github.com/zip-rs/zip2/releases/tag/v9.9.0),
[libwebp source comparison](https://github.com/NoXF/libwebp-sys/compare/11b096bd65f0d51326673ae3e2e5af2dae02368f...13c6aa7589a151f50d20bff705f698eb1f0a4ee5),
[Workers types registry](https://registry.npmjs.org/%40cloudflare%2Fworkers-types/latest),
[OpenSSL signature verification](https://docs.openssl.org/3.6/man1/openssl-dgst/).
