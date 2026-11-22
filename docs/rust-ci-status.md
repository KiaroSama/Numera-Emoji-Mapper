# Rust build and CI policy

The supported compiler is Rust **1.100.0**, pinned in `rust-toolchain.toml` and
CI. Supported application targets are four 64-bit platforms: **Linux x64**,
**Linux arm64**, **Windows x64**, and **Windows arm64**. macOS and 32-bit
build/test matrices are excluded.

Every push and pull request runs:

- A mandatory release build of all standalone Rust executables on all four 64-bit targets, followed by byte-level embedded logo and architecture verification (`verify-binary`).
- rustfmt, Clippy with warnings denied, and unfiltered Cargo test discovery.
- Native codecs, SQLite/locking, process death/recovery and offline package tests.
- One Chromium job for all 156 panel browser/source cases, with actual discovery.
- Worker TypeScript checks, Vitest, dependency advisories and dry-run packaging.
- Rust security-tool regressions and the Gitleaks scan under its retained policy.

CodeQL default setup should analyze **Rust**, **JavaScript/TypeScript** and
**Actions**. Deselect **Python** after retiring its sources. GitHub stores this
language selection in repository settings. After saving a configuration change,
verify a new analysis against the PR's current head. See GitHub's [configuration
guide](https://docs.github.com/en/enterprise-cloud@latest/code-security/how-tos/find-and-fix-code-vulnerabilities/manage-your-configuration/edit-default-setup).

The Prebuilt packages workflow runs on pushes, PRs, and version tags across all
four 64-bit targets (`linux-x64`, `linux-arm64`, `windows-x64`, `windows-arm64`).
It builds the native executables, verifies embedded logos (`verify-binary`),
exercises extracted offline installation, and uploads ZIP/checksum pairs. On
`main` pushes and `v*` tags, the `signing` job runs inside the protected
`numera-signing` GitHub environment only after the package matrix, the complete
reusable CI workflow, and the complete reusable Gitleaks workflow succeed for
the same triggering revision. A failure, cancellation or missing acceptance
job prevents signing and, transitively, publication. The release Gitleaks
invocation explicitly scans tag pushes instead of inheriting the standalone
workflow's non-default-branch push skip. The signing job applies Windows Authenticode signatures
with RFC 3161 timestamps, sign every archive with the pinned RSA-3072 publisher
key (`docs/SIGNING.md`), and verify tamper rejection. Only a version tag runs
the `publish` job to sign `SHA256SUMS`, attest build provenance via
`actions/attest-build-provenance@v4`, and attach the verified release assets.
The manual Rust checkpoint is diagnostic, not a replacement for CI.

The reusable acceptance calls deliberately have separate concurrency groups
from ordinary push/PR checks. Neither invocation can cancel the other. Ordinary
checks remain visible on every latest pushed commit; signing cannot rely on a
green result from an earlier commit or a separately triggered manual run.
Signed main/tag builds repeat the acceptance suites inside the release workflow
so GitHub enforces their dependency before granting access to the signing
environment. PR and feature-branch package builds keep the existing lightweight
package-only path and still run the ordinary CI and Gitleaks workflows.

CI compiles the unfiltered Rust tests in `runtime/target/ci-tests` using release
settings with LTO disabled and 16 codegen units. This avoids repeating link-time
optimization for every test executable. Test companions use those compilation
settings; distribution executables, package smoke tests and the similarity
benchmark retain the normal optimized release build. The test cfg, default
features, assertions and discovery are unchanged.

All jobs check out the triggering PR head SHA (or push SHA) and retain
`source-revision.log`. Linux and Windows matrix entries do not cancel each
other. Build artifacts and failure diagnostics are uploaded with explicit
retention; failures are not hidden with `continue-on-error`.

Rust test discovery has no maintained target list that can omit a new suite.
Browser discovery uses the committed Playwright test directory. The separate
[test inventory](native-test-migration.md) checks all 1,516 original IDs and
preserves browser/performance/source categories, inherited cases and genuine
platform skips.

The application archive contains the Rust executable, runtime web assets,
configuration template, licenses and native dependency notices. It excludes
Python, shell launchers, contributor tests, npm dependencies and build tools.
FFmpeg/FFprobe remain external tools for video.

Badges report GitHub's current workflow state; they do not certify unrun edits.
For an unmerged migration, inspect the PR/branch's latest commit checks. The
main branch's latest commit and its check mark change only when main changes.
