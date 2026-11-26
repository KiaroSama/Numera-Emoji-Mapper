# Native entry points and contributor tools

The installed entry point is `numera-emoji.exe` on Windows or `numera-emoji` on
Linux. Keep it beside the package's assets, licenses, manifest and configuration
template. Supported prebuilt packages cover four 64-bit targets: Linux x64,
Linux arm64, Windows x64, and Windows arm64 (no macOS or 32-bit builds).

```powershell
.\numera-emoji.exe menu
.\numera-emoji.exe doctor
.\numera-emoji.exe panel
.\numera-emoji.exe build-collection --help
```

On Linux use `./numera-emoji`. The menu and doctor are Rust commands. Installed
archives contain no PowerShell/shell application launchers, interpreter bootstrap
or automatic build. In a source checkout, put the verified native executable at
`bin\numera-emoji.exe` and run `run.ps1` with PowerShell 7.3 or newer:

```powershell
.\run.ps1
.\run.ps1 -Check
.\run.ps1 panel
```

The launcher uses the original console. If Explorer runs it with PowerShell 7,
it pauses after a failure only when standard streams are not redirected. The
menu supports Back and cancellation; entering it does not authorize a publish.

Only leading launcher controls are interpreted: `-Check` selects doctor, and
`-CommandArguments` accepts an explicit argument array for PowerShell callers.
After the first ordinary native token, every remaining argument is literal,
including names such as `-Debug`, `-Verbose`, `-Check`, and `-CommandArguments`.
For example, `.\run.ps1 build-collection --title -Check --help` forwards the
complete native command. With an explicit array, place literal launcher-like
values inside the array: `.\run.ps1 -CommandArguments @('status', '-Check')`.
The scalar value immediately after `-CommandArguments` cannot be another
launcher control. Quoting a reserved scalar name does not distinguish it from a
control after PowerShell argument parsing; use an explicit array even for one
literal name, such as `.\run.ps1 -CommandArguments @('-Check')`.
Keep any launcher controls before the native command; `-Check` with command
arguments is an error. Explicit `-Check:$false` retains ordinary passthrough when called from PowerShell.

### Passing literal arguments to PowerShell

`pwsh -File` splits dash-prefixed arguments containing `:` before `run.ps1`
receives them. This can affect `-Check:$false`, `-CommandArguments:menu`, and
`--data-dir=C:\path`, even when the outer shell quotes the argument. To pass
literal values, call the script from an existing PowerShell session with an
array, or call `.\bin\numera-emoji.exe` directly:

```powershell
.\run.ps1 -CommandArguments @('panel', '--data-dir=C:\path with spaces')
```

`doctor` checks native codecs, physical paths, assets and package hashes. It
reports missing optional video tools and configuration key names without printing
secret values. It does not install dependencies or contact Telegram.

## Manual Windows desktop acceptance checklist

Use a disposable fixture, not a live catalog. Record these observations
separately from automated CI results.

- On a desktop where Explorer opens `.ps1` with PowerShell 7, double-click
  `run.ps1`. Check that one console opens, it closes on success, and it waits
  for Enter after a controlled interactive failure.
- Use fixture input and output paths containing both spaces and non-ASCII
  characters. Confirm that the files land at the intended paths.
- Check the intended GPU and media settings with `run.ps1 -Check`, then convert
  a small fixture and inspect the output and log for the selected codec or a
  reported fallback.
- If a second physical volume is available, point `EMOJI_DATA_DIR` to a
  disposable external catalog there and repeat the media check. Sibling
  directories on `C:` test the project boundary, not separate physical volumes.

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
Windows use the matching x64 or arm64 MSVC build tools. FFmpeg/FFprobe are required for video tests.
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
cargo test --config tests/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release -- --test-threads=1
```

Build the release binaries first. Use the isolated runner; see
[test admission and migration](native-test-migration.md). Browser and Worker
checks use their committed npm lockfiles, actual Chromium and Vitest. No Python
compatibility environment or extension build is part of this workflow.
