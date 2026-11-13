# Rust migration and CI acceptance

The migration branch is `feature/first-party-rust`, reviewed in
[PR #70](https://github.com/KiaroSama/Numera-Emoji-Mapper/pull/70). It must remain
unmerged until the complete migration and required CI checks pass.

The Cloudflare Worker and browser interface intentionally retain TypeScript and
JavaScript. The requested Rust scope is the local core and contributor tools.

## Coverage retained

- The original Rust tests from the starting migration branch remain, with
  additional tests for bounded source-constant parsing, demand-driven preview work,
  source-bound mock assertion accounting and repeatable wheel builds.
- Original Python regression cases are retained, with an added journal-integrity
  regression; browser and Windows cases retain their separate CI jobs.
- All 119 Worker tests remain. Dependency updates repair the `undici` advisory
  without changing the Worker's implementation language.
- Native acceptance uses unfiltered Cargo discovery. Compilation-only jobs are
  additional checks and never count as executed test coverage.
- Original-case migration mappings remain pending until actual parity evidence
  is verified. A larger Rust suite alone does not authorize deleting old cases.

## Build and test policy

CI and package workflows run on every push and pull request, with manual dispatch
available. Release binaries and every Rust test target compile on Linux x64,
Windows x64, Intel macOS and Apple Silicon macOS. Matrix jobs do not stop their
siblings when one fails.
Concurrency groups separate push, pull-request and manual events so simultaneous
checks of one commit complete without cancelling each other. Superseded runs of
the same event and branch still stop.

The Gitleaks workflow inherited from main retains its scheduled and pull-request
secret scans. Its security event policy is checked separately from automatic
builds and the manual native checkpoint. Four historical false positives (two
source SHA-256 digests and two synthetic redaction-test identifiers) have exact
commit/file/line/rule identities; future matches remain reportable. A Rust
regression checks those boundaries and summary privacy. Scanner integrity,
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
network guard stays enabled. Windows uses Job ownership. Darwin uses the existing
process-group owner and each lifecycle test's own child owner and cleanup markers;
it does not claim Linux subreaper semantics.

Image output checks preserve independent source generation and input hashes.
Compressed PNG/WebP bytes come from the historical encoder on the current platform;
pixel identity and deterministic cache reuse remain checked. Pillow is pinned to
the fixture encoder version while the codec migration is incomplete.

## Remaining work

The CLI and panel now use the same executable's Rust media worker. The build
driver, package builder and fresh offline installer are Rust commands.
Third-party codecs still require CPython, and compatibility modules plus the
original regression suite remain for behavioral coverage. This is not a
Python-free local runtime. Original-case migration rows remain pending until
reviewed evidence authorizes retiring their historical cases. Packages verify
manifest hashes; they do not claim cryptographic publisher signatures.

Latest hosted checks and failures are visible in the PR. Passing builds on some
platforms are not a substitute for a green final revision and package acceptance
on every required platform.

The panel now uses the same executable's Rust codec worker for previews. Opening
a catalog primes only stills for the first eight cards; full animations are
rendered on demand and cached. A second warm-up reuses those files. The browser's
default remains **24 simultaneous animations**, with its existing visibility,
scroll and background-tab controls.
