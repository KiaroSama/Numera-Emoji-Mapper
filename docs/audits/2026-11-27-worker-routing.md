# Worker routing and quiet-fixture recheck

Follow the owner's Rules, hooks, AGENTS instructions and prescribed Spec Kit chain.
This record describes the finite repair after PR #111 was merged. Final acceptance
belongs to the exact published PR head and its verification comment, not to an
earlier source or an owner-initiated rerun.

## Scope and baseline

Baseline: `79cffbd93c78d23eae183867af6f7ae6da223a00`, released as `v3.4.0`.
PR #111 was merged at 2026-11-27 11:47:03 UTC; Issue #110 was completed by the
operator. The subsequent changes add six-orb Worker logs, Pack Links routing,
an authenticated `/log-test` route, optional channel secret synchronization and
an explicit native publisher User-Agent. The prior doctor/catalog/redaction
repairs remain intact and passed 34 ordinary-release regression scenarios.

The retained Worker suite passed 186 cases and typechecking on its first local
execution. An independent 31-case renderer/sink matrix also passed; no additional
logging-renderer defect was demonstrated. These results did not cover the two
newly reproduced routing failures below.

## R25: automatic discussion mirrors feed the log channel

**Priority: P2.** A log-channel post is delivered directly as `channel_post` and,
when the channel has a linked discussion group, as an automatic `message` in
that supergroup. The existing echo guard handled only the direct channel post.
The mirror therefore reached authorization and produced an `access.denied`
warning, which v3.4.0 sends to the same log channel. Both bots can receive the
resulting automatic mirror again. The existing per-isolate 12-message/minute
limiter constrains one instance; this finding is feedback amplification, not a
claim that an unbounded live loop was executed.

Sixteen independently specified combinations reproduced an unwanted send:
general/coin bots, message/edited_message envelopes and four supported numeric,
bare-ID or username spellings. All network calls were stubbed. Negative controls
retain ordinary handling for manual forwards, unrelated channels and unknown
sources.

The correction shares configured-log-channel matching with the direct echo
guard. Before authorization or any sink, it suppresses a discussion mirror only
when `is_automatic_forward === true`, `sender_chat.type === "channel"` and the
source matches the configured log channel through the existing normalizer.
It returns the existing `LOG_ECHO` result. It does not trust message text or
forwarded text, broaden channel authorization, alter rate limiting or change
ordinary user replies. Both Telegram fields are represented in `TgMessage`.
The operator guide describes the direct and automatic-mirror cases.

## R26: authenticated log-test body failures escape the response contract

**Priority: P3.** `/log-test` consumed `request.text()` outside an error handler.
A rejected body read escaped `worker.fetch` instead of returning a controlled
client error. A synthetic rejected read reproduced the failure; a separate
negative control proves invalid bearer authorization prevents body consumption.

The correction catches only body-consumption failure and returns constant
HTTP 400 `bad request`, after bearer validation and before any Telegram, D1 or
logging sink. Existing malformed-JSON/shape, default selection, configuration,
upstream-error and redaction behavior is preserved. It does not echo exception
text or retry the request.

The identical frozen 46-case routing matrix changed from **29 pass / 17 fail**
to **46 pass / 0 fail / 0 skip** on its first corrected execution. All 186
retained cases plus these 46 regressions passed together: **232 pass**, with
typecheck exit 0. No existing case was removed or weakened.

## R27: a supposedly quiet Windows fixture is not strictly quiescent

**Priority: P2, CI reliability and evidence integrity.** The first Windows ARM64
full suite in [run 46005091128, job 140526143206](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/46005091128/job/140526143206)
failed `actual_idle_file_progress_error_and_wall_keep_original_owner_contracts`.
During `FileProgressThenError`, the real Job sample changed from
`[156250, 0, 0, 5, 2]` to `[156250, 156250, 0, 5, 2]`. These fields are user time,
kernel time, read bytes, write bytes and cumulative process count. The full
invocation completed 131 parent harnesses with **1903 pass / 1 fail / 4 ignored**.
An owner-initiated same-source retry passed; that is preserved as a retry, not
described as a repair or a clean first attempt.

The quiet helper published readable `ready` bytes before its `sync_all` and
file-handle close completed, then used `thread::park`, whose contract permits
spurious wakeups. A 400 ms stable sample cannot establish that neither mechanism
will affect later accounting. The observed log proves the kernel-time change;
it does not identify a console process or establish the exact source of that
tick. Cumulative `TotalProcesses == 2` is not evidence of two live children.

The fixture-only correction creates an unnamed, noninheritable, initially
nonsignaled Windows event before readiness. It writes and synchronizes
`ready.pending`, closes the file, then publishes it by same-directory rename.
The Windows leaf uses `WaitForSingleObject(INFINITE)` on that private event, a
non-alertable wait with no normal spurious return. An unexpected return fails
visibly. The common `hold()` and every other fixture mode remain unchanged.

The existing real-owner driver and original test remain unchanged. A new
Windows-only regression executes all three original scenarios in four fresh
owned roots. Every repetition must pass; a failure is never retried or discarded.
The original CPU/IO equality, live-child evidence, idle/wall limits, callback
error, external file writes, fallback absence and verified cleanup all remain.
No process-owner implementation, admission guard, timeout or skip is relaxed.

Exact-source Windows x64/ARM64 CI must establish execution of the original and
new tests. Cross-target compilation is not native execution. The finite four-run
check addresses the observed intermittent risk without claiming statistical
proof for every possible future host condition.

## Verification and integration contract

Use the normal full Linux/Windows x64/ARM64 workflows, all 1516 original test
identities, browser execution, Worker suite, launcher, package and security gates.
Inspect all source Checks and preserved earlier failures, not only the latest
workflow summary. No workflow rerun, alert dismissal or settings change is part
of this repair. There is no dependency or version change; retain the dynamic tag
badge and existing weekly Dependabot coverage.

Human author and committer emails must be `Kiaro.Sama.Dev@gmail.com`; genuine
bot commits are exempt. The initial complete 602-commit history audit found no
unexpected human identity. Do not rewrite clean history or move `v3.4.0`.

The audit agent opens the implementation PR and leaves it open. The
owner-authorized integration agent must review every repository PR,
reconcile required behavior and give each its Rules-compliant merged/closed/DONE
disposition, requiring fresh visible CI on the final integration commit.
Stop optional retesting after the finite acceptance contract passes; reopen only
for a new reproducible counterexample or a relevant source/environment change.

## Primary references

- [Telegram Message fields](https://core.telegram.org/bots/api#message)
- [Telegram discussion-group forwarding](https://core.telegram.org/api/discussion)
- [Rust thread parking contract](https://doc.rust-lang.org/std/thread/fn.park.html)
- [Windows event creation](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createeventw)
- [Windows non-alertable object wait](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)
- [Job accounting field semantics](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information)

All probes use owned synthetic data and stubbed or local services. No real
Telegram/Cloudflare deployment, operator credential or private collection was
used. Private-input exclusions and interactive physical-volume/Explorer checks
remain explicitly separate from automated acceptance.
