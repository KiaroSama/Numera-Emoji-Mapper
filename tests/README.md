# Tests

Build the required native module first with the project interpreter:
`python scripts/build_native.py` (Rust 1.99+ and the platform linker required).
CI builds a release wheel and installed native executable before every Python job
that uses the catalog/panel. Native command parity also requires the debug executable;
CI builds it explicitly. See [native backend status](../docs/native-backend.md).
`test_native_similarity.py` compares exact indices against source-generated
fixtures, checks fixture drift, malformed inputs and missing-native failures.
The Python oracle is test-only, never a production fallback. Regenerate fixtures
with `python -m tests._similarity_oracle`; changes require reviewing source parity.
Rust formatting, clippy, unit checks and a repeated benchmark run once in the
3.11 build job; native parity also runs on Windows and Python 3.12/3.14.

The expanded first-party Rust migration has a separate, manually dispatched
checkpoint for additive codec, coin, storage, tooling and historical-source seams,
with pinned [source fixtures](fixtures/native/README.md). Workflow selection is
not execution evidence: the new historical replay, runner and audit-cancellation
checks remain uncompiled and unexecuted. The checkpoint does not replace the main
suite, four-platform acceptance or source-retirement gates. No production codec
adapter is switched by it.

The native launcher candidates have scripted workflow tests in `tooling_menu`,
exit/cancellation and retained-check planning in `tooling_launch`, and actual
copied-binary doctor/menu/shared-input scenarios in the existing `tooling_runtime`
tracer. They reuse its one disposable venv and built artifacts; the echo child
proves stdin ownership only, not bot behavior. Scripted capture admission is covered
in `tooling_process`; `tooling_install` adds offline argument/environment policy,
extension location and actual CLI refusal before activation. The public installer
remains closed pending signing/identity/logo/rollback admission; no successful install
is claimed. All these additions remain uncompiled and unexecuted.
See [launcher migration limits](../docs/native-launcher.md). The same real-venv
tracer now selects three hash-pinned original hermetic-guard methods through the
actual native `check` command with compile/lint skipped, followed by a nonexistent
selection that must fail. This remains retained-source orchestration, not native
network isolation or test retirement. Package CLI tests retain external stdlib
argparse grammar; the synthetic archive test verifies every original payload hash,
CRC, file mode and exact27-member inventory. All new runtime evidence is pending.

The `wheel_package` Rust suite checks the actual newest built wheel, preserving
native-extension presence, private-content exclusion and complete CRC/header
validation. Missing wheels fail rather than skip. It belongs to the full
wheel-building lanes, not the additive no-wheel checkpoint. An in-memory
corruption matrix checks that member names alone cannot satisfy integrity.
The original Python test remains; the new Rust tests are uncompiled and unrun.

The shared pinned Git corpus also drives native-owned regeneration of the 24
original manifest, two environment and four confinement observations. Source and
input-recipe hashes are admitted before runtime access; caller-supplied expected
outputs are not trusted. The existing bounded historical worker executes original
functions with private host facts and real disposable files, never native policy
as its own oracle. A second fixed recipe regenerates the original synthetic
package archive: all 26 inventory hashes, 27 ZIP members, CRCs, input preservation
and checksum are checked with explicit same-platform differences. A third recipe
regenerates the original sandbox environment scrub and five parser/argument cases
using a synthetic environment and real template, without creating a clone or
starting a panel. None uses caller-supplied expected values. Failures retain
evidence; all three generators remain uncompiled and unrun.

Collection resume migration also retains 19 original `StateFileContract`
assertion methods in a fixed three-source replay. Its synthetic two-key setup
replaces the original image catalog; catalog writer ownership is explicitly not
proved by that replay. A private replacement fault exercises the original atomic
writer without patching the worker's real `os` module. Native `intent_parity`
checks the corresponding schema variants, while the existing copied-command
tracer checks corrupt state/plan refusal, preserved bytes and zero API requests.
Two full publisher methods remain excluded from the historical state-only replay.
The command's actual JSON reader also has responsibility-local absent/default,
existing-null and malformed/nonregular-file assertions; the production writer's
last-replace failure must retain the original `sent` list and unrelated staging.
All new source and native execution evidence remains pending.

Live reconciliation, identity, uniqueness, the plan applier and a transport subset
retain 145 unchanged original assertion methods through one fixed 34-source replay
of the admitted Git corpus. The applier contributes all 20 original methods; five
fresh-upload methods retain the real publisher CLI, downloaded-content proof and
final identity readback. The 33 selected transport methods retain token masking,
lookup/create name-lock policy, upload-content proof, measured post-retry size,
tri-state live counts, creation-only repainting and malformed-body retry/proof
routing. Diagnostic assertions observe real original warnings through a private
registered logger; its registration, handlers, namespace and stdout are restored
on success or failure. Thirty-five original ticker-bookkeeping, skipped-image
resume, capacity, unresolved-mutation and announcer methods retain the original builder,
argparse and atomic state writer with private configuration, clock and argv
ports. Its family-lock and announcement ports are no-op; exact unknown/foreign/
extra-content refusal assertions remain distinct from native wire evidence. The replay keeps the historical SQLite catalog, opaque-cell Pillow
image recipe, fake live family, publisher, removal recovery, reorder, state
validator, atomic writer and OS family lock. The expected 127 temporary contexts share one delayed teardown
ledger, including all selected original PNG setup and cleanup methods. Private
synthetic environment, HTTP and sleep ports avoid real credentials, requests and
long waits; exact client sleep and print patch targets are redirected. Manifest
output and native prerequisite/catalog ownership ports are explicitly no-op.
Source success would not establish native locking, cancellation, transport or
exception equivalence. The existing 15-second wall / 8-second idle process bounds
and 512 KiB envelope limit are unchanged. Every assertion, teardown acknowledgment
and bounded physical inventory must succeed before deletion; failures retain
evidence. This extended replay remains uncompiled and unrun.

The native `reconcile_commit` cohort exercises the real publisher/catalog against
scripted loopback reads, including actual downloaded pinned images through the
bounded native codec. It covers unknown/missing/drift, prior CID preservation,
reuploaded content, foreign tails, unreadable rivals and stopping at the first
attributable item behind a foreign one. The previous partial-commit regression
remains: an earlier identity-proven upload survives a later duplicate refusal.
The fresh-upload tracer adds real native create/add/readback, final reorder or
foreign replacement, and answered adds with zero or two new identities. Failed
post-answer proof stops before eager tail recovery and keeps the unresolved
intent; previously verified CIDs survive. Two additional real-WebM variants require native-worker
video admission, successful create/add/readback/manifest, and rejection of a one-frame change whose
10 fps sampled key is identical. The full native timeline, not that key, must prove the upload;
a failed second proof keeps its intent and the first committed CID without retry or attribution.
Exact forms, uploaded bytes, saved membership and reopened catalog mappings are asserted. Smaller source-pinned
rasters replace the original generated images here, so this is not remote
Telegram dimensions-validation evidence. The same sequential process-owning test
also contains copied `build-pack` commands with real codec readbacks.
A retried add following foreign content must book the measured size and persist
`expected_before` values `1, 1, 3`; unknown/foreign final evidence must stop before
the next item. Clean adds retain one snapshot each, without post-add reads.
Skipped-image recovery checks legacy intents, our content behind foreign content,
same-count replacement, unexplained drift, corrupt state and durable create/add
intents. Integrity refusals return 4; unknown live reads remain retryable at 3.
In-run ambiguous creates require exactly one content-proven sticker; unknown,
foreign, missing, unreadable or extra evidence keeps the original intent and
stops before another upload or notification. Later-set recovery targets set two,
not the previous set, and only definite Missing permits a fresh create. Deleted,
shrunken and foreign-family state refuses without changing its bytes. A missing
result in an answered success envelope drives the native ambiguous-call cases;
this adapts the original fake exception, not a real remote timeout.
Five dry runs check default/valid capacity and invalid zero/400/negative start
without creating state or making requests. These use the transitional retained-codec fixture, not a Python-free package.
Two announcer commands also require exact authenticated Worker JSON: the coin
token selects `coin`, every other token selects `general`, the upload is durable
before the send, and `sent` changes only after its successful response. The same
finite fixture owns both API and Worker requests; no live service is contacted.
Six original collection-publication scenarios share that copied executable:
normal and branded publication plus repeat, content-proven lost-response adoption,
foreign restart refusal, explicit earlier-pack append and explicit new-pack creation.
They retain original public artwork, escaped manifest keywords, separate full/final
notification milestones, exact mutation snapshots and unchanged other-pack IDs.
Malformed JSON adapts the original socket-close fault; circle/coin conversion setup
is not independent encoding-golden evidence. Those nine invocations remain unrun.
Ten additional refusal commands distinguish integrity exit 4 from unknown exit 3,
including older recorded packs, explicit missing/full targets and interrupted adds.
Four two-command progress scenarios cover clean repeat, definite all/partial failure,
permanent refusal and the next clean notification. Exactly one shared command log
must contain one key/set/slot/progress line per confirmed upload; retained media logs
are not counted. Definite refusals remain pending or permanently skipped as appropriate;
unknown/unexaminable transport and answered-but-unproven confirmation retain their
intent and stop. An ambiguous add may settle only after successful live reconciliation;
its proven key is never resent. Restart, occupied-name and foreign-tail tracers check
source/uploaded FUID retention, one correct CID per key and a fresh pack instead of
appending behind proven foreign artwork.
Dry-run validation uses 200 real catalog rows to test logo capacity, selection/publication
filtering, naming, invalid arguments and the stable family lease. Shared immutable
artwork replaces 200 generated files only for this counting seam. Every dry-run command
has its own finite no-request server, so a long validation batch cannot silently lose
its request observer. No plan/state is written; the real-bot name-length refusal allows
only `getMe`. Nine preflight commands check accepted/refused/unknown/partial/missing/precedence/
all-category/empty/filtered queues with exact multipart bytes and no publication or plan/state write.
The separate eleven-source preflight replay keeps all eleven original outcome/summary methods,
eight delayed temporary contexts and 26 zero-delay pacing calls. Its five legacy report observations
feed the existing exact-output command comparisons; source identity mutations refuse before execution.
These counts are authored expectations, not runtime results. A real mixed-family
command skips both transparent and one-visible-pixel PNGs without counting a failure.
A separate actual-command assertion uses the original real 30-frame transparent VP9 recipe:
only `getMe`, exit 0, a skipped key and no set are permitted. The matching fade-in recipe must
pass strict native-worker usability. Encoding uses the shared bounded capture owner, not a
synthetic decoder; publication shares the existing alpha-preserving VP8/VP9 decoder selector.
These real-codec assertions remain uncompiled and unrun.
Branded/plain manifests assert the actual pack total and logo-adjusted row numbers.
A separate fixed 37-source selector now authors 81 unchanged original CLI/dedup/closed-set/
item-format/brand/notification/order, six blank-video and five real-content retry methods,
including original notification, manifest and preflight functions plus scoped `assertLogs`.
It expects 79 delayed temporary contexts; these are requirements, not observed execution.
Publication admission is separate from strict codec validation: complete-frame alpha samples
reject only proven blank video, while a settled failed probe leaves the upload to decide.
Custody, capture-admission and cleanup errors still stop the operation. The existing controlled-tool
cohort keeps six strict codec cases and sixteen publication cases through the actual public publisher.
Blank/noise and selected-alpha decoding skip without HTTP; settled metadata/probe/capability failures
reach one explicitly refused video create. Oversized decode, metadata or capability output stops
without a mutation. Exact observed argv requires the alpha decoder before its input and no decode
when metadata or capability is unavailable. Child death and empty scratch are required; source mocks
prove no process lifetime. Native-worker IPC separately checks static and animated admission.
Animated media is not revalidated at publication. The copied-command brand fixture also requires
static-logo/animated/animated upload order, original dummy-TGS bytes, content readback and shifted
CIDs; this synthetic service is not proof Telegram accepts invalid animation. All new paths remain
uncompiled and unrun. Retained static-worker open-error compatibility is authored through the
shared Rust binding; its actual execution and complete error/lifetime parity remain cutover gates.
Seven notification/routing fixtures inspect full versus initial milestones, real HTTP 503
without automatic retry, subsequent explicit retry, direct note/previews and one 30-pack
Worker call. A reverse-panel-order command keeps stored plan membership while uploads/CIDs
follow current catalog order. A logo-plus-two-item command checks both CID offsets.
Five public mutation variants use actual bounded native-codec downloaded-content comparisons,
not injected matcher answers; the original legacy blind-retry-without-snapshot case remains
unmapped because it conflicts with the current no-blind-retry safety contract. It shares the same admitted
Git corpus, AST selector, bounded historical process owner and delayed teardown ledger;
its request stays separate from the nearly full reconciliation envelope. All new command
and original replay assertions are uncompiled and unrun. Complete transport-failure,
remaining brand/format and platform equivalence remain pending. Occupied-name drift has
two command assertions: one durable container survives the refused reconciliation, and
a later invocation remains loadable without recreating or duplicating the set record.
Seven capacity/follow-up command invocations additionally require a successful logo
create to survive a refused artwork add: another item uses that same pack, a transient
refusal retries only on the next invocation, and a permanent refusal stays skipped.
A definite `STICKERS_TOO_MUCH` on an existing pack moves the item to one fresh pack;
`--into-pack` instead refuses without changing the requested target. An occupied-name
adoption that fills its pack must record the full and final milestones independently.
These transitions share the existing confirmation/accounting path; transport uncertainty
never enters the definite-full rollover route. Two more command scenarios check one title
sequence across formats and a full notification using a curated recorded title rather than
the current CLI title. The seeded video record proves sequencing, not video decoding.
All remain uncompiled and unrun.
Each command has 25-second wall / 8-second idle bounds and the existing
Windows/Linux descendant owner; Darwin custody remains unsupported.
All new execution remains pending. The `reconcile_identity` cohort adds real fixed-colour
collision/reencode/rival comparisons and durable resolver refusals. Its public
policy inputs replace private original SQL/mock seams; a disposable missing-column
fault drives the actual resolver's candidate-enumeration refusal.

The existing `telegram_transport` suite also sends all three original non-object
JSON bodies through the actual HTTP parser: two requests per read refusal, one
add plus one proof callback without resending. Its isolated children retain the
12-second wall / 5-second idle bounds and failure diagnostics. `telegram_tests`
uses the shared post/sleep effect seam for requested waits, explicit proof and
no-blind-retry policies; that is not actual sleeping or content-proof evidence.
Public mutation scenarios also assert exact multipart forms and bytes, no resend
on unknown/proven outcomes, measured size only with our content, and repaint flag
presence versus omission. Their injected content comparisons deliberately match
the original test fakes; real decoding has separate codec/publication cohorts.
Native exported-class reads check exists/missing/unknown without collapsing an
unknown to zero. A real UTF-8 file assertion covers shared retry/flood/name-lock
and verified-no-resend warnings, token redaction and one run's warning counts.
CLI clients attach the existing run log rather than opening a second file;
retained extension/announcement callers remain a separate diagnostic boundary.
All additions remain uncompiled and unrun.

Downloaded verification/attribution bytes now share an exclusive private
invocation directory and file, with the codec scoped inside that directory.
Cleanup uncertainty retains bytes and refuses before durable attribution; normal
cleanup errors are propagated. `publication_scratch` observes two held public
calls at a time, actual downloaded bytes, Unix private mode, error/unwind cleanup,
caller sentinel preservation and controlled unresolved-custody retention. Its
codec is a diagnostic seam, not real decoder evidence. All these source/native
additions remain uncompiled and unrun.

[Coin migration test boundaries](coin-migration.md) documents the keyword,
argument, HTTP, cache, provider, remap, replacement and rebuild assertion cohorts.
They reuse the same corpus/worker, finite loopback observer and bounded command
capture. Native/source additions remain uncompiled and unrun; explicit exclusions,
wire/error adaptations and process-death limits are recorded there.

Operator configuration migration retains all eight original assertion methods
through three fixed sources, a private environment and the same historical worker.
Native `operator_config` checks absent versus explicitly empty branding, normalized
unique bot names, configured/relative logo paths, disable and keyword defaults.
Five copied-command dry runs start with disposable empty catalogs: refusals must
preserve every catalog-directory byte and success must publish no state. These
checks are uncompiled and unrun. The native single-key diagnostic and project-root
CLI override remain adaptations, not Python exception/variadic API equivalence.

Ordering migration retains 13 original methods through a fixed four-source
replay with private patchable globals and the original state validator/writer.
Native `sync_order` covers pinned-logo planning, a 30-item reversal, idempotence,
actual catalog panel order, loopback position forms, stale-record repair,
report-only preservation and refusal without ID reassignment. Five copied-command
cases check whole-family/repeated pack selection, unknown-pack refusal and an
applied record repair that leaves other sets unchanged. Two further native-backed
command invocations perform one real scripted move after the pinned logo, then
repeat with no second mutation; saved order, empty stdout and every catalog ID
must survive. Original fake catalog,
refusal sentinel and no-op ownership ports are disclosed separately from native
transport/storage evidence. All new source/native tests are uncompiled and unrun;
no live set is contacted or reordered.

Archive migration retains 14 original methods through seven fixed sources,
real original SQLite, path helpers, file moves and pure Markdown rendering.
Native `archive_state` checks capacity/freshness without writes, slot-one logo
identity, unidentified later slots and the original partial-move guarantee:
a missing second source must not undo the first file's committed catalog path.
Synthetic bytes establish file-transfer behavior only, not image validity.
Historical no-op ownership and native warning/path refinements remain explicit;
all new source/native tests are uncompiled and unrun.

The native-backed archive/export lifecycle also has one Rust `archive_commands`
tracer sharing one copied executable. Eight archive calls check partial per-file
commit, repaired-source recovery, zero-move repeat, conflicting final-file refusal
and missing configuration. Three export calls check ordered CRC-validated members,
missing-slot preservation of the completed ZIP and stale-roster refusal. Every
command has wall/idle limits and uses only disposable data; exact scripted
loopback requests forbid mutations. These assertions remain uncompiled and unrun.

ZIP export migration retains four original methods through nine fixed sources
and actual historical Pillow/ZIP operations. Native `pack_export` uses a real
roster fingerprint and read-only SQLite projection: number/name selection must
keep slot order, decoded member bytes, manifest IDs and every source unchanged.
Stale rosters, missing slots and unknown packs must leave no partial ZIP; source
aliases refuse and caller-owned staging bytes survive. Compressed ZIP byte
identity is not claimed. All new source/native checks are uncompiled and unrun.

Offline status migration retains four unchanged original methods, including the
real read-only SQLite count, through four fixed sources and a private environment.
Native `status` authors ten actual copied-command scenarios for current/stale
roster/archive, unpublished counts, unknown configuration, unreadable/missing
catalog and usage refusal. Catalog and roster directory membership/bytes must stay
identical; per-run logs are checked separately. All new checks are uncompiled and
unrun; historical collaborator mocks do not establish native freshness behavior.

Roster and report migration shares eighteen fixed sources, sixteen delayed
original temporary contexts and one historical worker. It retains all 24 original
roster methods, private patchable modules, real SQLite/files and the original writer and
HTML renderer. Original PNG/VP9 builders, deterministic TGS generation and Lottie
renderers are retained; FFmpeg uses the shared bounded process owner inside an
outer session. Missing tools fail rather than skip. Native `roster_state` covers
live/source/retired IDs, the coin logo exemption, freshness, safe machine-readable
HTML and untouched-family lookups. Its separate real static/Lottie/video tracer
checks encoded output hashes, cache bytes and modification times against pinned
independent fixtures. `roster_commands` reuses one copied executable for nine
original native-backed lifecycle commands and three coin refresh/incomplete-read/
usage cases, with exact loopback requests and bounded process ownership. Untouched reports
and input files must remain byte-identical, and their old freshness signatures
must survive. All new checks are uncompiled and unrun; markup checks are not
browser interaction or rendering-performance evidence.

Plan-status migration shares the existing status replay: nine exact sources,
four unchanged status methods plus five unchanged plan-report methods, and three
delayed temporary contexts in one historical worker. Original plan validation and
report functions remain intact. Native `plan_steps_parity` adds public summary and
four copied-command checks for absent plans, retired IDs, capacity and offline/live
reports; real SQLite/data membership and bytes must remain unchanged. Two exact
read-only API requests are allowed. Independent source rendering uses literal inputs
from the original native CLI test. All new checks remain uncompiled and unrun.

Native plan-removal migration authors budgeted deletion, unknown-live-state intent
retention, flood-wait/usage exits, four dry-run layouts and five structural refusals
through one copied binary and thirteen bounded commands. Family-scoped unpublish
and direct applied/not-applied/unknown/flood settlement assertions reopen the real
catalog. They do not replace complete crash/retry publisher verification. The
scripted server reads the durable journal before answering
a deletion; no real Telegram write occurs. A separate full cross-pack tracer authors
two identical actual commands with the original three baseline-pinned artworks,
exact multipart upload bytes, downloaded content proof, new/retired IDs, reorder,
untouched third-pack state and no repeated mutation. It reuses the retained codec
closure while production still selects it; this is not Python-free package proof.
Existing descendant custody bounds Windows/Linux commands; unsupported Darwin is
not counted as parity. All new command checks are uncompiled and unrun.

Shared report migration adds native `pack_reports` assertions for the original
hostile cell, empty dash, CRLF/control-line splitting, right alignment and complete
keywords. All four real native writers retain their original column headers and
escaped labels; the publisher writes through a disposable actual catalog with an
independently pinned PNG. Existing original row generation and registered-binding
replay are reused, not replaced. All seven unchanged original report methods also
run inside the existing roster replay with their original writers/catalog/PNG
builder; no-op prerequisite/lease and one exact single-insert identity port are
explicit, not locking/dedup proof. All new checks remain uncompiled and unrun.

Bot migration adds 26 original pure assertion methods through the same pinned
source corpus and historical worker. Native tests retain entity order, markup
refusal, rich/plain alignment and complete bounded copy-button coverage. The
native typed-list parser reuses the frozen decimal/whitespace policy and refuses
leading/trailing separators like the original. Private span/keyboard checks are
adapted to public escape/payload seams. A separate fixed four-source replay
retains the remaining 18 typed-transport, access, polling-wiring and cursor
assertions, with private patchable globals/environment and the original atomic
writer. Original `KeyboardInterrupt` termination remains in that replay; its
five temporary directories are removed only after verified successful teardown.

The `bot_commands` suite exercises actual native handling and eight copied-binary
invocations against a bounded numeric-loopback Axum server. It checks denials,
resolved versus missing IDs, all configured numeric members, negative channels,
Unicode decimal grammar, restart/corrupt cursors and per-invocation redacted logs.
Startup refusals must reach no request, not merely fail against a closed port.
Synthetic API conflict deliberately ends polling with exit 4; it is not normal
shutdown or Python exception equivalence. The suite is selected by the manual
checkpoint and Windows bucket; these additions are uncompiled and unrun, not
live bot verification.

Native suites opt into the contributor `test-runner` through `.cargo/tests.toml`;
the matching debug executable must already exist. The runner's credential scrub
and process ownership do not provide an OS network sandbox. Darwin currently
refuses before spawning suites because complete descendant custody is not
established. See the [native testing boundary](../docs/native-backend.md#expanded-first-party-migration-in-progress).

The shared Git-source corpus now includes a fixed replay of all seven original
audit test methods from six hash-pinned sources. It retains the original
`KeyboardInterrupt` and shared-blank-rule assertions; it is not native parity
proof. A separate native command tracer cancels execution while the second set
listing is held, checking the first-set checkpoint and absence of a partial
report. External process cancellation is not Python exception equivalence.
Historical worker failures retain request/response evidence and gallery thumbnails;
intentional refusal tests clean only their exact, validated owned evidence after
assertions pass. These replays, cancellation and cleanup checks remain unexecuted.

Run from the repository root:

```powershell
.venv\Scripts\python.exe -m unittest discover -s tests -t . -p "test_*.py"
```

**`-t .` is required.** Without it the tests directory becomes the top-level,
modules load as `test_x` instead of `tests.test_x`, and `tests/__init__.py`
never runs — which disables the guard described below.
One module: `python -m unittest tests.test_x -v` — never `python tests/test_x.py`,
which skips the guard and now refuses to start.
`SuiteIsHermetic.test_the_guard_was_installed_before_the_test_modules` fails
loudly if the suite is started without it, and names the correct command.

That canary reads a flag sampled **while the test modules were being imported**,
not the live state of the patch. The earlier form asked whether `socket.connect`
was patched at assert time — but one test does `import tests`, so the package
installed itself mid-run and the canary passed while the real `.env` values had
already been read into the environment. A check something later can satisfy
cannot fail at the moment it is needed.

Tests use Python's stdlib `unittest` (no extra dependencies). Video tests are
skipped automatically when `ffmpeg`/`ffprobe` are not on `PATH`.

Four modules are the exception: `test_panel_browser.py`,
`test_panel_queues.py`, `test_panel_curation.py` and `test_panel_perf.py` need
playwright and a Chromium build, and they **raise** rather than skipping when
those are missing — a browser test that reports green on a machine with no
browser is worse than no browser test at all. All four reach the actual Rust panel
through `_panel_browser_fixtures.py`, using a throwaway catalog and an owned native
server process. Startup polls `/api/ping` with a ten-second ceiling; teardown reaps
the server's process tree. JavaScript timing/media fixtures and committed database
assertions are unchanged. `test_ci_coverage.py` watches this shared harness.

```powershell
.venv\Scripts\python.exe -m pip install -r requirements-dev.txt
.venv\Scripts\python.exe -m playwright install chromium
```

Set `NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` to opt out on purpose. CI does exactly
that in the Python matrix and runs all four in its own `panel-browser:` job
instead: they test JavaScript, so once per Python version would download
Chromium twice to prove the same thing. That job names its modules explicitly
rather than globbing, so `test_ci_coverage.py` fails if a new browser suite is
added without being listed there.

The additive Rust browser cohort in `native/tests/panel_browser.rs` now authors
counterparts of the 97 original declared browser cases. It owns one real Rust
router/catalog and one headless Playwright instance, with isolated contexts,
original JavaScript fixtures, actual SQLite observations and real animated WebP
performance probes. It calls only typed external Python libraries, not the
first-party Python harness. The browser job selects it once through the native
process-tree runner (900-second wall / 180-second idle ceilings); browser files
stay in the project-local cache. Browser fixtures acknowledge shutdown but are
removed only by the outer runner after descendant quiescence; failed or unready
fixtures retain their diagnostics. These new tests remain **uncompiled and unrun**;
authored correspondence is not parity acceptance, and all original suites remain.

A third job, `windows-safety:`, runs the suites whose behaviour Linux and WSL
cannot exercise — native file-handle locks, migration replay and rollback over
real paths, atomic plan writes, the decoder subprocess and the CLI entry points
— plus the sandbox clone, whose ownership lease and colon-bearing media
filenames only mean anything where a colon opens an alternate data stream —
on an ephemeral `windows-latest` runner. Which suites those are is a
judgement rather than a property of the code, so each one declares it with a
module-level `RUNS_ON_NATIVE_WINDOWS = True`, and `test_ci_coverage.py`
enforces the match in both directions: a marked suite missing from the job, and
a name in the job that no longer marks itself, each fail.

## The suite never touches the real network

`tests/__init__.py` runs before any test module and scrubs every
token/API-key-shaped environment variable, points `TELEGRAM_API_BASE` at the
discard port, stops `emojikit.build_pack.load_env()` from reading `.env`, and refuses
outbound sockets to anything but loopback (`NetworkAccessDenied`).

The `.env` half no longer depends on that file running at all:
`emojikit.build_pack.load_env()` refuses to read `.env` whenever a test runner owns the
process, decided from the entry point's own `__spec__`. The package-level scrub
could only remove credential-*shaped* names and only protected what was imported
after it, so anything else in `.env` stayed visible for a whole run.

This exists because a test meant only to check a CLI usage error once reached
live Telegram and replaced a sticker in a published pack. Inject a fake session
rather than adding an opt-out.

The original reencoding cohort retains all 21 unchanged assertions plus all 42 original
media assertions, plus 12 original finite-timeline and repaint assertions, through ten admitted
Git sources, the same bounded historical worker and 67 expected delayed directory contexts.
No media or reencoding method is excluded; the repaint module-ownership assertion remains an
explicit exclusion, not a skipped success.
Real Pillow/gzip/JSON and the original static/vector/video identity, conversion, validation,
probe, decoder/cache and sampling definitions supply the reference; no native result is an
oracle. Original FFmpeg encode/copy/sample/conversion/container-generation commands use the
existing owned capture with 5-second wall/3-second idle bounds inside the 15-second wall/8-second
idle historical session, explicitly adapting the original subprocess timeout of 120 seconds.
The native `codec_reencode` suite adds the exact hidden-RGB and opaque WebP recipes with full pixel,
changed-byte and stable-key checks. `codec_compare` adds the original real lossy artwork, foreign
shape/colour, exact-self and unknown comparisons. `codec_lottie_prepare` checks the original empty
animation, deterministic gzip timestamps and unchanged decoded document. Native actual-cap and
unsupported-format refusal boundaries are explicitly adapted from the original monkeypatch/false
return. The four original single-frame video scenarios share one exact real encode, independent
stream copy and native reencode; the two underdeclared-duration scenarios use the existing
controlled-tool cohort. All 21 methods have pending native candidates; the historical replay
retains the two original mocked duration methods and all four real single-frame methods. Four
original one-frame encodes, three stream copies, seven conversions, six container generators
and one alpha-first-frame read require completed-process accounting, exact argv/path admission
and empty owned capture scratch before success-only cleanup.
The existing native static, Lottie-file and video-operations suites now carry pending counterparts
for all 42 original `test_media.py` methods: real image/hash/visibility recipes, real compressed
animation validation, original GIF/transparent padding and actual video-container refusals.
Three controlled success variants join the existing bounded tool cohort (27 cases total) to
observe one-decode accounting and decoder-before-input selection without replacing real-codec
coverage. The `codec_video` suite also authors all 14 original video-identity counterparts
using one shared set of exact 64×64 alpha/VP8/VP9/structured-frame recipes: eight encodes,
one genuine remux, an extensionless copy and a same-path codec replacement. Public real-frame
availability plus successful comparison adapts the original private `_premultiplied` assertion;
it is not private API equivalence. Original blue solid/keyframe/gradient recipes run at the
existing typed tint seam, while the actual compressed-file tests retain valid, NaN, infinity
and nonnumeric timeline cases. All new execution remains pending.

## Layout

| File | Covers |
|------|--------|
| `test_media.py` | format detection (`emojikit/media.py`) and identity (`emojikit/identity.py`): content/perceptual hashing, `same_image`, static→PNG, GIF→WEBM, Lottie→TGS, and the animated contract (512×512 canvas, frame rate, duration, gzip packaging, bounded decompression) |
| `test_media_reencode.py` | owner rule 1 — never republish another pack's file byte-for-byte — and the requirement pulling against it: the result must be pixel-identical, because the content key is computed from decoded pixels and a lossy re-compress would split one catalog row into two. Also `same_image` across a re-encode |
| `test_media_bounds.py` | the bounds around ingest: decompression limits, the perceptual-hash threshold range, and the signed storage conversion |
| `test_media_fitting.py` | `fit_100` must not touch opacity. It pasted the image using ITSELF as the mask — a composite against the transparent canvas, so colour came back multiplied by alpha and alpha squared |
| `test_identity_video.py` | video identity: alpha is part of the picture, and the two fingerprint APIs sample the same way. ffmpeg's default `vp9` decoder drops the alpha layer in silence, so the decoder is chosen from the PROBED codec, never the extension. Real VP9/VP8 encodes |
| `test_lottie_repaint.py` | `validate_tgs` against a timeline that is not a number (NaN defeats every comparison), and `repaint_in_place` against the two shapes it silently skipped: a keyframed colour and a gradient's opacity ramp |
| `test_catalog.py` | catalog dedup (exact + perceptual), `file_unique_id` skip, pending/upload tracking, persistence |
| `test_media_paths.py` | media paths survive a folder move: in-folder paths stored relative to the data folder, archive paths absolute, an older catalog converted once (absolute and project-relative rows), a converted one left alone, and a direct SQL reader resolving exactly like the catalog |
| `test_catalog_order.py` | the manual publish order (the `position` column) shared by the panel's drag-and-drop and `build_collection`'s publish order |
| `test_migration_provenance.py` | Legacy CLI recovery requires unambiguous immutable identifiers across snapshots; reused paths/slots, conflicting identifiers and missing provenance cannot rewrite required references |
| `test_ingest_retention.py` | Both fake-Telegram CLI paths preserve complete refused VP9 downloads and provenance after scratch cleanup, without replacing sentinels or binding identifiers |
| `test_video_collision_ingest.py` | Native-frame video identity through all ingest paths, exact/near merge and recovery, including brief/VFR differences, extra tails, remux/re-encode, reopen/resume and occupied destinations |
| `test_identity_migration.py` | `emojikit/collection_migrate.py` + `scripts/identity_repair.py`: moving EVERY durable reference to a content key when the decoder changes what that key IS — three tables, the derived `phash`, the publisher state and plan files, and the archived filenames that embed `key[:12]`. The properties under test are the refusals and the recoveries: a collision stops it rather than merging two rows, an unreadable row stops it rather than half-converting, the backup is taken through SQLite's online API (a `copy2` of a WAL database opens as `no such table: items`), a crash between any two stages resumes because every stage is idempotent, and a second run is a verified no-op |
| `test_state_artifacts.py` | a curation plan is live state, not an expendable sidecar: one inventory decides which JSON migration and rollback must carry, and only schema-defined ID fields are remapped — free-text labels are left alone even when they look like keys |
| `test_sqlite_snapshot.py` | real BUSY/LOCKED contention: a snapshot refuses inside its budget instead of waiting forever, the destination's original data survives that refusal, and a retry succeeds once the lock is released |
| `test_migration_lifecycle.py` | A03-A06: native actual-writer exclusion, pending-journal refusal, destination collisions, CLI replay after SQLite/JSON/media faults and real process death, complete rollback with unchanged-pack reconciliation, and final missing/decode/reference refusals. Decoder values are injected only for bookkeeping; SQLite, files, native processes and HTTP saves remain real. |
| `test_video_identity_fidelity.py` | identity-grade decoding either carries a video's alpha or refuses. A missing `libvpx-vp9`, a failed codec probe and a container naming no codec all used to answer "no decoder needed", which drops the alpha layer — two clips differing only in opacity then share one key, and `Catalog.add` merges them and deletes the file it merged away. Real VP9 and VP8 encodes |
| `test_video_timeline_comparison.py` | `same_image` on video compares the whole timeline, not frame zero. Two clips sharing ten opening frames used to compare equal; so did two differing in exactly one frame, because the identity stream is sampled at 10 fps and the content-key shortcut answered from that. Ends at `_resolve_sticker_key`, asserting no foreign id is written |
| `test_resume_safety.py` | the duplicate-upload paths, driven through `emojikit.build_pack.main()`: recorded-cursor resume, write-ahead in-flight record, atomic state writes, refusal to guess on unexplained drift, per-set limits |
| `test_unresolved_mutation.py` | the same engine when the live state is UNKNOWN: an ambiguous create, an unresolved in-flight record, a recorded set that no longer reads back. The guarantee is that the run stops rather than guess |
| `test_telegram_client.py` | `telegram_api.Telegram` on its own: token redaction, the STICKERSET_INVALID retry scope, and what the client accepts as evidence that an upload landed |
| `test_pack_locks.py` | `packstate.exclusive_lock` mechanics: refusal, release on error, stale reclaim and its races, ownership, heartbeat, and the lock-path helpers. (`test_lock_order.py` checks the documented ORDER of the same locks, by AST.) |
| `test_pack_locks_exclusion.py` | that two real processes never hold one pack-family lock at once. The old recovery path could seat two publishers: it decided ownership by reading a file, comparing a token and unlinking, and an unlinked inode is a lock nobody else can see. Two genuine subprocesses, not threads |
| `test_rebuild_dedup_state.py` | the `coins/rebuild_dedup.py` delete phase (an old pack that survived deletion must not complete it), the link message's retry bound, and the owner id parsed at import |
| `test_rebuild_dedup_resume.py` | resuming the rebuild after an interruption, decided by identity: an ambiguous upload stops the run, a failed live read is not "0 stickers", an in-flight upload and an adopted create are proven by content |
| `test_rebuild_dedup_plan.py` | the frozen plan, the cursor that walks it and the state schema; the `build` command's exit (saved cursor on trailing skips, partial while an upload is unresolved) |
| `test_rebuild_dedup_locks.py` | one rebuild per pack family: the lock keyed on the pack base, taken and released around every run |
| `test_rebuild_dedup_map.py` | the second phase of the same module: `map_and_fill` resolving `ticker_to_id.json` by image identity under the map lock, and the shared-logo-group guard |
| `test_publish_dedup.py` | verified retries for non-idempotent Bot API calls, live-set reconcile, adopt-on-occupied, recorded fuids |
| `test_build_collection_state.py` | `build_collection`'s state machine: plan/state files failing closed, live-set drift, identity on a recorded position and the unattributed tail |
| `test_publish_cli.py` | the same publisher at its CLI boundary: argument validation, `--new-set`/`--into-pack`, and `main()` end to end. Both halves drive `_CatalogFixture` |
| `test_publish_contracts.py` | the publisher contracts that are NOT collection state: pack titles as one sequence, the mixed-family layout, the blank-video guard, and the announcement path all three publishers share |
| `test_publish_invariants.py` | two invariants the publisher states but only enforced in one place each: a set that closed mid-run is never appended to on the next item, and the blank-media check gets the ITEM's format rather than the family's, so `--mixed` cannot bypass it |
| `test_preflight_outcomes.py` | preflight may not report acceptance it never obtained. The old counter counted ATTEMPTS, so a run in which every `check_uploadable` failed at the transport reported "all accepted" |
| `test_reconcile_uniqueness.py` | unique attribution means every rival was EXCLUDED. With two candidates matching one live sticker the function correctly reported ambiguity — and deleting one candidate's FILE made it answer "unique" with the survivor, so removing evidence promoted a guess to a certainty |
| `test_reconcile_identity.py` | recovery must never give a stranger's picture our item's identity. dHash is a grayscale STRUCTURE hash — an opaque red square and an opaque blue one are zero apart — so a perceptual match may only NOMINATE; content verification decides, and "more than one" and "could not examine" stay distinct from "no match" |
| `test_brand_logo.py` | the mandatory logo-first behaviour in `build_collection`: its conversion per format, and that no default logo or bot list survives in source |
| `test_operator_config.py` | the operator's identities come only from configuration: an unset key stops naming itself, empty `BRAND_LOGO_BOTS` means none, a listed bot needs an existing logo |
| `test_pack_manifest.py` | the `packs/` roster: what it records, and when it admits to being stale |
| `test_pack_archive.py` | when a FULL pack earns an archive move, and when the archive has stopped being true |
| `test_sync_order.py` | reordering an already-published pack with `setStickerPositionInSet`, which moves a sticker without re-uploading it, so every `custom_emoji_id` survives |
| `test_entry_point_contracts.py` | exit codes and argument validation at the CLI boundary of `fetch_pack`, `make_emoji_pngs`, `panel` and `logsetup` |
| `test_entry_points.py` | every executable imports cleanly in its own interpreter, and the suite's hermetic guard was installed before the test modules were imported |
| `test_fetch_emoji_ids.py` | premium-id extraction and de-duplication in the collector |
| `test_fetch_pack_limit.py` | `fetch_pack --limit N` delivers N NEW items — counting already-known stickers against the cap made a re-run a no-op |
| `test_make_emoji_pngs.py` | the image converter: blank guards, output freshness, source priority |
| `test_coin_cli_args.py` | the coin tools refuse an unrecognised argument instead of falling through to the live branch — a typo must not publish |
| `test_coin_logo_cache.py` | `fetch_logos` resume: a cached file is re-validated before it is trusted as a logo |
| `test_coin_http.py` | the one pooled `coins/_http.py` client: retry ladder, `Retry-After`, and the paging delay |
| `test_coin_providers.py` | the coin providers publishing logos: blank-logo refusal, the verified publish, and the canonical map re-read under the lock |
| `test_coin_recovery.py` | `fetch_paprika`'s unverified-upload recovery: an add that MAY be live is reconciled against the live set, never silently re-sent |
| `test_coin_ticker_map.py` | every writer of `ticker_to_id.json` — alias/enhance/provider — serialised so none loses another's update, and one inventory implementation |
| `test_remap_ids.py` | `coins/remap_ids.py`: a download cache that used to be trusted blindly, the `--apply` refusals, and the map written under the pack-family lock |
| `test_check_all_packs.py` | `coins/check_all_packs.py`: the audit report describes the LIVE packs, an analyse failure is an error retried next run (not a blank), and a throttled set listing is retried |
| `test_coin_keywords.py` | `coins/_keywords.py`, the one writer of `keywords.csv`: invalid PNGs are not listed, an empty name keeps the old one, a crash leaves the previous file whole |
| `test_contracts_shared.py` | the Python half of the Python↔Worker contracts, read from `fixtures/contracts/` (the Worker's `contracts.test.ts` reads the same files): the publish body, and the custom-emoji id syntax |
| `test_build_pack_announce.py` | `build_pack` announces finished packs as the bot whose token it publishes with |
| `test_cli_env.py` | reading `.env`: a file saved with a byte order mark keeps its first key |
| `test_telegram_body_shape.py` | a JSON body that is not an object goes through the client's retry and applied-check path instead of raising `AttributeError` |
| `test_launcher_contract.py` | every flag `run.ps1` (and `scripts/run-actions.ps1`) passes exists in that tool's `--help` |
| `test_cli_docs.py` | GUIDE §12, the CLI reference, against each tool's own `--help`: every flag documented, every documented flag real |
| `test_pack_rows.py` | the one renderer behind every "emoji in this pack" table, and its four writers: a pipe or a newline in a label is escaped, column headers unchanged |
| `test_pack_export.py` | `pack_archive --export`: a pack as a zip in slot order with its manifest, sources untouched, refused while the roster is stale or a file is missing |
| `test_status.py` | `emojikit.status`: offline, read-only; exit 3 and the fix command when the roster or archive is stale |
| `test_plan_status.py` | `emojikit.plan_status`: what the panel's saved pack plan would change, read-only, including the ids a move would retire |
| `test_plan_apply.py` | `emojikit.plan_apply` against a fake Telegram: the dry run writes nothing, 25 changes take two runs at the cap, a delete applied before a crash is settled and never re-sent, the next new pack leads with the logo, and every refusal (over cap, unknown key, per-format family, pack gap, unreadable set, held lock) changes nothing; plus `Catalog.unpublish` |
| `test_panel_perf.py` | the browser performance suite: real animated WebP previews, the 24-card budget nearest the centre, All visible, the scroll freeze, bounded zoomed-out windows, unchanged CSS variables, the pick-ring pause, bulk hold matching the item-by-item result. Each scenario prints one `PANEL_PERF` line; timings are printed, never asserted, counts are. Runs in the `panel-browser` job |
| `test_panel_startup.py` | the panel binds and opens the browser without waiting on a hanging bot-name lookup, and a panel already on the port is reopened, not rebuilt |
| `test_suite_guard.py` | every test module refuses a direct run (its `__main__` block), and `load_env()` treats an imported `unittest` as a test run and nothing else |
| `test_fetch_pack_ids.py` | `fetch_pack` given an emoji id fetches the pack it belongs to, once; an id that names no pack is reported |
| `test_verify_logos.py` | `verify_logos`: the inversion-aware distance, the durable replacement intent bound to its own `--map`, and the fix path's exit codes |
| `test_panel.py` | what `emojikit/panel.py` serves of the catalog: inert item JSON (no script breakout), the save-during-reorder window, an unavailable catalog, and saving from a filtered grid |
| `test_panel_view.py` | what `emojikit.panel_view.build_view` makes of the catalog: brand-logo preview, the similarity order, the tap-to-copy id, the published-item filter, the pack index travelling onto an already-live card so the grid can draw the boundary between two packs, and one pack unhidden on request |
| `test_panel_guard.py` | the POST guard (token, loopback Host/Origin, content type, body cap, exact-permutation order) and the two behaviours built on it. `MutationGuard` owns nine `test_*` methods and is subclassed twice, so all three classes must stay in one module — importing the base elsewhere would collect it again rather than move it |
| `test_panel_page_actions.py` | served-text contracts (`emojikit.panel.PAGE`, `emojikit.panel.SCRIPT`) for the editing gestures: drag-and-drop (every dragover edits the model and re-projects the grid, the drop records the dragstart snapshot, a cancel restores it, the pointer's side of a tile decides before-or-after, a carried card is parked rather than unmounted), undo/redo, and selection mode (a click drives the picks or `included`, the pick box owns its own click, a picked card's ring is a rotating transform that stops under reduced motion) |
| `test_panel_page_grid.py` | served-text contracts for the grid: the viewport observers and the scroll-time animation freeze, the virtual grid (only rows near the viewport exist, integer row arithmetic), zoom (buttons, Ctrl+wheel, a typeable percentage, clamping, compact below 75 %), the page actually shipping its scripts, the pack separators and the jump buttons. Shared text comes from `_panel_page_fixtures.py` (no test classes) |
| `test_panel_assets.py` | the scripts ship as real files, the page loads every one of them in order with a version that is their content hash (`emojikit.panel.ASSET_VER`), the icon URL carries its own content hash (`ICON_VER`, so a replaced logo is not served stale), the inline block carries values not behaviour, and `/static/<script>?v=…` reaches the file on disk (immutable-cached, so a stale script would otherwise be served forever) |
| `test_panel_sandbox.py` | that the test sandbox reaches neither the owner's data nor their account: the argument allowlist (a forwarded `--data-dir` used to win over the wrapper's own), a clone that shares no bytes and keeps no source path, the refusals that abandon a clone rather than finish one that points at production, and the ownership marker plus lease that stop one sandbox's start-up from deleting another's catalog |
| `test_sandbox_lifecycle.py` | that making a sandbox never writes to the source and that a live sandbox is never reclaimed: the source's journal mode, full dump and raw bytes unchanged on success and refusal; the project-relative media file chosen over a same-named decoy; same-length corruption and a source changed mid-copy refused; distinct keys never sharing a file; a sweep during construction and during serving removing nothing, then reclaiming after the real serving process dies; a real wrapper answering a real HTTP Save on the clone only. Native Windows |
| `test_atomic_state_files.py` | that a state write touches only its own temporary file: an unrelated `<name>.tmp` survives success and every injected failure; a failed write keeps the old content; two writers released together both finish; an existing file keeps its permissions (POSIX); Windows' transient refusal to replace is retried, and a persistent one still fails within its deadline. Native Windows |
| `test_panel_save_scope.py` | `POST /api/save` carries the FULL selection, so a request with no notion of scope speaks for rows the page never saw. The request now states what it was showing, the server applies the decision only inside that scope, and a page too old to say is refused with 409. Driven through the real handler on a real socket |
| `test_panel_queues.py` | what the panel still owes the server, on a fake clock: unsaved ticks are dirty even when no Save is in flight, an older acknowledgement cannot clear newer unsaved work, a permanent 400/409 stops resubmitting (its own comment said retrying could not fix it, and it retried anyway), the 5-second heartbeat obeys the backoff instead of walking past it, and a fetch that never resolves is released by a raced deadline rather than claiming the queue forever |
| `test_panel_curation.py` | Real browser controls: Shift picks, held-card removal/counts, per-card/all release, full-pack refusal, drag Undo/Redo, original-slot restoration, explicit Save checkpoints, Reset history and real SQLite reload. Selection by hand in BOTH surfaces -- a click anywhere on a card picks it, in the tray and in the grid, Shift takes the range, the pick box still toggles exactly once, an unpicked box draws no check and a picked card wears the moving ring. And the pack a drop joins: the card the pointer was AIMED at decides it, for a drag out of the tray and a drag inside the grid alike, a drop that aimed at nothing stamps nothing, and a drop into an unnumbered run invents no number. Classes share the page fixture through a `PanelPage` mixin, never by subclassing a `TestCase` -- that made unittest collect the parent's tests again per subclass |
| `test_panel_instance.py` | reopening a panel reuses its own server instead of failing a second bind, and tells an unrelated listener on the port apart from one of ours |
| `test_panel_plan.py` | the move plan as a pure function: what counts as a move, what is merely held, what is absent because the page had no opinion, and the merge that keeps out-of-scope decisions while dropping the ones about emoji the catalog no longer has |
| `test_panel_intent_persistence.py` | the saved layout survives, through the real HTTP handler and SQLite: repeated Save/reload/Save, a partial view that merges instead of overwriting, an older client that sends no `packs`, a legacy plan carrying only `moves`, invalid/duplicate/out-of-scope targets, and a malformed plan that is preserved rather than replaced with an empty one |
| `test_panel_logging.py` | Bounded UI event schema accepts diagnostics and rejects private/arbitrary fields or invalid numeric values before writing any log |
| `test_panel_preview.py` | Real HTTP previews: requested size/rate, cache reuse, single-frame posters for all formats, VP9 alpha preservation and invalid-resource-budget refusal |
| `test_ci_coverage.py` | the browser job runs from a hand-maintained list of module names, so this fails when a suite imports the harness without being named there — and asserts none is run twice |
| `test_fake_contracts.py` | every test double whose method shadows a `Telegram` method must accept what the real client passes. `FakeTelegram.send_message()` rejected `disable_preview`, so every happy-path announcement in two suites was silently exercising the swallowed error path while the tests stayed green |
| `test_panel_browser.py` | what the panel's client code actually DOES, in headless Chromium: the save pipeline's revisioned queues, the separator nodes, the zoom anchor, the animation freeze, the pack count, and a denied `localStorage`. A source-text assertion cannot tell a correct implementation of any of these from a broken one. Needs `requirements-dev.txt` and `python -m playwright install chromium`; set `NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` to opt out deliberately, never to make a missing browser look green |
| `test_panel_server.py` | the panel as a process: which Host may reach it, and who owns the port. Real subprocesses and real sockets, so the slowest of the four |
| `test_repaintable.py` | the `--repaintable` gate: the flag is read off the STICKER not the set, only an explicit yes proceeds, `skip`/`keep` never prompt, and an unanswerable prompt (EOF, Ctrl-C) is a no rather than a crash. Also `--tint`, which bakes the repaint instead of skipping: colour parsing, a Lottie recoloured through its tree (fills, strokes and gradient stops, offsets kept), a static filled through its alpha, video refused, and the tint recorded on the item |
| `test_emoji_bot.py` | the bot's pure helpers: entity extraction and reply building |
| `test_logsetup.py` | secret redaction, plus a guard that fails if any `.env` secret value appears in a git-tracked file |

`_pack_fixtures.py`, `_rebuild_fixtures.py`, `_cli_fixtures.py`,
`_bc_fixtures.py`, `_panel_fixtures.py`, `_media_fixtures.py`,
`_panel_browser_fixtures.py` and `_coin_fixtures.py` hold the
fakes shared by the modules above them (the PNG builders, `FakeTelegram`,
`RebuildCase`, and the standalone-script loader every entry-point contract
module imports). One copy each, because a duplicated fake drifts away from the
thing it stands in for. The leading underscore is load-bearing:
`-p "test_*.py"` must not collect them as test modules. `_media_fixtures.py`
holds `make_png` (a block on transparency, any size) and `encode_vp9` (numbered
frames to an alpha-keeping WebM), which the publisher and video suites share;
`_cli_fixtures.cli_help` runs a tool's `--help` without writing a log file.

**Deprecations fail.** `tests/__init__.py` turns a `DeprecationWarning` issued
by the project's own code (`emojikit`, `coins`, `scripts`, `tests`) into an
error, so a removal shows up one release early; third-party warnings stay
warnings.

**Run chosen suites with the gate, not a hand-written command:**
`.\scripts\check.ps1 -Tests test_catalog,test_panel_plan -SkipCompile -SkipLint`
runs them as `tests.<name>`, so the guard above still applies.

A fixture may only be shared if it carries no `test_*` methods and no
TestCase base. One that does multiplies with every importer instead of
moving — which is why `MutationGuard` stays whole in `test_panel_guard.py`.

**Patch the namespace the CALLER reads, not the one that defines the name.**
`build_pack` does `from telegram_api import Telegram`, so the name lives in
`build_pack`'s globals and `patch.object(telegram_api, "Telegram")` leaves
the real client in place — the suite then dials the discard port and sleeps
through the retry ladder. `announce_via_worker` is the mirror image: its
only caller is `announce.announce_packs`, so it must be patched on
`announce`.

`FakeTelegram` binds the **real** `emojikit.build_pack.Telegram.send_message` rather than
reimplementing it. The retry count and the link-preview flag are guarantees
those tests assert, and a hand-written copy would keep asserting them long after
the shipped method stopped providing them.

The native video-fidelity candidate adds the original four real100×100 VP9/VP8
alpha-only recipes, eight bounded concurrent callers and a still-image exemption.
Six refusal/recovery cases reuse the existing controlled-tool worker and process
owner. String errors/public recovery are explicit adaptations of original private
Python mocks and exception classes; no runtime parity is claimed. Original creator
cases reuse the existing conversion artifact/deadline for freshness, corrupt/blank
repair, real SVG priority, failure exits, marker observation and quarantine. Three
copied dry runs check original coin-keyword source selection without network calls.
All candidates remain pending until compilation and actual hosted execution.

## The Cloudflare Worker has its own suite

`worker/` is TypeScript and is **not** collected by the command above, nor by
`scripts\check.ps1` — running it needs Node, which nothing else in this project
does. Run it yourself after touching `worker/`:

```powershell
cd worker
npm ci               # exactly the committed lockfile, like CI
npm run typecheck    # tsc --noEmit
npx vitest run       # the Worker's own suite
```

Same hermetic rule, enforced the same way: the tests stub `fetch`, so nothing in
them can reach Telegram. CI runs these in a dedicated `worker:` job — separate
from the Python matrix, which needs Python and ffmpeg and would otherwise run
this suite once per Python version.

## Fixtures

| Fixture | Purpose | Safe to commit |
|---------|---------|----------------|
| `fixtures/lottie/red_circle_512.json` | minimal valid 512×512 Lottie animation used to exercise TGS packaging/validation and animated content hashing. 512×512 is Telegram's required canvas for animated emoji — a 100×100 fixture would encode the wrong contract | yes (synthetic, no secrets) |
| `fixtures/contracts/publish_request.json` | the exact body `announce_via_worker` sends; read by `test_contracts_shared.py` and `worker/test/contracts.test.ts`, so either side changing it breaks both | yes (synthetic, neutral names) |
| `fixtures/contracts/emoji_messages.json` | messages and typed id lists with the ids each side must extract | yes (synthetic) |

Static images and animated GIFs used by the tests are generated on the fly with
Pillow in temporary directories and are **not** committed. No network access or
real Telegram credentials are required to run the suite.
