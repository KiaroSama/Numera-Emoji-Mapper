# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [3.5.0] - 2026-11-28

### Added
- Display-name and `no @username` fallback annotations beside numeric `user_id` values in Cloudflare Worker log-channel alerts when a Telegram sender has no handle.
- Live drag-destination pack and capacity indicator (`#dragTarget`), bounded Worker Telegram request timeouts, privacy-safe channel fan-out delivery counts, provider `status=complete|partial` summaries, and cache reconciliation diagnostics.

### Fixed
- Guard `coin-remap-ids` cache reuse and set completeness with strict 1,024-byte decoded signature validation and unresolved-error retry (R32).
- Protect all five durable remap roles, implicit `.prebroken.json` backups, and `.locks` storage against path and hard-link collisions on Unix and Windows (R33).
- Return an ordinary validation error instead of panicking when `sets` is missing from a remap state object (R34).
- Clear stale drag destination feedback when the pointer leaves `#grid` (R35).
- Suppress automatic log-channel mirror echoes, harden `/log-test` body reads, stabilize Windows quiet-process fixtures, and isolate per-admin channel forwarding failures (R25–R31).

### Changed
- Advance the supported Rust compiler toolchain to `1.100.1` across CI, package builds, developer tooling, and documentation.
- Refresh `coins/keywords.csv` across all 29 coin emoji packs from a complete pack-by-pack audit.

## [3.4.0] - 2026-11-27

### Added
- Structured HTML log-channel formatting in the Cloudflare Worker matching Numera Group Bot: six circle orbs (`🔴🟠🟡🟢🔵🟣`), `bot` and structured `key: <code>value</code>` fields with folded `(@username)` annotations, raw text filtering, and UTC timestamps.
- Bearer-authenticated `POST /log-test` endpoint and boolean `log_channel` / `pack_links` readiness flags on `GET /health`.
- Optional `BOT_ALLOWED_CHANNEL_IDS` secret upload in `numera-worker-admin secrets`.

### Changed
- Route warnings, access denials, pack announcements, emoji lookups, greetings, and allowed channel forwards to `LOG_CHAT_ID` while keeping D1 writes lean and external webhook-secret probes console-only.
- Normalize bare positive channel IDs for `PACK_LINKS_CHAT_ID` and `BOT_ALLOWED_CHANNEL_IDS` in the Worker, and attach an explicit `User-Agent` header to Rust Worker announcement requests.
- Polish documentation and bot copy for plain, natural English, update README badges to live GitHub endpoints, and align README ending sections (`Author`, `License`, `Donate`).

### Fixed
- Resolve relative and not-yet-created catalog paths accurately in `doctor` and redact effective configured secrets in diagnostics (#111, closes #110).

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
