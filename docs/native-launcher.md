# Native entry points and contributor tools

The installed entry point is `numera-emoji.exe` on Windows or `numera-emoji` on
Linux. Keep it beside the package's assets, licenses, manifest and configuration
template. Only Linux x64 and Windows x64 packages are supported.

```powershell
.\numera-emoji.exe menu
.\numera-emoji.exe doctor
.\numera-emoji.exe panel
.\numera-emoji.exe build-collection --help
```

On Linux use `./numera-emoji`. The menu and doctor are Rust commands. There are no
PowerShell/shell application launchers, interpreter bootstrap or automatic build.
The menu supports Back and cancellation; entering it does not authorize a publish.

`doctor` checks native codecs, physical paths, assets and package hashes. It
reports missing optional video tools and configuration key names without printing
secret values. It does not install dependencies or contact Telegram.

## Installation and updates

Download a successful release/build archive and its SHA256 checksum, extract it,
then run `package-verify` and `doctor`. Checksums detect changes; obtain packages
from a trusted publisher. The archive includes native dependency notices and
renderer licenses. Contributor tests, npm packages and script launchers are not
part of the operator archive.

`install-prebuilt --destination DIR` makes a verified offline copy. It rejects
links, path escapes, concurrent installers and replacement of local operator
data. Successful replacements retain the previous directory for late writes and
recovery. Keep `.env`, collections and unresolved journals backed up; extract an
update into a fresh directory when an existing installation contains local data.

## Contributor commands

Install the exact compiler in `rust-toolchain.toml` (Rust 1.100.0), rustfmt,
Clippy, CMake and the platform C/C++ linker for bundled third-party codecs. On
Windows use the x64 MSVC build tools. FFmpeg/FFprobe are required for video tests.
Normal installed use needs none of these build tools.

From the repository root:

```bash
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
runtime/target/release/numera-dev build
runtime/target/release/numera-dev check
runtime/target/release/numera-dev test --test baseline_catalog
runtime/target/release/numera-dev panel-sandbox --source collection
runtime/target/release/numera-dev benchmark-similarity --count 1500 --seed 321
```

Use `.exe` suffixes on Windows. `numera-dev` keeps temporary files and caches
inside the project, owns child process trees and reports missing prerequisites.
A sandbox uses an owned snapshot and no operator credentials. Cleanup uncertainty
retains recoverable evidence; it is not permission to remove a possibly active
clone. Contributor binaries and the guarded runner are excluded from downloads.

<a id="transitional-source-check-orchestration"></a>
## Source checks

```bash
cargo fmt --manifest-path runtime/Cargo.toml --check
cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings
cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release -- --test-threads=1
```

Build the release binaries first. Use the isolated runner; see
[test admission and migration](native-test-migration.md). Browser and Worker
checks use their committed npm lockfiles, actual Chromium and Vitest. No Python
compatibility environment or extension build is part of this workflow.
