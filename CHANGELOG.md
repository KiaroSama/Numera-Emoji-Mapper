# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [3.3.0] - 2026-11-26

### Added
- `audit-publication --json` emits schema version 1 with stable finding and error codes. A regression test covers read-only access when SQLite has uncheckpointed WAL data.
- `doctor` reports resolved catalog and private codec workspace paths without printing credentials or creating directories.

### Fixed
- Use the publisher's state schema and compatibility defaults in the publication audit (#109).
- Keep codec workspace files for external catalogs private. Verify Linux permissions and Windows ACLs, including pointer and bounds checks at the Windows API boundary (#109).
- Check Linux ARM64 video identities against independent pixel references, and bound directory allocation for retained compiler fixtures (#109).

## [3.2.0] - 2026-11-25

### Added
- `numera-dev check` reports which checks passed, failed, or were skipped and why.
- `numera-emoji audit-publication` reports publication state and catalog inconsistencies without writing either file.
- Document how `pwsh -File` changes some literal arguments and how to check the launcher on a Windows desktop. CI checks the launcher on PowerShell 7.3.12 on Linux separately.

### Changed
- Move the explicit guarded Cargo runner configuration to `tests/runtime-tests.toml`. The test runner and its isolation rules stay the same.
- Cache only Cargo dependency downloads in CI and release builds, leaving compiled artifacts and credentials out.

### Fixed
- Exclude converter control files from directory ingest and discover APNG inputs (#107).
- Describe when the OS releases a publication lock. Validate recovery journal framing and write test readiness markers atomically (#107).
- Verify numeric and string version metadata in native Windows binaries before and after signing (#107).
- Validate Telegram sticker-set and custom-emoji responses without treating malformed results as missing IDs.

## [3.1.0] - 2026-11-25

### Changed
- Run the menu and panel in one PowerShell window with `run.ps1`. Removed the separate `run-panel.ps1` launcher, and preserve native arguments that match PowerShell option names (#103).
- Link the README version badge to Git tags.
- Add hover-only animation playback. Pause animations while dragging or when the window loses focus.
- Update `zip` to `9.8.1`, `wrangler` to `4.166.0`, and `@cloudflare/workers-types` to `5.20261121.1`.

### Fixed
- Reject invalid or duplicate custom emoji IDs during publication reconciliation, including the logo ID in the first slot of branded packs (NEM-01).
- Preserve unbranded archives, panel text that resembles template markers, and logo positions when importing drafts. Report failed emoji lookups as errors instead of saying the IDs do not exist (NEM-02 through NEM-05).
- Find standard Windows npm installations, verify the approved icon in Windows binaries, correct the CI badge and panel help, and let the Windows process inventory settle within its existing timeout (NEM-06 through NEM-10, #105).
- Publish the lifecycle test's readiness file atomically so the parent cannot read it before it is complete.

## [3.0.0] - 2026-11-22

### Breaking
- Replaced the Python runtime, PowerShell and shell launchers, and PyO3 bridge with a standalone Rust 1.100.0 (edition 2024) application (`numera-emoji`).
- Kept the local curation web panel in browser JavaScript (`assets/`) and the Cloudflare Worker in TypeScript (`worker/`).
- Standardized prebuilt release packages on four 64-bit targets: Windows x64, Windows arm64, Linux x64, and Linux arm64 (macOS and 32-bit targets removed).

### Added
- Embedded the approved project logo into every compiled binary across all four targets (Windows PE icon and version resources via `winresource`, Linux `.numera_logo` ELF section plus `.desktop` entry and hicolor icon) with byte-level `verify-binary` checks in CI and release workflows.
- Added hosted release signing under the protected `numera-signing` GitHub Actions environment with a dedicated RSA-3072 publisher identity (`docs/SIGNING.md` and `docs/signing/publisher.json`): Windows Authenticode signatures with RFC 3161 DigiCert timestamps, detached `.zip.sig` and `SHA256SUMS.sig` signatures verified with tamper-rejection checks, and GitHub build provenance via `actions/attest-build-provenance@v4`.
- Added `numera-emoji plan-apply` to apply the Curate panel's saved `pack_plan.json` to live packs with dry-run default, `--apply`, journalled deletes, and a 20-change per-run cap.
- Seeded `coins/keywords.csv` inside standalone packages (`assets/coins-keywords-seed.csv`) with automatic directory bootstrap and `--flag=VALUE` CLI option parity.
- Split oversized Rust modules under the 800-line ceiling while preserving all 1,516 original test identities (`tools/test-inventory`).

### Changed
- Upgraded `zip` to `9.7.3` in `runtime/` (with updated `THIRD_PARTY_NOTICES.md`) and `vitest` to `5.1.1` in `worker/`.
- Refreshed project and coin logos across the README, Curate panel, and compiled executables.

### Fixed
- Resolved CodeQL alert `#1` (`js/incomplete-multi-character-sanitization`) in `worker/test/publish-input.test.ts` and alert `#11` (`rust/cleartext-logging`) in `runtime/src/worker_admin_main.rs` and `runtime/src/worker_admin.rs`.
- Updated `process_owner_descendants` and Linux test isolation to use `libc::sigaction` across both `x86_64` and `aarch64` GNU Linux hosts.
- Fixed holding-tray card removal when a held emoji is dropped back into the Curate grid.

## Release history

- [2.0.0](docs/changelog-2.0.0.md)
- [1.0.0](docs/changelog-1-0-0.md)
