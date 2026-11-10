# Native launcher and contributor tools

[Complete guide](GUIDE.md) · [Native backend](native-backend.md)

The Rust launcher and contributor tools are compiled and exercised by automatic
CI. The public CLI and panel use the same executable's Rust codec worker;
`numera-build` builds and installs the matching wheel/executable, and
`install-prebuilt` installs a verified archive offline into a fresh environment.
See [current CI acceptance](rust-ci-status.md) for revision-specific platform
results and remaining codec/compatibility dependencies. The detailed checkpoint
contracts below do not replace that current validation record.

## Menu and foreground commands

`numera-emoji menu` accepts no options except `--help`. It preserves the fourteen
existing choices, one-step Back (`0`), Quit (`exit`/`quit`), and EOF cancellation.
Ordinary confirmations default to yes; legacy coin-pack deletion, reorder apply
and archive sync default to no. A failed conversion, dry-run, report or preflight
prevents the dependent mutation. Exit3 remains partial completion, not success.

Actions invoke the installed native executable with argument arrays, the owning
project directory and project-local temp/cache settings. They do not invoke a
Python command wrapper, install prerequisites or compile Rust. Missing runtime
files refuse the action. The menu's D1 choice launches `numera-emoji check` in its
own process; it does not replace all checks with a Cargo-only subset.

Foreground children inherit stdin/stdout/stderr. Redirected menu input reads no
ahead beyond the current line, so it cannot steal a child's confirmation or the
following menu choice. A Windows console child is assigned to the existing
kill-on-close Job while suspended, before execution. Headless automation remains
hidden. POSIX children use the existing private-session accounting, not the
parent's foreground process group.

While a child runs, a scoped Ctrl+C handler records cancellation. POSIX forwarding
rechecks session membership and birth identity; Windows console delivery already
reaches the child. After at most two seconds for graceful shutdown, the existing
bounded owner terminates/reaps the tree and verifies quiescence before scratch
removal. Cancellation reports130, never completed work. Unknown execution or
cleanup stops the menu rather than starting another action. User-driven actions
have no arbitrary application/idle timeout; automated tests retain explicit outer
wall and idle limits. Abrupt POSIX wrapper death, hostile session escape and real
console behavior remain unverified acceptance gates.

## Non-interactive doctor

`numera-emoji doctor` never installs, builds or contacts Telegram. It checks the
installed executable's startup, a fresh selected-runtime probe, real external
codec imports and an actual loaded native extension. The extension is loaded
without importing the application package. Required evidence that is absent or
unknown fails; missing video tools, optional NumPy and unset configuration key
names produce warnings. No configured values are printed.

Runtime selection reuses the same package `.venv` policy and independent
interpreter `sysconfig` observation as `runtime-check`. An environment-variable
selection alone does not prove the embedded interpreter chose that environment.

## Transitional source-check orchestration

`numera-emoji check` owns the **retained Python checks**, not completion of test
migration. With no options it verifies the selected compatible environment, then
uses the original ordered `find_spec` prerequisite check for NumPy, Ruff, the
native extension and, unless explicitly opted out, Playwright. This locates each
target without importing it, but imports the dotted extension's parent package.
Actual Python truthiness controls short-circuiting; missing or failed discovery
returns exit2 before any source-check stage. It does not require the doctor's
codec or Requests imports. After admission it runs these steps in order:

1. Git-selected current Python source files through `compileall`.
2. The existing linter as `ruff check .`, without rule overrides.
3. `unittest discover -s tests -t . -p test_*.py` with explicit UTF-8 mode.

`--tests NAME[,NAME] ...` selects package-qualified unittest names; one optional
`tests.` prefix and `.py` suffix are normalized. `--skip-compile` avoids source/Git
discovery and `--skip-lint` avoids linter resolution; neither bypasses the selected
runtime and ordered module-discovery prerequisite probe. `--timeout-seconds N` sets a positive
32-bit per-step wall ceiling; the idle ceiling is the smaller of N and180seconds.
The default wall ceiling remains1800seconds. `--python FILE|NAME` selects an
explicit compatible venv interpreter by absolute/project-relative path or the
scrubbed inherited `PATH`. It preserves the final POSIX interpreter symlink and
refuses linked ancestors or Windows Store aliases. The independent `sysconfig`
probe must report the exact configured patch, and its paths must agree with the
fresh embedded probe before any source check starts. Borrowed Windows runtime
keys such as `Path` are canonicalized without admitting additional credentials;
POSIX key spelling remains case-sensitive. The venv must use CPython3.11 with the exact linked patch;
arbitrary base interpreters and other Python versions remain unsupported, unlike
the original PowerShell override. No environment is created. These command
changes and the three-original-guard subset tracer are authored but have not yet
been compiled or executed.

The `-t .` hermetic guard remains mandatory for discovery. Explicit names stay
under `tests.` and use only the physical owning root for the unit-stage import
path. Credentials are scrubbed using the existing native test-environment policy;
output is captured and owned-process cleanup is required.
Windows uses its existing Job owner and Linux the dedicated subreaper owner;
Darwin refuses before checks because nested-session custody is unverified.
The existing explicit browser-test opt-out is preserved; it is not enabled by
default. Prerequisites are not installed automatically. Full native test migration,
OS network isolation, browser-driver migration and package acceptance remain open.

## Native test-runner network boundary

The explicit Cargo test runner now has an authored Linux-only bootstrap before
Python, asynchronous runtimes or threads start. It creates a fresh private user
and network namespace for every invocation, including nested runner calls. The
namespace admits only its own IPv4 and IPv6 loopback interface; inherited
nonstandard descriptors are closed before the admitted test executable replaces
the bootstrap in the same process. The existing descendant owner retains the
root process identity and exit status. Setup failure returns exit2 without
starting the target; there is no unisolated fallback or host network reconfiguration.

This is a network-only candidate for GNU x86_64 LP64 Linux, not filesystem or
Unix-domain broker isolation. Raw TCP/UDP loopback, non-loopback refusal,
separate-session descendants, inherited descriptors and setup refusal have
source-authored assertions in the existing runner suite. Neither compilation nor
kernel/host enforcement has been tested. User-namespace restrictions, IPv6 and
required kernel capabilities can refuse setup and remain platform acceptance gates.
Windows still uses its existing Job without this OS egress boundary. Exact macOS
numeric-loopback isolation and Darwin descendant custody remain unresolved.
The retained Python `check` stages above do not acquire this bootstrap merely by
using the same process owner. Full cross-platform isolation is not complete.

## Compiled-case observation candidate

`numera-emoji test-runner --observe MANIFEST EXECUTABLE [LIBTEST_ARGUMENTS]`
is an explicit parent-only opt-in. It does not change the ordinary runner or
activate reporting automatically through Cargo. The manifest and executable must
be absolute project-contained paths: the input JSON under `logs/tmp/` and the
binary under `native/target/`. The manifest binds schema1, claimed target/baseline
revisions, a complete Cargo `compiler-artifact` object and a public source,
target, fixture, support and configuration file closure with SHA-256 digests.
`native/Cargo.toml`, `native/Cargo.lock` and `.cargo/tests.toml` are required members.
The manifest's claimed revisions and build metadata are not authenticated receipts.

The same admitted binary, environment, working directory and descendant owner run
one guarded list-only invocation followed by one actual suite invocation. Pinned
Rust1.99 libtest's stable but deprecated `--logfile` option supplies separate
listing and result sidecars; raw `--nocapture` output is never pass evidence.
Observation mode admits control-free filters, `--exact`, repeated `--skip`, ignored
selection, `--test`, workers1–2, pretty/terse formatting, color, capture and
show-output options. Caller logfiles, list/help/benchmark actions, unstable modes
and conflicting options are refused rather than silently changed. The ordinary
runner retains its existing argument forwarding and worker clamp.

Each invocation exclusively retains `logs/test-observations/<random-id>/` outside
the suite workspace. Listing has at most30seconds; admission/listing time reduces
the existing900second suite wall budget, with at most180seconds idle for execution.
Hash reads are byte-bounded but synchronous, not independently interruptible;
the invoking process still needs its existing outer wall/idle owner. Binary and
closure hashes are checked before and after owner quiescence. Missing, duplicate,
unlisted, malformed or partial results refuse a completed report. Ignored is not
passed; exit0 without at least one actual `ok` result is refused. A valid failed
transcript may produce a diagnostic report, but cannot replace the child's
nonzero exit with success. Observation failure changes an otherwise successful
suite exit to1. Uncertain custody retains the suite workspace and diagnostics.

A finished store contains `observation.json`, `listing.log`, `dispositions.log`,
`results.log` and `manifest.json`. Only the first three are candidates for a
separately reviewed public artifact allowlist: raw results can contain private
failure/ignore reasons, and the copied manifest can contain absolute build paths.
Never upload the entire store. The report says `result=observed`, `trust=untrusted`
and `release_ready=false`; `completed` describes transcript consistency, not
semantic parity, exhaustive source discovery or authenticated CI provenance.
Exact Cargo metadata, runtime identity, checkout bytes and GitHub run/job/artifact
provenance still require independent authentication. This option and its assertions
remain uncompiled and unexecuted; no pending migration row or retirement gate changes.

### Same-Cargo metadata admission

`numera-emoji test-runner --observe-cargo PLAN BUILD_LOG EXECUTABLE [LIBTEST_ARGUMENTS]`
adds a finite adapter to that same observation path. `PLAN` is an absolute
`logs/tmp/*.json` file prepared **before** building: schema1, target/baseline
revision claims, package ID, the same hash-bound file closure, and explicit target
entries containing `name`, `kind`, project-relative `src_path` and `mode`.
An observed target uses `mode=observe` without a reason. The two known application
binary harnesses may use `mode=noncoverage` with an explicit reason. Two exact
source-hash-bound integration harnesses also use it only on their inactive host:
`sandbox_lease_windows` on Linux GNU x64 and `tooling_test_cohort` on Windows MSVC x64.
Changed source bytes revoke that classification. These targets still run once
through the ordinary guard, remain in the artifact inventory and claim no passed
cases. Unknown targets and arbitrary empty/filter selections never fall back.

`BUILD_LOG` must be an absolute `logs/tmp/*.log` file receiving stdout directly
from the **same** Cargo invocation with `--message-format=json`; keep stderr
separate. A combined diagnostic stream, tee pipeline or fabricated artifact object
is not that contract. The adapter reads at most8MiB through the first complete
successful `build-finished` record, selecting exactly one test-profile artifact
for the physical executable and declared package/target/source. Later runtime
stdout is not parsed as build metadata. Input files, malformed/duplicate records,
missing/failed boundaries, ambiguous artifacts and closure drift fail closed.
Cargo retains its own scheduling, working directory and external-runtime setup;
this is not a direct-binary loop or another build/test invocation.

The adapter retains a unique `logs/tmp/cargo-observation-<id>/` with the admitted
`manifest.json` and an untrusted `binding.json` for the plan, metadata prefix and
artifact hashes. These are diagnostic inputs, not authenticated coverage receipts.
The binding and observation outputs have not been joined to verified CI provenance.
The pre-build producer below is source-authored; automatic workflow integration
and its runtime proof remain pending. The main Linux and native browser workflow
steps now select the wrapper described below, but have not executed it. The
synthetic runner fixture verifies only the authored adapter contract,
not a real Cargo build.

Rustdoc-generated test executables are not ordinary compiler-artifact libtest
harnesses. Their source association, project-contained temporary output and owned
leaf execution remain unresolved; this adapter does not disable doctests, widen
allowed roots, invent metadata or count them as passed. Both observation options,
including their compiler/platform/cleanup behavior, remain uncompiled and unexecuted.

### Pre-build plan producer

`numera-emoji test-plan --baseline SHA --output FILE [--lib] [--bin NAME]... [--test NAME]...`
authors that pre-build plan before the original Cargo test invocation. The output
must be a new JSON file under `logs/tmp/`; existing plans are never overwritten.
The exact lowercase40hex baseline is retained as a claim, not proof that every
current source byte matches that revision. The current HEAD and actual worktree
file hashes are separate observations.

The producer uses one bounded offline `cargo metadata --no-deps --format-version=1`
call and the existing owned Git/capture helpers. Package ID and targets come from
the actual physical native manifest/workspace, not executable names or a guessed
package. No selectors means the current default library, binary and integration
test set; exact `--lib`, repeated `--bin` and repeated `--test` selectors form a
union. An exact selector may include a target with `test=false`: that flag controls
default selection, while the actual compiler artifact must still have
`profile.test=true`. Missing or nonboolean target flags are refused. Unknown targets,
globs, feature switches, benchmarks, custom harnesses and other unimplemented modes
are refused, not approximated. This command does not build, install or execute a
test suite.

Only tracked stage0 regular public inputs in finite source/fixture/assets/docs/
configuration scopes are considered. Protected paths are excluded before reading
content, and relevant unsupported public files refuse the plan rather than being
silently omitted or relabeled. Selected target entry points must be indexed and
present. The two tracked native Unicode tables consumed by `include_str!` are
included as exact support paths; unrelated or private data files are not admitted.
The root public `.env.example` template, also compiled into a tooling test, is an
exact configuration input; real, nested or suffixed environment files stay excluded.
The same existing file policy and bounded SHA-256 reader hash admitted inputs
before metadata, then recheck their bytes, index and HEAD. Per-file8MiB,
aggregate64MiB and4096member ceilings remain. An exclusive retained staging directory
contains an untrusted input-binding record; publication uses the existing atomic
no-overwrite writer.

This is a **pre-retirement** snapshot, not an exhaustive dependency/runtime
manifest. Untracked authoring files, generated runtime inputs, external libraries,
private real-data selectors and source-baseline membership require separate
receipts. The current schema still requires retained original sources; it does
not add dummy sources or authorize source deletion after retirement. The producer,
entry-point refusal assertions and all actual metadata/process/filesystem behavior
remain uncompiled and unexecuted. Full workflow and source-retirement acceptance
is still pending.

### Same-invocation Cargo candidate

`numera-emoji test-cargo --baseline SHA -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked [CARGO_SELECTION] [FILTER] [-- LIBTEST_ARGUMENTS]`
is the additive orchestration candidate. It derives the pre-build plan from that
same finite argument vector, writes an exclusive literal runner configuration and
executes **one** original Cargo test invocation. Supported selection is no selectors,
`--lib`, repeated exact `--bin NAME` and repeated exact `--test NAME`, optionally
`--no-fail-fast`. Arbitrary configuration, alternate manifests, cross targets,
features, benchmarks, `--no-run` and unsupported libtest modes are refused.
A plan containing only noncoverage harnesses refuses before Cargo starts.

Invoke from the physical project root. Only native Windows x64/MSVC and Linux
x64/GNU have an admitted descendant owner; macOS remains blocked. On Windows the
contributor driver must already be copied to an owned project-local path outside
`native/target`, so Cargo can replace its binary without overwriting the running
driver. This candidate does not install or activate an application.

The existing configured runner is overridden by an absolute, literal five-element
argument array for the admitted host triple, not an interpolated environment
string. Cargo receives `--message-format=json`; stdout goes directly to a new
`logs/tmp/test-cargo-<id>/build.log`, stderr to `stderr.log`. No tee, pipe, combined
stream, second build or direct-binary scheduler is introduced. Both full streams
retain the existing8MiB capture cap; the adapter's separate8MiB prefix cap is not
an unlimited-log promise. File-size activity on either stream supplements the
existing process-tree activity without restarting wall/idle clocks. The original
900s wall/180s idle bounds and verified descendant shutdown precede reading.
Source-authored contract tests now exercise each stream's overflow and invalid
UTF-8 retention, file-progress idle extension, an unchanged wall deadline and
callback refusal through the same owner. A dedicated sequential driver avoids
parallel Linux subreaper ownership; a live unchanged-accounting witness separates
supplemental progress from CPU/Windows Job IO. These tests and their timing limits
are unexecuted. Full library discovery selects them; filtered checkpoint commands
do not claim that coverage.

The plan, runner configuration and both raw logs remain local on success or
failure; they are not uploadable as a directory. Console logging reports only
bounded metadata, not foreign diagnostic bodies or compiler paths. Cargo's real
status is preserved after verified shutdown. A fail-closed postflight reconciles
actual compiled targets with the plan and requires completed, hash-bound reports
for every observed target before returning success. Different configured targets,
missing or bypassed runners, duplicate artifacts and absent completion records
cannot be accepted from Cargo exit0 alone. Marker text only locates receipts;
manifest, binary, listing, result and disposition identities must agree. These
checks remain untrusted observations, **not** authenticated case coverage or full
migration acceptance. The main Linux, native browser and Windows acceptance steps
select this wrapper; Windows uses an exclusive copied driver outside the target tree.
The migration checkpoint's existing fourteen bounded test commands also select
the wrapper with unchanged filters and ordering; package acceptance remains ordinary
Cargo. These workflow declarations have not been dispatched or runtime-verified.
Its source assertions, actual Cargo invocation, platform behavior and cleanup
remain uncompiled and unexecuted. The unchanged Cargo doctest phase is not silently
disabled: ordinary libtest metadata does not yet admit its generated leaves.
Rustdoc can run separate merged and standalone harnesses; one shared logfile is
not a complete joined result stream, and doctest names use a different grammar.
Compile-only/no-run cases also cannot be represented by a fabricated executed
Cargo artifact. A truthful doctest-specific contract remains an open gate.

## Offline installation candidate

`numera-emoji install-prebuilt` accepts no options except `--help`. It is an
Rust offline installer used by the package acceptance workflow. It verifies a
complete archive before exclusively creating a fresh environment, and refuses
an existing installation before changing it. The contributor build driver is
also enabled; it compiles both artifacts before installing either artifact.
The authored chain verifies the existing schema1 package and preserves an existing
`.venv`; selects an installed base interpreter; exclusively claims a fresh package
`.venv`; creates an isolated environment with external stdlib `venv`; verifies its
actual interpreter through `sysconfig`; and installs the bundled wheels using
`--no-index --only-binary=:all:`. Inherited pip target/index/config overrides and
Python import overrides are not carried into installation. No Cargo, source build,
online fallback or first-party Python command string is used.

The selected wheel's single native extension and repaired relative libraries are
copied by the existing staged installer. The same fresh selected-runtime probe
checks dependency origins and the actual loaded extension, then the installed
native executable must start successfully. Every child has finite wall/idle bounds.
A failed initial environment is retained for diagnosis; it is not renamed, reused
or deleted automatically. This is not transactional package rollback or OS network
isolation. Manifest hashes detect changed payloads; publisher signing is not
implemented or claimed. Hosted package acceptance exercises all four targets.

## Logs and evidence

The `menu`, `doctor`, `check` and `install-prebuilt` entry points use existing redacted UTF-8 UTC run
logs under `logs/`, with per-run identity and final exit status. The menu logs
command name and argument count, not argument values or credentials. Child
commands retain their own diagnostic logs. Cleanup uncertainty retains recovery
markers and blocks continuation; operational logs are not removed by these commands.

Scripted workflow tests, actual copied-binary doctor/menu traces, a shared-input
fixture, exit preservation and cancellation tests are authored in the existing
native tooling suites. The menu contract test obtains actual generated command
arguments and checks each command's real help once; the no-option bot is never
started. Original prebuilt environment assertions are replayed only from three
hash-admitted historical Git sources in the existing bounded worker. Its install
path is refusal-only, not successful package setup. Formatting, source inspection
and authored replay code are not runtime proof.
Final acceptance still requires hosted compilation/tests, real console behavior,
four-platform installed packages and copied-data preservation. See the current
[CI status](rust-ci-status.md) before treating authored checks as passing evidence.
