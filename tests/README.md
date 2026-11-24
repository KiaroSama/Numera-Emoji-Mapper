# Tests and independent fixtures

The standalone Rust crate in `runtime/` owns local application tests. Browser
JavaScript is tested with Playwright/Chromium in `assets/tests`; the Cloudflare
Worker keeps its TypeScript/Vitest suite. The two reviewed source-checkout
PowerShell launchers have a separate process contract suite described below.
No Python interpreter or PyO3 extension is part of test execution.

## Original case inventories

The immutable original baseline is
[`fixtures/migration/reference-cases.json`](fixtures/migration/reference-cases.json):
1,516 distinct identities, including 26 inherited executions.

| Category | Original identities | Current test language |
| --- | ---: | --- |
| Backend | 1,360 | Rust |
| Browser behavior | 84 | JavaScript/Playwright |
| Browser performance | 13 | JavaScript/Playwright |
| Served-source contracts | 59 | JavaScript/Playwright |
| Total baseline | 1,516 | Rust and browser JavaScript |

Main originally collected 1,515; the reference adds one journal-overlap recovery
case. Six original conditional Windows-only cases retain their platform gates.
The separate [original-main Rust inventory](fixtures/migration/original-main-rust.json)
tracks 34 applicable declarations, including two technical subprocess entrypoints;
these do not inflate the 1,516 baseline. The Worker retains 119 separate cases.

The [Rust inventory tool](../tools/test-inventory/README.md) checks actual wired
Rust bodies and unfiltered Playwright discovery against pinned original source
identities, categories and hashes. It cannot prove behavior, fault fidelity,
process cleanup or successful test execution. Exact-source Linux/Windows CI and
real browser execution remain separate acceptance requirements.

## Build and guarded Rust tests

Supported application platforms are Linux and Windows on x64 and arm64. Install the exact
compiler in `rust-toolchain.toml` (Rust 1.100.0), rustfmt, Clippy, CMake and the
platform C/C++ build tools. Video tests require FFmpeg and FFprobe.

Both full native CI workflows use the shared
[`windows-video-tools`](../.github/actions/windows-video-tools/action.yml) action
on Windows. It preserves an existing complete tool pair or installs the fixed
FFmpeg 9.1 essentials ZIP after verifying its pinned SHA256. Release and digest
updates belong together in that action and require archive-layout review. Both
workflows execute the actual FFmpeg and FFprobe version commands before full
native tests; installation failure must not become a media-test skip.

From the repository root:

```sh
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
cargo fmt --manifest-path runtime/Cargo.toml --check
cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings
cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release --no-fail-fast -- --test-threads=1
```

Build the release runner before using `.cargo/runtime-tests.toml`, even if
selecting a different Cargo test profile. Selection remains available, for
example `--test baseline_catalog`, or through `numera-dev test --test NAME`.
A selected run proves only that selection; final acceptance uses unfiltered
collection and execution.

The unchanged `numera-runtime-tests` runner clears credentials, suppresses dotenv,
uses project-owned temporary storage and owns descendant processes. Linux uses
its private-network bootstrap; Windows uses native process custody. Tests use
owned numeric-loopback HTTP services, actual SQLite/WAL, deterministic clocks
where appropriate, real media and fault processes. Missing runner prerequisites
must be reported. Never run a protected executable directly to bypass a failed
guard, or substitute a live API call for a fixture.

## Browser and performance tests

Use Node 24, the committed npm lockfile and actual Chromium:

```sh
npm ci --prefix assets
npx --prefix assets playwright install chromium
npm test --prefix assets
```

The full 156-case suite is 84 browser +13 performance +59 source contracts.
`test:browser` selects the 97 browser/performance cases; `test:contracts` selects
the 59 served-source cases. See [browser fixtures and options](../assets/tests/README.md).
The explicit `NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` opt-out is reported as a
skip; it is never browser acceptance. Missing Chromium or the Rust panel binary
fails by default. Fixtures start the real Rust server and inspect committed
SQLite state after saves. Screenshots and artifacts stay under `logs/tmp/browser`.

## Source-checkout launcher process contracts

The Windows x64 and Windows arm64 `native-build` jobs also execute:

```powershell
node --test assets/tests/launcher-process.js
```

This separate launcher integration suite requires Node 24, PowerShell 7.3 or newer (`pwsh`
on PATH, or an explicit `NUMERA_TEST_PWSH` path), and the pinned Rust compiler.
It compiles a small synthetic executable in an owned temporary fixture root,
copies the actual `run.ps1` and `run-panel.ps1` there, and records exact native
arguments, working directories and exit codes. Child environments use an explicit
allowlist. The suite never invokes the repository's native executable, reads an
operator `.env`, or contacts a network service. Each PowerShell child has a timeout
that terminates its owned process tree.

The 46 cases cover menu/doctor/panel dispatch, multi-pack arrays, invalid inputs,
missing binaries, ordinary and legacy native argument passing, quotes and empty
strings and timestamp-shaped argument preservation, error preferences, pipeline
output capture, caller preference isolation, log failures, loop prevention,
pauses, and real child
reentry from paths containing spaces, Unicode, apostrophes and semicolons. Explorer
detection is supplied at the actual script branch. A synthetic terminal captures
its arguments; a separate delimiter check follows Windows Terminal's documented
semicolon rule before real child PowerShell execution. This checks the transport
boundary and does not claim GUI Windows Terminal coverage.

Pause assertions record the actual `Read-Host` invocation and delegate to the
real cmdlet, because redirected Windows hosts need not echo its prompt to stdout.
They still require exactly one pause on failure, none on success, and the native
exit status after the real read completes.

The timestamped acceptance summary stays in
`logs/checkpoint/launcher-process-<os>-<arch>.log`, included by the existing CI
diagnostic upload. Failed synthetic fixtures remain under
`logs/tmp/launcher-tests-*`; successful runs delete their owned fixtures. These
process checks supplement the guarded Rust suites above and never replace or
bypass their runner.

## Worker and inventory tooling

```sh
npm ci --prefix worker
npm run typecheck --prefix worker
npm test --prefix worker
cargo test --config .cargo/runtime-tests.toml --manifest-path tools/test-inventory/Cargo.toml --locked --release
cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- --mode rust
```

Worker tests stub fetch; they do not deploy or contact Telegram. The shared
contracts in `fixtures/contracts/` are read by both Rust and TypeScript tests.
The panel's JavaScript type check runs from `worker/` with
`npx tsc -p ../assets/jsconfig.json --noEmit` after its locked install.

## Fixture provenance and historical records

`fixtures/` contains synthetic art, independent historical pixels/identities,
frame/timing observations, contract JSON and original discovery receipts.
`runtime/tests/*_support` supplies owned Rust HTTP, file, database, clock and
process fixtures. Existing expected pixels and IDs must not be replaced by
outputs generated from the implementation being tested. Encoded image bytes
may differ across codec builds; use the documented independent decoded-pixel
contract where exact byte equality is not the contract.

Original Python implementation names in provenance files, audits and ADRs name
the source that produced the historical observation. They are not active
installation or execution instructions. Keep those records accurate and retain
license/attribution information with their fixtures.

See [test migration evidence](../docs/native-test-migration.md),
[codec contracts](../docs/native-codec-contracts.md) and
[revision-specific CI status](../docs/rust-ci-status.md). These documents do not
claim that an unexecuted cohort, pending exact-head CI job or another platform
has passed.
