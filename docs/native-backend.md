# Native local backend

Current branch integration: the public CLI and panel use `NativeMediaAdapter`
and the same executable's Rust codec worker. `numera-build`, `package-native`
and `install-prebuilt` provide the Rust build and packaging path. The historical
checkpoint notes below describe earlier snapshots; current validation and
remaining codec/compatibility dependencies are tracked in
[CI acceptance](rust-ci-status.md).

The installed Rust backend is now the primary local command path. The Windows
menu and doctor invoke it; superseded Python application modules have been
retired from production. Final integrated source-retirement acceptance and all
four platform package checks have passed; the revision-bound evidence is below.
See [ADR 0003](adr/0003-rust-local-backend.md) for the decision and rollback boundary.

## Expanded first-party migration (in progress)

The accepted backend migration above does not mean the whole repository is
Rust-owned. A subsequent migration includes the remaining local media/identity,
coin-domain, test, generator and tooling code. Browser assets and the entire
Cloudflare Worker remain unchanged; third-party codec implementations remain
external dependencies.

The initial static-image/identity/conversion, SVG renderer binding, pure Lottie
tint/bounded TGS loading/canonical animated identity/loaded-document validation
and Lottie preview/file-preparation/file-validation/vector-comparison handling,
static preview/tint/tri-state comparison, source media detection/collision keys
and coin-alias/inventory-text/block-parser/frozen-group-plan/alias-replay/provider-status-policy
and bounded provider-GET/retry/Paprika-search/search-cache/rebuild-resume-validation
slices are additive. Provider-logo conversion uses the same native static fit but
rejects sparse/blank source pixels before enlargement. Initial CMC authentication,
status and logo-metadata handling is also additive; synthetic loopback tests do
not spend provider quota or publish to Telegram. Public coin orchestration,
persistence and full original-case parity remain pending. Search-cache persistence
now has an additive native operation using existing OS leases and atomic writes;
it records each answered coin, leaves unanswered searches uncached and preserves
progress across a failed later search. Public provider CLI integration and full
restart/platform acceptance remain pending. The additive native keyword writer
retains earlier names after failed lookups, verifies/reopens PNGs before admission,
and writes quoted UTF-8/CRLF CSV atomically. Initial CoinGecko canonical-name
paging preserves market-cap precedence and incomplete-lookup status; public
fetch/build commands and complete source coercion parity remain pending.
Provider upload proof and recovery ordering are being mapped independently:
recorded dHash, unknown/duplicate refusal, idempotent provider tally and map-before-
source promotion. These policy seams are not yet the public native publisher;
initial durable recovery and private staging are being connected using the
existing transport, family-before-map leases and atomic persistence. Pending
intent sources survive cleanup; only proven matching bytes become the shared
logo. A source-traced native top-up policy now maps existing/overflow sets, writes
intent before mutation, retains unknown outcomes and maps only content-proven
IDs. Native effects now connect existing Bot API transport, verified-retry
checks, private staging and durable recovery; a bounded loopback upload/readback
checks one-time publication and a subsequent mapped-ticker skip. Full ambiguous-
mutation, crash/restart, platform and public-command acceptance remain pending.
A failed durable intent-clear keeps its in-memory pending record too, so cleanup
cannot delete the only source while the on-disk intent still needs recovery.
An initial video seam owns
FFprobe, alpha-capable VP8/VP9 decoding and the frozen identity stream. The next
additive slice includes canonical first-frame dHash, bounded file/toolchain-keyed
working sets and complete native-frame/timestamp comparison. Video validation,
blank sampling, VP9 conversion, stream-copy preparation and WebP preview are
covered at independent source-fixture seams. Internal worker video fingerprint,
comparison, validation, preparation, conversion and preview dispatch are being
connected; the production adapter and installed packages remain unchanged.
Gallery thumbnails and animation metadata are also being mapped at their own
source-fixture seam, retaining their distinct filters/sizes/quality.
The additive adapter now requests optional session ownership from the existing
process owner: POSIX nested private groups share one session; Windows retains
its existing Job containment. The real worker places nested codec captures
inside its validated request envelope. Session status collection first observes
terminal root state without reaping, then samples membership while that original
root still anchors identity. The same gate serves wait and final cleanup.
Consumer cleanup also retains scratch for either supported ownership-marker
layout, unreadable containers or links. A statically dispatched codec contract
lets brand, previews, collection, gallery, publication, identity maintenance,
conversion, local ingest and requested-ID collection use native IPC; installed
selection is unchanged. Authored tracers cover GIF motion, cache/RGB repair,
ID diagnostics/tint/dedup, video collisions, downloaded recovery and cancellation.
Publication readback and live attribution now share invocation-owned private
storage: the downloaded file and scoped codec workspace are cleaned only after
custody is resolved. Cleanup failure refuses rather than silently succeeding;
unresolved codec evidence keeps the input bytes and blocks durable attribution.
The new identity/uniqueness and synchronized private-download tracers remain
uncompiled and unexecuted, not accepted runtime guarantees.
Copied-command tracers cover requested-ID getMe/tint/repeat/mixed outcomes through
retained codecs, and coin-audit corrupt-image retry/cache/report/log effects;
none proves Python-free packages. All remain unexecuted.
They are not an OS sandbox or proof of abrupt-wrapper-death cleanup; deliberate
session escape, signaling races and four-platform acceptance remain unresolved.
Loaded Lottie validation now carries original exception identity through the
animated `usable` worker path: explicit media refusals produce the handled error
envelope; numeric overflow, iterable/attribute faults and truncated-stream EOF
remain uncaught. Existing string-returning APIs retain their messages. The 84
loaded-document and seven file observations remain independent source fixtures;
typed and actual-binary tracers are authored but unexecuted. File validation now
retains the external decoded JSON object instead of round-tripping it through
serde: native validation policy can distinguish named nonfinite timeline refusals
from ignored nonfinite metadata. The finite-document API uses the same policy;
number conversion, iteration and error formatting use unchanged external object
APIs. Animated fingerprint now also carries the original decoder class and catch
membership through its typed worker branch; raw-byte fallback remains limited to
the original OSError/ValueError boundary. Existing source-derived identity keys
are unchanged. Typed Lottie preview/gallery now reuse the same decoded object,
preserve external exception identity and destroy the renderer before still-image
encoding. Short-clip rate selection keeps the original numeric conversion and
Exception-only fallback; interruptions escape. Source-inspected resource/fault
tracers and existing same-platform render goldens are wired but unexecuted.
Animated conversion also uses the same decoded object, deterministic encoder and
validation policy. Canvas conversion calls the original external integer API
rather than narrowing dimensions to i64. Generated gzip is checked through the
same bounded decoder, after the compressed-size cap and before validation, so
large plain JSON cannot bypass the 8 MiB expansion limit. The native exclusive-
publication rule still prevents failed candidates from becoming output. Its
byte/refusal, numeric and decoded-size boundary tracers remain unexecuted.
Animated preparation now shares that external-object encoder too. Rust owns the
single recolouring policy over typed lists/dictionaries, retaining nonfinite
metadata, alpha, keyframes and gradient opacity. Ordinary codec exceptions warn
and preserve input; interruptions escape. Re-encoding still occurs before a later
tint refusal. Existing byte goldens and source-inspected warning/order/fault tracers
are authored but unexecuted. Exact recursion-depth behavior, final-output failure
parity and broader path/consumer acceptance remain pending.

The additive contributor command `source-lottie-render --output-dir DIRECTORY`
is authored to replace the Python render-oracle driver. It reads two exact pinned
Git sources, admits the original fixture recipes and physical input hashes, and
uses only their fixed historical renderer functions to produce same-platform
observations. The output directory must be new and project-contained; it contains
`lottie-preview-source.json` and `gallery-codec-source.json`. It requires exactly
CPython 3.11.9, Pillow 12.3.0 and rlottie-python 1.3.8. It bypasses operator
configuration, uses an owned wall/idle-bounded worker and writes a redacted UTC
run log under `logs/`; unresolved render diagnostics stay in project scratch.
The migration workflows are wired to consume these observations, but the new
command and its tests have not compiled or executed. The original driver is
retained until the tooling and retirement gates pass.

The additive `source-static-images --output-dir DIRECTORY` command similarly
replays the five original static conversions and seven provider-image cases. It
requires exactly CPython 3.11.9, Pillow 12.3.0 and resvg-py 0.5.0, reads both
original sources from pinned Git, and writes `static-convert-source.json` and
`provider-image-source.json` into a new project-contained directory. Frozen
pixels, dimensions, modes and refusals remain independent acceptance inputs;
only platform PNG observations are regenerated. The original driver's explicit
`MediaError=RuntimeError` substitution is disclosed, not actual exception-class
parity. Both media commands share one bounded historical process and exclusive
output implementation. Their new code and tests remain uncompiled/unexecuted.
Fixed source-corpus replays select seven original fitting/backdrop methods,
six brand-preparation methods and twelve ID-inventory methods. Historical media,
regexes, cache and file-reader assertions remain independent of native results.
Native tracers retain fitting, cache refusal and ID diagnostics; required-logo
and strict UTF-8 boundaries disclose their adaptations. Publisher/import identity,
whole-command parity and native resource lifetimes are not proved; all these
new replays remain unexecuted.

The explicit contributor `test-runner EXECUTABLE [LIBTEST_ARGUMENTS]`
entry point is wired through `cargo test --config .cargo/tests.toml`. Build the
matching debug executable first. It does not load operator configuration: it
passes only reviewed runtime/fixture selectors, synthetic identities and
NO_DOTENV, binds project-local temp/cache, bounds wall/idle time and caps test
threads at two (or a lower requested limit). It preserves the caller's working
directory and the child's exit code. Normal `cargo run` is unchanged. Its Windows
Job-based runtime tests remain unexecuted. Linux now has an authored dedicated
subreaper scope in the same process owner for the supported GNU x86_64 64-bit ABI:
orphan descendants remain its children across independent sessions, signals use
pinned process descriptors, and kernel child-absence checks precede cleanup.
Admission queries the actual SIGCHLD action and rejects automatic child reaping
(`SA_NOCLDWAIT`) or a nondefault handler before spawn. The orphan tracer uses
pollable pinned descriptors for death evidence; inspection errors are failures,
not proof of absence. This tracer and the isolated signal-action regression have
not executed. Darwin invocation still refuses before spawning: available unprivileged
tracking does not establish complete custody across unseen nested sessions and
exited ancestors. The explicit test config preserves this refusal instead of
running unguarded suites. This remains a pending cross-platform gate, not passing
acceptance. Application HTTP loopback checks and environment scrubbing alone are
not an OS network sandbox. The separate authored Linux
[network bootstrap](native-launcher.md#native-test-runner-network-boundary) creates
fresh private user/network namespaces with loopback-only interfaces before test
execution; its compiler and kernel/host evidence remain unrun. Windows lacks that
egress boundary, and complete cross-platform T006 acceptance remains open.

The additive `runtime-check [--extension FILE]` contributor command inspects an existing package
`.venv` through a fresh bounded copy of the same native executable. It does not
create an environment, install dependencies or activate a runtime. It removes
inherited Python home/import/launcher overrides, then runs the selected interpreter's
isolated standard-library `sysconfig` command before the private native probe.
Explicit UTF-8 and no-bytecode flags keep these subprocesses independent of ignored
Python environment overrides and avoid writes into shared runtimes. The native
probe rejects a package configuration whose patch version differs from the linked
interpreter before initialization, and verifies the actual CPython 3.11 executable,
prefixes, disabled user site, external codec versions and origins, and a tiny
Pillow RGBA roundtrip. The report includes the actual scripts/platlib paths,
native extension suffixes and Maturin availability. Scripts/platlib must agree
with the independently executed interpreter, not merely stay under `.venv`.
Optional `--extension FILE` loads a nonempty regular, nonlinked file inside the
owning project through Python's binary-extension loader. Its exact runtime suffix,
file origin, hash, `API_VERSION` and required callable APIs are checked without
importing the application's package initializer. This loads native executable code;
use only trusted matching project builds. A corrupt binary or Python-source file
is not accepted instead of the actual extension. The copied-artifact test is authored,
not executed, and does not prove repaired-wheel or clean-install compatibility.
A venv allowing shared sites is reported as such, not as a minimal
offline installation. The probe retains the existing `abi3-py311` contract using
a persistent program-name buffer before the first Python attachment; it cannot
replace the Python shared library already linked or loaded by the OS. It writes
redacted UTC `runtime-check`/`runtime-probe` logs under `logs/`, uses the existing
process owner with 10-second wall and 5-second idle limits per child, and refuses missing
or malformed package prerequisites. Its copied-binary/real-venv tracer remains
uncompiled/unexecuted. Production startup and codec workers are unchanged; actual
four-platform selected-runtime and minimal-package acceptance remain pending.

A separate contributor-only `numera-build [--install-maturin]` candidate now owns
the complete wheel/install/API/executable sequence in Rust. Keeping the builder
separate avoids holding the application executable open while replacing it on
Windows. It requires an existing `.venv`, installed Rust tools and the platform
linker; it never creates an environment or automatically installs a toolchain.
Only the explicit `--install-maturin` option permits installing the pinned build
dependency into that environment. The driver resolves installed compiler tools
before binding project-local build caches, preserves locked builds and finite
wall/idle limits, uses the same staged installation primitives, and checks the
actual extension and installed executable. Its logs use the existing redacted
UTC `build_native` format and contain child exit metadata, not raw child output.
The public build path currently refuses before starting any compiler, runtime
probe, dependency install or artifact write: approved signer/identity verification,
logo inclusion and multiartifact rollback are not integrated. There is no bypass;
`--install-maturin` does not override that refusal. Help remains available, and
missing Cargo still returns exit 2; the installation gate returns exit 1. Refusal
writes only the existing diagnostic log. Authored checks cover help, prerequisite
refusal, installation preservation and stop-on-first-failure, but none has run.
No successful native-builder run, repaired-wheel/platform acceptance, signing or
rollback has been verified. It is not wired into the installed launcher or package
workflow; the existing working installation and bootstrap remain unchanged.

The additive `source-inventory --baseline SHA [--output FILE]` contributor command
reads exact pinned Git blobs without importing source modules. The SHA must be an
exact 40-character revision; optional output is exclusive and physically inside
the owning project. It records declarations and conservative corpus-wide inherited
candidates, including relative imports, re-exported classes and source-qualified
method origins. Each discovered case also links to its defining source hash and
original assertion/control-context spans, using class definition identity rather
than a potentially rebound class name. The final callable declaration line must
also match: a later noncallable `runTest` replacement stays collected by the
original loader but cannot inherit assertions from the replaced function.
Missing method evidence remains unresolved.
Imported modules are never executed: decorators, import effects and dynamic
collection remain explicitly unresolved.
The immutable baseline lists all 277 original source identities and coverage keeps
1489 declared original cases pending. These counts are not migration acceptance:
complete discovery, independently authenticated target evidence and full package/
rollback gates are still required. This command is authored on the migration
branch and is not activated in the currently installed runtime.
Native test tracers reuse the original intent, resume, plan-step, similarity,
bot, roster, reconciliation and view fixtures without rewriting their expected
results. Candidate source-to-test mappings stay pending until their complete
assertions and actual execution are verified; a fixture hash is not regeneration
proof. Native-owned historical generator paths are also authored for intent,
resume, bot payloads, roster rows and whole packs, reconciliation, unbranded
views, plan steps, ingest, state artifacts, SQLite signatures/rewrites, archive
reports and gallery HTML, plus the whole similarity corpus and the original
catalog storage observation. Fixed Lottie validation replay admits the original
validator and actual exception-class source, then regenerates all 84 loaded-
document messages/classes. Supplied stat/magic/load ports are explicitly not
filesystem, gzip or renderer coverage. A distinct fixed replay invokes the
original loader and validator against the seven admitted real-file inputs using
external pathlib/gzip/io/json, with no supplied loader. All input hashes are
checked before historical definitions execute; the Base64 input is reconstructed
only in empty project scratch. Success removes the reconstructed file; observation
drift retains it for diagnosis. This does not regenerate the six comparison pairs
or prove renderer, IPC, arbitrary-file or full codec coverage. Both replays remain
unexecuted. The historical input document is independently pinned, including huge
integers, while changed expected messages or classes must fail source replay.
Shared-row replay also compares complete original
Markdown—including Python cell conversion, line splitting and alignment—with the
actual registered table binding, and checks the original keyword result.
Catalog replay uses the real historical SQLite and
path logic; supplied identity, native-prerequisite and writer callbacks remain
explicit stubs, not dedup, install or cross-language lock proof. A separate native
storage tracer checks the actual canonical lease and release. The same in-memory
source report also feeds a tracer through production-registered `NativeCatalog`
callables; native lease checks do not prove the original Python writer wrapper.
A further fixed replay admits eight original source identities and collects all
35 original catalog, manual-order and media-path test methods, including their
setup, teardown and assertions, through historical unittest/SQLite APIs. The order
tests retain their original Pillow 12.3.0 image helper. All temporary files,
including the original project-relative path fixture, stay inside the replay's
isolated workspace. The original Windows-only casing test must execute on Windows;
its exact original skip is reported separately elsewhere, never counted as parity.
Original ingest and near-hash policy run with explicit no-op prerequisite/writer
ports; unexpected decoder fallback refuses.
Separate native tests cover those storage behaviors, and exact connection-local
WAL/NORMAL/empty-order assertions stay in the catalog's private test module.
The 22 candidate mappings remain pending: this replay and the target tests have
not executed, and no source-retirement or full parity claim follows from them.
Nine original media-path cases also have pending native counterparts for relative
storage, external archives, moved catalogs, legacy conversion and Windows casing.
Those tests use isolated SQLite/media fixtures; conversion effects do not prove
parity of the original Python wrapper's conversion-count attribute. Four original
manual-order cases also have pending storage assertions for insertion, reversed
order, reopen/new-item placement and persisted metadata. None has executed.
Native lock counterparts now exercise stale, empty and rewritten diagnostic records,
normal and panic release, stable file identity, family-path normalization and
separate family/map leases. One same-binary subprocess tracer uses explicit barriers
for two and four competing holders, checks the actual holder PID in binding errors,
and exits with code 9 without Rust destructors before testing OS release. Existing
binding checks also assert real heartbeat timestamp advancement. These are authored,
unexecuted counterparts, not proof of the original Python context manager,
`LockBusy` subclass or forced-kill cleanup. All 33 lock candidate mappings remain
pending; the native Windows bucket includes the suite.
A separate five-source replay retains the exact original state-compatibility test,
its original wrapper and reference implementation, and the real registered native
bindings. It is authored to check descriptor cleanup, callback failure, exception
subclasses, atomic bytes and reference/native OS-lock exclusion in both directions.
The wrapper's own-process reference diagnostic remains a disclosed port. The same
replay separately selects four unchanged original PID-probe tests and their helper,
using private patch/import bindings rather than global modules or live probes.
Native refusal diagnostics distinguish present, gone and unknown observations:
POSIX uses signal zero; Windows uses a synchronization-only process handle and an
immediate wait. Neither observation proves process birth identity or permits lock
reclamation. OS acquisition remains the only ownership authority. Source admission,
replay, native diagnostic and platform tests remain unexecuted. The same replay
selects six original state-shape tests, preserving all ten validation inputs and
independently observing acceptance/refusal through the original validator. Native
counterparts call the existing public state binding; native `ValueError` and error
text remain explicit adaptations of the original `StateInvalid`, not exception
identity parity. No second state validator is introduced. The Windows source lane
pins CPython 3.11.9 and includes the shared source corpus suite; this is wiring,
not platform acceptance.
Atomic-write counterparts also preserve a foreign temporary file, UTF-8 values,
POSIX permissions and complete JSON from competing writers. The Windows refusal
case retains the old file and foreign temporary before a successful later write.
Additional private unit tests now inject serialization, sync and replacement
failures into the same writer, require two Windows refusals before a real rename,
and hold two distinct complete staging files before either replacement. The
public writer still selects real filesystem calls; no second persistence backend
or global fault switch is introduced. Serialization now accepts `Serialize`,
while existing JSON callers keep their types and formatting. These tests are
unexecuted. The same state replay also selects three original basic atomic JSON
tests against the original writer, with real workspace-confined temporary directories.
Native counterparts assert sole-target directory membership, successive JSON replacement
and previous done-list preservation after a real serializer failure. Native serialization
errors and the persistent Windows deadline remain explicit adaptations; full original
error/platform/runtime parity is unverified.
Logging counterparts now cover the six original pure redaction cases and the
original worker-secret key-name assertion. A fixed historical replay admits the
exact logging and test sources, selects those seven original methods, and supplies
27 source observations through the same bounded
worker and Git corpus. The native scanner now recognizes the original Unicode
decimal digits and removes standalone `bot` prefixes while retaining `/bot.../`
URL formatting. Literal secrets are ordered by Unicode code-point count rather
than UTF-8 byte length, with both registration orders checked by source replay.
Adjacent ASCII text still receives the existing, stricter native mask; that known
source difference remains explicit, not falsely reported as exact parity.
Separate authored file-log checks cover UTF-8/UTC, distinct files, redaction,
exit status and closed handles. The same admitted replay selects the original
formatter and recorded-exit tests with private state and real stdlib formatting.
It compares redacted payloads and stored/returned codes, not the complete Python
layout or exception hooks. Native event lines retain their component/message
prefix and append a per-run identity; finishing returns the recorded exit code
and a serialized warning/error/critical count snapshot. The original counter and
summary bodies also run through a private real logger with captured exit/shutdown
callbacks and a fixed clock; no global root logger or process-exit hook is installed.
Counts and outcomes are compared, not the source's complete layout or clock duration.
The native nonzero summary retains ERROR severity without counting itself again.
The replay also selects the original exception-hook body against private namespaces:
normal exceptions, named/unnamed threads, a real original-function traceback and
KeyboardInterrupt delegation remain observable without installing Python hooks.
Application, panel, worker and contributor-tool log owners install a redacting
Rust panic hook after creating their executable log. It holds only a weak log reference, never delegates
to the raw default printer, and falls back to redacted stderr when the file mutex
is busy or the log has closed. Six isolated child variants exercise that contract;
they are authored, not runtime-verified. Failures before log initialization,
abort-time cleanup and complete global-setup equivalence remain separate gaps.
Log labels follow the original ASCII allowlist and fallback before filename
allocation; ten admitted source observations and native file-path assertions
cover separators, Unicode, control characters and preserved dots/underscores.
These checks are also authored and unexecuted.
A separate three-source retention replay preserves six original call-time and
file-pruning methods, 19 numeric observations and two real-file pruning
outcomes. The native counterpart resolves `EMOJI_LOG_RETENTION_DAYS` after
configuration loading: 30 days by default, 0 disables pruning, malformed values
warn and use the default, and negative values warn and clamp to 0. It selects
only direct regular log files by modification time; linked entries and unsupported
clock ranges are retained rather than guessed. Main application commands and the
panel use this policy; private bootstrap/worker log initialization does not prune.
An isolated copied-command tracer covers help/error exit codes, configured pruning,
arbitrary `--token-env` values and the three worker-secret names without operator
data. Custom credential registration occurs before argument validation for every
application command, so rejected input is redacted too. This adapts the original
Python CLI assertion rather than executing that original CLI. None of these new
source or target tests has executed. A synthetic committed-secret counterpart
now preserves selected keys, Unicode length, tracked-file enumeration, the 2 MiB
limit and key/path-only diagnostics. Its original assertion replay retains deliberate
failure outcomes rather than accepting skips; native enumeration also uses a tiny
isolated Git fixture. No operator configuration is read by those tests. Full
formatter/exception-hook behavior, complete CLI equivalence, original scanner skip
variants and global logging setup remain separate acceptance gaps.
The native configuration loader also preserves one leading UTF-8 BOM, the original
line boundaries and first-value/environment precedence. Its isolated tests and
fixed four-source configuration replay include invalid UTF-8 refusal without reading
operator configuration or changing the parent environment. The same replay retains
four original numeric-configuration methods and 24 source-generated value/warning
observations through a disclosed private-environment patch port. A shared native
decimal policy handles Unicode digits, underscores, signed bounds and normalized
warnings for configuration, retention and codec timeouts without initializing Python.
The historical 4,300-digit default is retained; custom interpreter digit limits
are not supported, and codec/retention values outside native duration ranges refuse.
These source and native tests remain unexecuted. The replay also retains five
original batch exit-code tests and independently observes their four input pairs.
The three collection commands share the existing success/partial/failure policy
(`0`/`3`/`4`); usage remains distinct (`2`). An eleven-source, hash-admitted replay
selects the three unchanged original local-ingest tests and their real static
conversion, fingerprint, SQLite and storage implementations. Its independent
stdout, exit codes, catalog rows and stored bytes feed a copied `add-media` command
tracer; a dedup repeat also checks stable rows/media and cleanup. Logging, environment
loading, native prerequisites and historical writer ownership remain disclosed no-op
ports. The same fixture adapts the two original real-image command cases: public
artwork ingest/repeat and an undecodable rival must preserve the prior catalog row,
file-unique ID and incoming PNG. Original conversion/fingerprint produce independent
bytes and pixel hashes; those two original subprocess wrappers are not replayed.
A transparent 120×90 RGBA input also must fail without a catalog item or stored
media. An invalid near-duplicate threshold must return usage code `2`, identify it
as out of range and leave the requested data directory absent. The target retains
the current codec bridge, so this is not Python-free package acceptance. Source
replay and target tests remain unexecuted.
A fixed benchmark-policy generator observes the original paired medians, input
boundaries, logging order and catch policy using supplied clock/database/target
ports. Those observations are not measured performance, real SQLite acceptance
or native-pair execution proof. A separate isolated loopback tracer exercises the
actual registered announcement binding: direct cards/list forms, normalized numeric
destination, Worker JSON, nondefault timeout input, HTTP refusal and disconnected-response
uncertainty. It requires nine exact requests with no retry/fallback and preserves diagnostics
on any failed assertion. The shared decimal policy also preserves destination signs,
Unicode digits, underscores and leading-zero normalization. The configuration replay
retains four original resolver tests and 22 independent source observations; the separate
shared-publisher import-identity assertion remains unmapped. All are authored but
unexecuted; retained Python-wrapper return, exact error text and API-base normalization
are not proved by the registered binding test.
A native public-command tracer also compares all five original preflight outcomes against historical
report output, including exact stdout, exit codes and absence of publication.
They accept only
exact hash-pinned source slices from Git and independently pin input recipes
(or regenerate the complete original corpus), using CPython 3.11.9 in an owned
bounded child. Generated Markdown, callback order, signed integers and real
public-art gallery output remain observable contracts, not native-derived
expectations. These paths are not production fallbacks or a complete generator
migration, and they have not yet executed.
The same branch adds contributor `package-native --executable FILE --wheel FILE
--dependencies DIR [--out DIR]` and read-only `package-verify`. Packaging uses the
owning checkout's exact Git revision and its existing schema1 runtime allowlist;
verification checks platform, Python 3.11, wheel consistency and file hashes.
These commands do not build, install, activate or retire source. Their new native
CLI tests have not yet run, and the current source-containing package layout is
not the final first-party migration artifact.
The contributor `benchmark-similarity [--catalog FILE]` command is also authored,
but unexecuted. It requires this Git checkout's pinned historical reference and
exact CPython 3.11.9. Without `--catalog` it uses the original 1500 seeded hashes;
a supplied catalog is opened read-only. Five paired measurements include native
binding overhead and require exact order and candidate parity before reporting
medians; inputs of at least 100 hashes also refuse a slower native median. Work
runs in the existing owned child with 120-second wall and 15-second idle ceilings,
not through a Python application import or a second backend. New UTF-8 UTC logs
use the existing `logs/` convention; no measured speedup or installed availability
is claimed. Its actual binary/read-only tests remain pending hosted execution.
The additive contributor command `native-convert-owner --in DIR --out DIR
--stdout FILE --stderr FILE` exposes the existing native converter watchdog.
All four options are required. It bypasses operator configuration, logs through
the existing UTF-8 UTC logger and requires an already installed native runtime;
it does not build or install one. The existing marker-based 120-second idle and
3600-second wall ceilings retain exit codes 124 and 125; cleanup failures refuse
success. Output and capture paths remain project-contained. Help, usage and
missing-runtime tracers are authored but unexecuted; actual conversion, wrapper
replacement and four-platform acceptance remain pending. The existing launcher
continues using its accepted path until those gates pass.
Additive `coin-alias-map` and `coin-enhance-map` commands now wire the existing
native policies to canonical map leases, atomic writes and inventory refill.
`--coin-dir DIR` selects isolated input/output; default is project `coins/`.
They refuse unknown flags and non-string IDs and perform no network calls.
Additive `coin-build-keywords` shares the native CoinGecko name/CSV policies,
scans configured coin logos with SVG precedence and preserves partial-lookup
exit status. Help/invalid-flag paths never contact providers; full live command
and installed-platform acceptance remain pending.
Native CoinGecko logo-fetch policy additionally maps good-cache resume,
corrupt-cache refresh, validated private download publication and final keyword
sweep. Its current fixture seam uses real image bytes with fake HTTP/virtual waits;
additive `coin-fetch-logos [pages] [--coin-dir DIR]` connects the pooled native
HTTP transport. Default 40 pages, invalid arguments fail before networking, and
paging failure exits 3. Full live command, errors and platform parity remain pending.
Additive `coin-fetch-paprika` / `coin-fetch-cmc` connect native admission,
search/cache, image conversion and shared publisher. Exact `--dry` publishes
and downloads nothing; Paprika supports `--retry-unmatched` and persists answered
searches. Unknown searches remain partial; live runs require the token before
provider work. They are not authorized live-test commands and full end-to-end,
source-case and platform acceptance remains pending.
Additive `coin-write-manifests` reads live coin sticker IDs through the existing
Bot API client and writes per-pack Markdown plus `manifests/INDEX.md`. It accepts
`--out-dir`, `--state`, `--map` and `--token-env`; default inputs/output are in
project `coins/`. State/map reads hold family-before-map leases, report writes are
atomic, and an unknown live read leaves prior reports untouched. Custom token
variables are redacted even without a secret-shaped key name. Full source error,
primitive coercion, concurrent-output and installed-platform acceptance remain pending.
Additive `coin-check-packs [--coin-dir DIR]` reads current live sets, downloads
only uncached/failed images and checkpoints `pack_audit4.json`; the text report
is `pack_audit4_report.txt`. Deleted IDs leave the totals; decode failures remain
retryable errors, not blanks. Duplicate groups require distinct live IDs with the
same normalized pixel hash. Exhausted set listings preserve progress and refuse
a partial report. It makes no Telegram mutation; full original-case/error,
process-death and platform acceptance remain pending.
Additive `coin-remap-ids --emoji-dir DIR` matches 16px RGB-on-black plus alpha
signatures by content. It reads live sets and checkpoints the manifest-keyed
cache; it never changes Telegram packs. `--apply` requires finite calibrated
`--max-distance > 0`; `--min-margin` defaults to that cutoff. Incomplete or
ambiguous work writes `--candidates`, not the canonical map. Accepted replacements
hold the family lease across the live read and the map lease across old-map
backup/replacement; an existing `.prebroken.json` is never overwritten. `--state`,
`--cache`, `--out`, `--candidates` and `--token-env` select isolated inputs/outputs.
Dry mode still updates the signature cache, not the map. Numerical calls use the
unchanged external NumPy API; 2.4.6 is the latest Python 3.11-compatible release
(2.5.4 requires Python 3.12+). Full original-case/BLAS/platform/crash acceptance
and installed activation remain pending.
Additive `coin-verify-logos --emoji-dir DIR` defaults to review only (`--top 100`,
`--threshold 20`). Inverted/white-flattened comparisons tolerate deliberate
restyling; flags are not proof of wrong art. Repair requires exact `--fix --only
 a,b`, token and positive owner ID. It reuses the native replacement intent,
private original/prepared staging, immutable upload bytes and verified-retry
transport. Unknown confirmation preserves intent/source and stops further fixes;
map/state targets must match recovery. `--map`, `--state`, `--token-env` retain
isolated input selection. **Repair can mutate Telegram packs**; no live testing
is authorized by the command or checkpoint. Full original-case/provider/CLI,
crash/platform and installed acceptance remain pending.
Additive `coin-rebuild-map` reads `rebuild_dedup_plan.json` and validated
`rebuild_dedup_state.json` beneath `--coin-dir` (default `coins/`), with source
images beneath `--emoji-dir` (default `logos/emoji` in that directory). It holds
family then canonical-map leases across the entire snapshot, live downloads,
alias replay and atomic map/inventory writes. Every live ID and recorded rebuild
or provider source must have exactly one content match; order is never position.
Lost aliases or unreviewed shared groups larger than 20 refuse replacement and
write `ticker_to_id.candidate.json`. `--token-env` selects the read-only token.
An absent/corrupt frozen plan refuses; this command makes no Telegram mutations.
The branch also implements `coin-rebuild [all|build|map|links]` with the same
path/token options. Default `all` builds, maps and sends final links; **both `all`
and `build` can delete old packs and upload**. They generate a missing frozen
plan, never regenerate a corrupt one, confirm each old deletion by live readback
and persist upload intent before transport. `links` deliberately resends the
combined index; `map` only reads Telegram. These latest rebuild paths are authored
but not yet compiled or runtime-verified. Full error/crash/platform/source-case
acceptance remains pending; do not use this branch for live migration testing.
Current prebuilt installations do not yet include these expanded commands.
Full failure/platform and original-case parity remain pending.
These slices use
pinned source-generated fixtures. The internal `codec-worker` entry point is
not yet the production media adapter. Its limited hosted checkpoint is build
and seam evidence only, not complete parity, a new installable release or
permission to remove the old source. Existing installed packages remain active.

## Ready-package installation

Download a matching archive and checksum from a successful **Prebuilt packages**
workflow run, or release assets after every platform job succeeds. See
[README installation](../README.md#install-a-ready-built-package--no-local-rust-build).
Windows x64, Linux x64 (glibc 2.39+) and macOS 15+ Intel/Apple Silicon are the
package targets. The bundled offline wheels require Python 3.11. FFmpeg/FFprobe
on PATH are required for video; Rust/Cargo and C++ build tools are not end-user
prerequisites. The older v2.0.0 source ZIP is not a prebuilt runtime.

The archive includes the executable, matching abi3 wheel, binary dependency
wheels, retained coin tools and public assets. `scripts/install_prebuilt.py`
verifies manifest/platform/hashes and installs offline with `--only-binary=:all:`
into its own virtual environment. It refuses an existing environment before
changing it. Extract updates into a separate folder; preserve your configuration,
collection, archives and recovery backups before switching installations.

```powershell
py -3.11 scripts\install_prebuilt.py
.venv\Scripts\python.exe scripts\launch_prebuilt.py --help
.\run.ps1 -Check
.\run.ps1
```

```bash
python3.11 scripts/install_prebuilt.py
.venv/bin/python scripts/launch_prebuilt.py --help
.venv/bin/python scripts/launch_prebuilt.py panel
```

Copy `.env.example` to `.env` and supply your own configuration before Telegram
commands. `scripts/launch_prebuilt.py` selects the package interpreter and root;
Windows retains a private Job until its child exits. Temp/cache paths stay under
`logs/tmp/` and `.cache/`; linked/reparse escapes refuse. Neither the menu nor
doctor invokes Cargo or falls back to retired Python application modules.

## Native command surface

Pass these commands to `scripts/launch_prebuilt.py` with the package interpreter,
or invoke the installed `native/runtime/numera-emoji` executable (`.exe` on
Windows) with the appropriate package Python environment:

| Responsibility | Commands |
|---|---|
| Convert and build prepared images | `make-emoji-pngs`, `build-pack` |
| Collect and publish a catalog | `add-media`, `fetch-pack`, `fetch-emoji-ids`, `build-collection` |
| Curate and apply saved intent | `panel`, `plan-status`, `plan-apply`, `sync-order` |
| Reports, archives and recovery | `status`, `pack-manifest`, `pack-archive`, `identity-repair` |
| Premium emoji ID bot | `emoji-bot` |

Individual commands expose `--help`; `emoji-bot` uses environment configuration
instead of command options. `plan-apply` and `sync-order` require `--apply` for
live changes; `identity-repair` supports `report`, `migrate-video-keys` and
`restore --bundle PATH`, with `--apply` required for migration/restore writes.
`pack-archive --export PACK --zip PATH` produces a copy-only ZIP. Existing JSON
schemas, content keys, published IDs, saved intent and configured logo policy
remain compatibility contracts. Commands are not permission to mutate live packs.

Rust owns SQLite, catalog/dedup decisions, transport and mutation proof, application
orchestration, HTTP/view/save/cache policy, state/filesystem operations and reports.
The JavaScript panel and Cloudflare Worker retain their existing stacks.

## Retained Python responsibilities

Three categories remain explicit; they are not all media-only:

1. **Exact media/identity:** `media_bridge`, `media`, `media_lottie`, `identity`,
   `video_decode` and `repaint` preserve Pillow/SVG/Lottie/FFmpeg bytes, pixels and
   durable identity. The bridge never runs a collector, catalog, publisher, bot,
   panel server or report application. Rust owns preview cache/admission/warm-up;
   Python renders only the supplied codec request. Codec children receive validated
   non-secret settings such as `EMOJI_FFMPEG_TIMEOUT`; dotenv and credentials are
   disabled in those children.
2. **Thin/shared coin bindings:** `telegram_api`, `announce`, `packstate` and
   `pack_rows` call Rust for transport/retries/upload proof, announcements,
   leases/atomic replacement and Markdown tables. Shared configuration, logging,
   error types and similarity exports remain for the unchanged coin-domain tools.
   Requests serves those provider tools; NumPy is only needed by coin remapping.
3. **Development/reference support:** sandbox cloning, bounded SQLite snapshots,
   path/state enumeration and build/install/process-owner harnesses remain Python.
   Pinned source oracles and relocated references stay under `tests/`, excluded
   from runtime packages. They are regression/rollback evidence, never production
   fallback or a second application dispatcher.

Retained coin-provider tools continue as `python -m coins.<tool>`. Public exception
classes and `add_sticker` results remain source-compatible: an integer after
verified recovery, otherwise `None`. An unanswered mutation without sufficient
proof refuses rather than blindly retrying. Worker announcements are not retried
or silently sent directly after an unknown outcome.

## Developer installation and isolated consumers

Only contributors changing Rust need Rust 1.99+, the platform linker and an
explicit source build:

```powershell
.venv\Scripts\python.exe scripts\build_native.py
```

The locked build installs the extension and executable under `native/runtime/`,
separate from disposable `native/target/`. Repaired wheel shared libraries remain
beside the extension. Staged hash-verified replacement preserves a working binary
on failure. Executable, matching extension/libraries, codec dependencies and assets
are all required; this is not a standalone-binary distribution.

`scripts/panel_sandbox.py` owns a copied catalog and runs the installed native
panel with scrubbed credentials, a strict argument allowlist and a lifetime lease.
Windows uses a private kill-on-close Job; Linux/macOS inherit the same locked file
description and monitor wrapper PID/birth identity. An unrelated listener is never
adopted. `coins/run_convert.ps1` invokes native `make-emoji-pngs` for SVG and PNG;
its owner stops the process tree after 120 seconds without marker progress or a
3,600-second wall ceiling. Ordinary failures are not silently retried.

## Verification status and limitations

Final integrated core [run 38174075123](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/38174075123)
at `0811e9b` passed all six jobs after production retirement: Python 3.11/3.12/3.14,
Windows native safety, the native-backed browser suite and Worker checks.
Four-platform [package run 38173007469](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/38173007469)
at `c010464` passed actual extracted installation, CLI, exact media, retained coin
manifest and lifecycle checks. The later `0811e9b` changes only the lock-test source
inventory; production/package inputs are unchanged. Tag publication was skipped.

Copied real catalog readback and private-copy native migration/restore/source rollback
have separate local evidence. After removing disposable build and verification
extractions, the retained Windows installation passed its actual doctor, native CLI,
real 100×100 PNG conversion and coin entry-point checks. Protected input and runtime
hashes were unchanged; no Cargo build or live Telegram mutation was used. Recovery
copies, source history and required test fixtures were preserved.

Persistent acceptance includes source-generated fixtures, actual CLI/loopback
cases, native-server-backed browser suites, exact PNG/TGS/VP9 codecs, cross-language
locks, seven persisted migration process-death/restart boundaries and save-time
competing ownership. Synthetic browser thumbnails alone do not prove codec parity.
Optional private fixtures remain separate from public CI coverage.

Version2 rollback uses WAL-safe snapshots, complete media/state intents, signatures
and journal replay. Changed media, later catalog edits or a different active bundle
refuse rather than overwrite. SQL REAL values refuse before writes: the original
catalog schema uses TEXT/INTEGER and no supported producer requiring REAL was
established. Arbitrary extension-table/producer Unicode-version compatibility is
not certified. This limitation is not evidence that normal catalog rollback failed.

All three hosted workflows are manually dispatched. Pushes, pull requests and
version tags do not start them. Full native acceptance keeps Cargo's automatic
library, binary, integration-test and doctest discovery; a persistent policy test
rejects disabled discovery or replacing that step with a selected target list.
The migration checkpoint remains explicitly partial. Changing these declarations
is not evidence that the new workflow or authored tests have executed.

Native logs are new UTF-8 UTC files under `logs/`, with secret/token redaction.
Preserve real operational logs and unresolved diagnostic/recovery evidence. A
separate historical verification-receipt association remains unresolved; current
CI cannot retroactively identify it. Final closure is not claimed here.

For rollback, preserve a verified pre-retirement application version from Git
history and your data/configuration backup. Test restoration on an independent
copy with that version's required dependencies before changing a live installation.
A native-produced rollback bundle is checked by retained source references; those
references are not a supported end-user application. Never restore over unrelated
later data changes or reintroduce a silent Python production fallback. Cleanup
waits for integrated acceptance and preserves data/media/archives, required
fixtures, recovery evidence, Git history and one verified current installation.
