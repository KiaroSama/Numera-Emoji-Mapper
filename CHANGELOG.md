# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
