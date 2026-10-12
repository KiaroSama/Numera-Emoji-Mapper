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

- Refreshed the project and coin logos in the README and the Curate panel's
  header and browser icon. Icon URLs remain versioned by their content.

### Fixed

- A held emoji dropped back into the grid disappears from the holding tray
  immediately, instead of leaving a stale card there until the next redraw.


## Release history

- [2.0.0](docs/changelog-2.0.0.md)
- [1.0.0](docs/changelog-1-0-0.md)
