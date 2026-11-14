# Native launcher and contributor tools

The production application is `runtime/Cargo.toml`, package
`numera-emoji-runtime`. A ready-built package places `numera-emoji.exe` (Windows)
or `numera-emoji` (Linux/macOS) in its root beside `assets/`, `lib/`,
`.env.example` and `package.json`. It contains no Python runtime or wheels.
FFmpeg/FFprobe remain external tools for video workflows.

Examples below use Windows spelling. On Linux/macOS replace
`.\numera-emoji.exe` with `./numera-emoji`.

## Menu and foreground commands

```powershell
.\numera-emoji.exe menu
.\numera-emoji.exe panel
.\numera-emoji.exe --help
.\numera-emoji.exe coin-remap-ids --help
```

`run.ps1` starts `menu` with no arguments; `run.ps1 -Check` starts `doctor`.
`run.sh` provides the same behavior with `--check`. Arguments are passed as
separate values and the child remains attached to the terminal.

Both launchers first look for the root executable, then an explicitly built
`runtime/target/release` or `runtime/target/debug` executable. They reject linked
executables and stop if none exists. They never create an environment, select an
interpreter, fetch dependencies or invoke Cargo.

The menu preserves the collection, conversion, publishing, bot, roster and
archive workflows. Navigation supports `0` for Back and `exit`/`quit` for
cancellation. Dry runs and confirmations remain part of the relevant workflows;
starting the menu does not authorize a Telegram mutation. The panel remains a
loopback HTTP service with its browser assets and a default budget of 24 animated
cards. A missing runtime or asset produces an error rather than an automatic build.

## Non-interactive doctor

```powershell
.\numera-emoji.exe doctor
.\run.ps1 -Check
```

`doctor` takes no arguments. It checks the physical application root, panel
assets, native PNG/WebP pixel roundtrips and, when `package.json` is present,
package integrity. It reports missing FFmpeg/FFprobe and unset configuration key
names without printing secret values. Missing optional video tools or unused
configuration do not become Python dependency checks. It performs no installation
or network request. `check` is an alias for this doctor; it does not run the
historical Python suite.

## Transitional source-check orchestration

This heading remains for existing links. Source checks now belong to the chosen
crate and test harness, rather than to the installed application's doctor.
Contributors explicitly build and test production Rust:

```powershell
cargo build --manifest-path runtime/Cargo.toml --locked --release
cargo test --manifest-path runtime/Cargo.toml --locked
```

A source build needs the Rust version declared in the manifest, a C/C++ compiler,
CMake and the platform linker for bundled native codecs. Those are contributor
requirements, not end-user package requirements. Browser and Cloudflare changes
also require their documented browser/Node checks.

The optional `numera-compat` developer toolkit is built separately from
`native/Cargo.toml`. It retains historical Python/PyO3 oracle and fault-injection
tests. Its Python environment and test-only dependencies are not loaded by the
standalone application. Follow the relevant test documentation when running that
harness; do not install its requirements to make the production doctor pass.

## Native test-runner network boundary

Hermetic tests must use the repository's credential scrubbing and loopback-only
fixtures. Test commands that need historical oracle support belong to
`numera-compat`; ordinary application commands never bootstrap that support.
No live Telegram mutation is a substitute for a fixture test.

## Compiled-case observation candidate

Historical test-observation tooling remains developer infrastructure. Observation
manifests, build logs and dispositions are evidence to review, not authenticated
coverage or an instruction to retire tests. The standalone application rejects
`test-runner`, `test-plan`, `test-cargo`, `runtime-check`, `runtime-probe` and
`source-*` oracle commands with a pointer to the optional developer toolkit.

### Same-Cargo metadata admission

Compatibility observation must bind a test executable to the actual Cargo
metadata and artifact from the recorded invocation. Do not manufacture a build
receipt from a second build or a direct-binary run.

### Pre-build plan producer

Historical `test-plan` inputs and receipts belong to the compatibility harness.
They do not install, activate or repair an operator package.

### Same-invocation Cargo candidate

Retain separate raw stdout/stderr logs and exact Cargo selection when collecting
compatibility evidence. Refer to [native test migration](native-test-migration.md)
for the recorded protocol and its limitations.

## Offline installation candidate

The current standalone package supports these explicit commands:

```powershell
.\numera-emoji.exe package-verify
.\numera-emoji.exe install-prebuilt --destination C:\Numera-new
```

`package-verify` checks the package root, platform and manifest hashes. Checksums
show whether files changed; they do not authenticate a publisher or establish
code signing. Obtain archives from a trusted release/build source.

`install-prebuilt` copies a verified package into a fresh destination; it does not
create a virtual environment or install wheels. It refuses occupied or linked
paths. An optional `--package-root DIR` selects the source package. Keep the old
installation and its data until the new one has been verified. Review data-path
configuration before starting against an existing collection; never discard an
unresolved migration journal to make an update start.

Contributors can package an already-built executable explicitly:

```powershell
.\numera-emoji.exe package-native --executable runtime\target\release\numera-emoji.exe --out dist
```

Use `package-native --help` for the supported library, revision and target
options. Package creation does not publish a release or replace an installation.

## Logs and evidence

Application commands use redacted UTC run logs beneath `logs/`. Token values and
credential-shaped text are masked; content hashes and numeric emoji IDs remain
available for reconciliation. Codec failures retain their request-specific
response and diagnostics when cleanup cannot be proved safe. Do not confuse a
successful local build, package checksum or test observation with acceptance of
all platforms or live publishing behavior.
