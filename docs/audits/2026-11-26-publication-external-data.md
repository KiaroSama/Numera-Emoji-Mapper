**First, comply with the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit chain. Preserve protected test admission, independent fixtures and exact-source CI.**

# Publication audit and external catalog recheck

## Scope and ownership

Reviewed main and authenticated v3.2.0 source:
`e1d051321dbe101990f9b63725e8c76be44eec82`.
Tracking Issue: [#108](https://github.com/KiaroSama/Numera-Emoji-Mapper/issues/108).
Prior PR #107 was already merged by the owner; this work does not reopen its
completed repairs. The audit agent may open the implementation PR and publish
evidence, but must not merge, close records or resolve review threads.

This record connects requirements, clarification, technical plan, regression
tests, implementation and verification. No tracked owner Rules, AGENTS file,
`.specify` workspace or configured hook path was present in this checkout.
It does not claim an unavailable external Spec Kit command was executed. The
integration agent must use the owner's actual Rules and prescribed chain.

Human author and committer email must be `Kiaro.Sama.Dev@gmail.com`; genuine
bots retain the owner's exception. The baseline audit found no unexpected
identity among 591 reachable commits. No history reconstruction was needed.

## R17: the audit and publisher use different state rules

### Reproduction

The authenticated Linux x64 v3.2.0 executable has SHA256
`50f9a3e3ced49dcb102615ddb8c0f52c3173fe124287b3d6115c24d3b883ec51`.
An independently specified 30-case ordinary-CLI matrix found 9 false-clean
results, 3 compatibility false positives and 18 expected results. Five
publisher dry-run controls on identical copied states isolated the disagreement:

| State | Released audit exit | Publisher dry-run exit |
| --- | ---: | ---: |
| Complete valid family | 0 | 0 |
| Unsupported `fmt: "sideways"` | 0 | 4 |
| Wrong stored family | 0 | 4 |
| One recorded key with `live: 0` | 0 | 4 |
| Compatible omitted `logo` | 1 | 0 |

The released audit also accepts invalid/missing indices, missing formats,
excessive live counts, duplicate set names, empty titles and malformed state
list entries. It rejects compatible empty/defaulted states.

`publication_audit.rs` independently validated selected fields while
`resume::collection` already defined the actual publication-state contract.
The new audit test's supposed clean fixture omitted mandatory `fmt` and
`index`, so it did not detect this disagreement.

### Repair and acceptance

Validate and normalize an in-memory clone through `resume::collection`.
Inspect normalized defaults on success. On a semantic error, append
`MALFORMED_STATE` and continue inspecting readable raw members so duplicate,
catalog, publication, upload-flag, custom-ID and set-name diagnostics survive.
Do not persist normalization or invoke the catalog writer/migration path.

Keep exit 0 for clean, 1 for known findings, and 2 for unreadable or
structurally uninspectable prerequisites. A non-object root or present
non-array `sets` is uninspectable. Missing compatible fields are valid;
present invalid values are not. Different formats may each start at index 1,
brand membership consumes a slot, and the existing unattributed live-tail
contract remains valid.

The supplemental test retains its original identity and meaningful anomaly
assertions. Its clean fixture supplies format/index; its malformed-logo
control uses a present invalid value. Added independent cases cover valid
defaults, brand slots, live tails, per-format indices, semantic violations,
coexisting catalog findings, absent files, invalid schema and exact input
bytes. Expectations are not calculated by the validator under test.

The frozen matrix SHA256 is
`3b9eba828d35b85d60702b07942ab5e24ebfc1327ebcd14310020c3812315625`.
The repaired ordinary CLI produced all 30 expected results: 10 clean, 16
findings and 4 uninspectable inputs. Every original state and main database
byte was preserved. The WAL control created the same normal `-wal` and
`-shm` sidecars as the released baseline; this is not a durable-state write
and is not hidden by deleting the baseline evidence.

## R18: an external catalog selects nonconfined codec IPC

### Reproduction and affected paths

The same released executable and two identical PNGs fail with an external
catalog: exit 4, `new=0 dedup=0 failed=2`, and
`codec envelope must stay inside project`. With an internal catalog they
succeed: exit 0, two durable rows, then repeat deduplication of both rows.
The independent comparison passed all 13 diagnostic checks and preserved
the source bytes.

A separate ordinary-application comparison exercised 8 external routes,
their 8 internal controls and 2 direct worker refusals:

| Route | Released external behavior | Correct behavior |
| --- | --- | --- |
| `fetch-pack` | Downloaded item fails; no durable item/identity | One proven item and pack source |
| `fetch-emoji-ids` | Downloaded item fails with confinement warning | One item, ID keyword and seen identity |
| Cold panel preview | HTTP 404, no cache entry | HTTP 200, decoded 52 by 52 WebP in the selected cache |
| Video identity report | Correct video becomes undecodable, exit 4 | Correct key, zero undecodable rows, unchanged catalog/media |
| Collection admission | Fails before fixture create | One simulated create and proven readback |
| Cold live attribution | Refuses after download | Durable proven attribution without simulated mutation |
| Interrupted-create readback | Refuses; intent remains | Proven recovery, attribution and cleared intent |
| Plan addition | Fails before fixture add | One simulated add/readback, prior membership preserved |
| External/linked worker envelope | Both refused | Both refusals remain |

All 145 diagnostic assertions passed in the verified baseline comparison.
The first receipt is also retained: two probe assertions incorrectly expected
PNG storage, while the successful collector controls produced the expected
512 by 512 WebP and exact content key. The corrected assertion verifies that
established artwork representation; it does not accept the external failures.

The frozen route driver SHA256 is
`21bc2c6d2efa00a1d05acb22f1b275bf5cac669d95690af31280712d7ad3a452`.
Its verified released receipt SHA256 is
`85d7d3db44f81ad193b582a8026c6969fb75696acfacb3c0487b4675ad194b2d`.
Only owned synthetic catalogs/media and numeric-loopback API fixtures were
used. Methods, forms, IDs, upload bytes and request sequences were bounded
and checked. No live service or operator credential was used.

### Complete caller-side repair

An absolute external `--data-dir` is valid durable storage, but the callers
used `<data>/tmp` for envelopes while retaining the executable's project
root as the native worker boundary. That boundary is correct; final media
destinations already support valid external locations.

Select a shared validated workspace base before allocating native temporary
work. Retain existing internal `<data>/tmp` layouts. A valid physical
external catalog selects confined project storage instead. Validate original
ancestors before canonicalization, component-wise physical containment and
the chosen temporary subtree. Invalid/linked paths return their error and
must not silently trigger fallback.

The complete production correction includes:

- `local_cli`: move the entire local `Scratch`, with converted inputs and its
  `codecs` child, preserving retained-process/evidence checks.
- `fetch_cli` and `fetch_ids_cli`: use a confined native-codec parent for the
  actual downloaded media/repaint operations.
- `identity_cli`: choose a confined report/migration codec parent; restore
  returns before this path as before.
- `publish_cli` and `plan_apply`: choose a valid initial adapter parent for
  logo preparation, admission and publication. Validate the plan workspace
  before removal/settlement effects. The old failure was not generally
  guaranteed to occur before all remote mutations.
- `entry`: choose confined IPC for cold previews, while keeping cache output
  in the selected external catalog. A cache hit alone is not a positive test.
- `publication`: choose the same validated base for both `resolve` and
  `effects`, including `UploadEffect::mutate`'s internal readback calls.

`Download` must keep `download.dl` and `codec` under one invocation owner.
Its cleanup checks must observe the same codec directory that consumes that
input. Remapping only the inner adapter scratch could erase a live child's
outer input and is not this repair. Keep `Publisher.data` as durable
catalog/state/lock/manifest storage. Preserve generic `config=None` behavior.

Do not widen the adapter's root, alter `with_scratch`, weaken process custody,
remove retained evidence, increase deadlines or change destination/no-clobber
validation. Converter, ticker and the current fixed-root pack gallery paths
already use confined scratch and remain outside the defect.

The supplemental installed-CLI cohort exercises real native codecs, external
and internal data paths, durable media/identity, cold preview/cache reuse,
video reporting and both publication download owners with owned fixture APIs.
Confinement and link refusals remain negative controls. Existing original
test identities and expected pixels/hashes remain unchanged.

### Privacy gate identified during independent review

The initial repair candidate passed functional probes but allocated the new
fallback with ordinary Unix mode `0755`. A traversable installation could
then expose temporary material previously protected by a private external
catalog. This candidate was not published as the repair PR. The correction
must establish private storage atomically and verify safe reuse, refusing
an unsafe existing ancestor without modifying it or deleting evidence.
Windows requires an explicit inheritable restricted ACL; an inaccessible
parent alone does not establish descendant confidentiality. Independent
permission/ACL and linked-path controls accompany the positive workflows.

The combined review also followed retained envelopes out of that working
parent. A failed external preview left structured request JSON at mode
`0644` beneath an ordinary `0755` diagnostic directory. Depending on the
failure point, moving a whole envelope can also move staged binary output.
Successful output finalization separately writes a request/response witness
in installation logs. Both destinations therefore need their own private
boundary before any rename or write; protecting only the active workspace
does not establish that property.

The correction creates external-catalog invocation envelopes and
diagnostic/witness directories privately and exclusively, while preserving collision behavior,
diagnostic discovery, process settlement, retained evidence and destination
publication ordering. On Windows, the original envelope's inherited access
must already be private because a same-volume rename may retain its ACL.
Independent controls inspect the actual retained request and witness files.
Existing ordinary human-readable error logs may still name source files;
the repository's secret-redaction policy remains in force. This change
protects complete codec artifacts and does not promise installation-wide
filename secrecy.

Carry that privacy requirement from the validated external workspace through
the complete invocation. Generic/internal adapters retain their original
filesystem support; this repair must not impose a new persistent-ACL-volume
requirement on every Windows converter, ticker or internal catalog operation.
External work must still fail if its private allocator is unavailable.

## Verification and integration gates

Local release builds, supplemental target compilation, formatting and Clippy
are separate from protected execution. The one authorized local guarded
attempt used `tests/runtime-tests.toml`; it refused before test startup with
`cannot enumerate private interfaces: Operation not permitted (os error 1)`.
No direct test binary or isolation workaround was used. Exact final-head
hosted CI must execute the protected regressions and full suite.

The prior main/tag CI and signed v3.2.0 artifacts were independently checked
as the baseline. Both Windows archives have correct numeric 3.2.0.0 and
textual 3.2.0 resources. Dependency audits found no npm vulnerability, the
existing weekly Dependabot configuration covers Cargo/npm/Actions, and the
README release badge already queries Git tags dynamically. These findings
do not justify unrelated source or workflow churn.

The implementation PR carries the final source SHA, commands, frozen probe
results, actual hosted CI links/counts and explicit platform/private limits.
Before integration, the authorized agent must inspect that final head,
review all open PRs under the owner's Rules, repair any additional concrete
bug encountered, and finish all required integration/dispositions in the
same session. Merge correct PRs when permitted; otherwise repair or manually
integrate their work and close them only after verified completion. Every
PR must be merged or closed with its work DONE according to those Rules.

After final commit/merge/push, the primary CI must appear on the latest
published target commit. Do not use a previous green head as acceptance,
blindly rerun, create empty commits or leave required fixes for another
prompt. Existing signed tags/releases remain immutable; a later release
uses the owner's normal new-version procedure and dynamic tag badge.

## Primary references

- [Cargo configuration and explicit runner paths](https://doc.rust-lang.org/stable/cargo/reference/config.html)
- [Upstream Cargo configuration source](https://github.com/rust-lang/cargo/blob/master/doc/book/src/reference/config.md)
- [SQLite WAL and read-only databases](https://sqlite.org/wal.html)
- [Official Telegram Bot API](https://core.telegram.org/bots/api)
- [Dependabot options](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference)
- [Spec Kit command chain](https://github.github.com/spec-kit/reference/agentic-sdd.html)
- [Windows file security and inheritance](https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights)
- [Security descriptors for new Windows objects](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-descriptors-for-new-objects)
