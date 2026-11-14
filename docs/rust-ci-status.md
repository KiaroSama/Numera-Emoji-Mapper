# Rust migration and CI acceptance

The migration branch is `feature/first-party-rust`, reviewed in
[PR #70](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/70). It must remain
unmerged until the complete migration and required CI checks pass.

The Cloudflare Worker and browser interface intentionally retain TypeScript and
JavaScript. The Rust scope includes the local core, contributor tools and the production
media backend. The standalone `runtime/` crate has no interpreter dependency.

## Coverage retained

- The original Rust tests from the starting migration branch remain, with
  additional tests for native raster/Lottie/video engines, standalone package
  installation, bounded source parsing, demand-driven previews and source-bound
  assertion accounting. Historical wheel cases remain in a separate compatibility job.
- Original Python regression cases are retained, with an added journal-integrity
  regression; browser and Windows cases retain their separate CI jobs.
- All 119 Worker tests remain. Dependency updates repair the `undici` advisory
  without changing the Worker's implementation language.
- Both production and compatibility acceptance use unfiltered Cargo discovery.
  Compilation-only jobs are additional checks and never count as executed coverage.
- Original-case migration mappings remain pending until actual parity evidence
  is verified. A larger Rust suite alone does not authorize deleting old cases.

## Build and test policy

CI and package workflows run on every push and pull request, with manual dispatch
available. Production release binaries and every production Rust test target build and
execute on Linux x64, Windows x64, Intel macOS and Apple Silicon macOS.
The separate historical compatibility matrix and original acceptance buckets remain. Matrix jobs do not stop their
siblings when one fails.
Concurrency groups separate push, pull-request and manual events so simultaneous
checks of one commit complete without cancelling each other. Superseded runs of
the same event and branch still stop.

The Gitleaks workflow inherited from main retains its scheduled and pull-request
secret scans. Its security event policy is checked separately from automatic
builds and the manual native checkpoint. Four historical false positives (two
source SHA-256 digests and two synthetic redaction-test identifiers) have exact
introduction commit/file/line/rule identities. Repeated merge-diff matches require
Git blame to prove the unchanged line's reviewed origin; new matches remain
reportable. A Rust regression checks those boundaries and summary privacy. Scanner integrity,
full-history coverage, repository-suppression refusal and privacy guards remain.

Native cases run serially because embedded codec patches and descendant custody
share process state. Tests that exercise concurrent callers and competing
processes retain their explicit concurrency.

The Cargo batch has a 40-minute aggregate bound on Unix and a 60-minute bound on
Windows, including compilation. Individual suite and codec watchdogs are
unchanged. Once the Windows driver is built, all three acceptance buckets run
even when an earlier bucket fails, and failure logs are retained for diagnosis.

Linux test bootstraps create private user/network namespaces. A checkout-scoped
AppArmor profile permits that operation on hosted Ubuntu; the loopback-only
network guard stays enabled. Windows uses Job ownership and retains verified
member handles until both the Job's active count is zero and each original
process object signals termination; a zero counter alone cannot finish cleanup.
Its existing 10-second cleanup ceiling remains. Darwin uses the existing
process-group owner and each lifecycle test's own child owner and cleanup markers;
it does not claim Linux subreaper semantics.

Image output checks preserve independent source generation and input hashes.
Historical codec tests continue to compare their original encoder outputs. The
production encoder is checked against frozen decoded pixels and durable identity;
compressed bytes may differ where the new encoder makes an equivalent image.
Pillow remains pinned only for the historical test oracle. Native Lottie rendering
uses the exact historical rlottie revision; fixture frames and timing stay independent.

## Runtime and verification status

The CLI and panel use the same executable's native Rust media worker. The
production dependency graph excludes PyO3 and CPython. Schema2 archives contain
the executable, assets and manifest; extracted installs are tested with an empty
PATH and deliberately invalid interpreter settings. FFmpeg/FFprobe remain optional
external tools until a video operation needs them.

The original compatibility library and `numera-compat` developer toolkit remain
for behavioral and fault-injection coverage. They are built separately and are
excluded from production archives. No original CI cohort is removed. Packages
verify manifest hashes; they do not claim cryptographic publisher signatures.

Latest hosted checks and failures are visible in the PR. Passing builds on some
platforms are not a substitute for a green final revision and package acceptance
on every required platform.

The panel now uses the same executable's Rust codec worker for previews. Opening
a catalog primes only stills for the first eight cards; full animations are
rendered on demand and cached. A second warm-up reuses those files. The browser's
default remains **24 simultaneous animations**, with its existing visibility,
scroll and background-tab controls.
