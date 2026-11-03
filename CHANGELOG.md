# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- The local application backend and primary launcher now use Rust. Normal
  installation uses verified prebuilt packages; Rust/Cargo is developer-only.
  Exact codecs and native-backed coin exports remain Python compatibility boundaries.

### Added

- Migration-branch `test-runner --observe` opt-in retains per-invocation compiled
  listing and actual libtest dispositions with binary/input hashes. Reports remain
  explicitly untrusted and cannot certify coverage or retirement. Fake-stdout and
  early-exit refusal assertions are authored but uncompiled and unexecuted. The
  `--observe-cargo` adapter binds a supplied plan to the same invocation's actual
  compiler-artifact prefix. The additive `test-plan` producer snapshots tracked
  public inputs and actual offline Cargo target metadata before that invocation;
  the additive `test-cargo` wrapper derives the plan from the same selector vector
  and retains one original Cargo invocation with split local raw logs. Automatic
  main Linux/native browser/Windows and migration-checkpoint steps select that
  wrapper without changing discovery or selectors. Exact source-bound inactive-host
  harnesses remain inventoried without claiming passed cases; source changes revoke
  that classification. Split-stream overflow, invalid UTF-8 and deadline assertions
  are authored but unexecuted. Remaining platform integration, runtime proof and
  doctest admission remain pending.

- Additive contributor runtime inspection with an optional explicit native-extension
  origin/API check. This migration-branch command is unverified at runtime and does
  not change the installed launcher or activate the new media backend. A separate
  contributor builder candidate contains the wheel/install/check orchestration,
  but refuses before build or installation until approved signing, identity, logo
  and rollback checks are integrated. Runtime acceptance remains pending.

- Initial Rust-owned static codec identity, internal worker envelope and pure
  coin-alias policy, with pinned source fixtures and a limited hosted migration
  checkpoint. Production media dispatch is unchanged; full migration and source
  retirement remain pending.

- Native catalog, Telegram transport, publisher, bot, panel, maintenance and
  reporting commands, with source-oracle recovery/rollback tests and offline
  platform packages. Replaced Python application counterparts are retired;
  source references and persisted data compatibility are preserved.

- Exact Rust similarity ordering and batch perceptual-hash candidate selection,
  with source-generated parity fixtures, supported-platform native builds and a
  read-only benchmark/replay command. Identity and published data are unchanged.

- **`numera-emoji plan-apply`** applies the Curate panel's saved pack
  plan to the live packs: dry run by default, `--apply` removes the outgoing
  half of each move, adds arrivals and new emoji through the verified publisher
  path, then reorders — at most 20 Telegram changes per run, resumable, with
  every delete journalled and confirmed. A held emoji that is still live stays.
  A Telegram flood wait longer than `--max-wait` (default 300 s) ends the run with
  exit 3 instead of stalling it.

### Changed

- The migration-branch native test runner has a Linux private user/network
  namespace bootstrap with fresh nested namespaces, loopback-only interfaces,
  inherited-descriptor closure and same-process target execution. Existing
  descendant ownership is reused. Network, descriptor and refusal assertions
  are authored but uncompiled/unexecuted; Windows and macOS isolation remain open.

- All three hosted workflows now require manual dispatch; pushes, pull requests
  and version tags do not start them. Four existing codec suites are included in
  the Windows lifecycle selection with an exact-once policy assertion. These
  declarations and authored assertions do not establish runtime acceptance.

- Refreshed the project and coin logos in the README and the Curate panel's
  header and browser icon. Icon URLs remain versioned by their content.

### Fixed

- Migration-branch source collection retains class and decorated-method skip flags
  separately without counting collection as test execution. Shared premultiplication
  reads dHash mode once and preserves merge-before-multiply lookup ordering.
  New source-derived assertions remain uncompiled and unexecuted.

- Migration-branch check prerequisites use ordered module discovery and actual
  Python truthiness without the unrelated doctor/codec imports; failed discovery
  returns exit2 before source checks. Shared Lottie gzip reads/writes now honor
  actual context entry, exit and close-error precedence. Static tinting resolves
  `putalpha` before requesting the alpha channel; failed tint assertions retain
  their diagnostic inputs. Typed fitting preserves mode/split/bounding-box and
  resize-before-canvas ordering without replacing the legacy path. Shared identity
  resampling resolves resize before the direct LANCZOS alias; preview failures retain
  their diagnostic files. New refusal and lifetime assertions are authored but
  remain uncompiled and unexecuted.

- Migration-branch typed codecs preserve source alpha-call ordering and actual
  Python file-size comparisons for video and TGS admission. Worker warnings also
  reach the per-run file log while retaining the standalone stderr diagnostic.
  Copied-worker assertions require exact error classes, attributed logs and empty
  nested capture scratch before cleanup, retaining unexpected recovery evidence.
  These changes and regression assertions remain uncompiled and unexecuted;
  production media dispatch and source-retirement gates are unchanged.

- Migration-branch native redaction handles Unicode decimal token IDs, standalone
  `bot` prefixes and Unicode literal-secret ordering. Native application log
  retention is resolved after configuration loading, and arbitrary `--token-env`
  values are registered before command validation. Native lines carry a per-run
  identity and finishing returns the recorded exit code with diagnostic counts.
  Configured application and worker panic hooks redact payloads and backtraces,
  with a nonblocking fallback when file logging is unavailable. Original-source replay,
  synthetic tracked-secret scanning, file-log and copied-command assertions are
  authored but not yet executed or installed.
- Migration-branch configuration parsing preserves a single leading UTF-8 BOM,
  original line boundaries and first-value/environment precedence. Numeric settings,
  log retention and codec timeouts share a native Unicode decimal parser with
  source-compatible bound warnings, without initializing Python for numeric policy.
  Source replay and native failure-path assertions are authored but not yet executed.
- Migration-branch lock refusal diagnostics distinguish recorded-PID presence,
  absence and uncertainty without changing OS-lease ownership. Four original
  diagnostic tests have native counterparts and a private historical replay;
  compilation and execution remain pending.

- A held emoji dropped back into the grid disappears from the holding tray
  immediately, instead of leaving a stale card there until the next redraw.


## Release history

- [2.0.0](docs/changelog-2.0.0.md)
- [1.0.0](docs/changelog-1-0-0.md)
