# Native backend migration status

The Rust local backend is under development. The existing Python launcher remains
the production entry point until command, recovery, rollback and platform acceptance
are complete. Development commands are not permission to test against live packs
or the operator's catalog.

## Ready-package installation

Normal users download the matching archive from a successful **Prebuilt packages**
workflow run, or from release assets after a versioned release has passed every
platform job. See [README installation](../README.md#install-a-ready-built-package--no-local-rust-build).
Windows x64, Linux x64 (glibc 2.39+) and macOS 15+ Intel/Apple Silicon are the
package targets. Python 3.11 and FFmpeg/FFprobe remain runtime prerequisites;
Rust/Cargo and a source build are not required for package installation.

The archive includes the executable, matching abi3 wheel, binary dependency
wheels and public assets. `scripts/install_prebuilt.py` installs offline with
`--only-binary=:all:` and checks the manifest, platform, hashes and package-local
virtual environment. `scripts/launch_prebuilt.py` selects that environment;
Windows retains a private Job until the native child exits. Owned temporary and
cache paths stay under the extracted folder's `logs/tmp/` and `.cache/`, with
linked/reparse escape paths refused. Hosted build caches likewise stay inside
the CI checkout. No operator configuration, data or build tree is shipped.

Package smoke tests do not replace the complete migration acceptance ledger.
Successful package delivery alone does not authorize primary cutover or deleting
legacy production source, rollback evidence or operator files.

## Developer build and installation

Only contributors changing Rust source need the selected project interpreter,
Rust 1.99+ and the platform linker:

```powershell
.venv\Scripts\python.exe scripts\build_native.py
```

The installer builds the locked release PyO3 wheel, checks its import, explicitly
builds the `numera-emoji` executable and installs it under `native/runtime/`.
On Linux, auditwheel-repaired shared libraries are also installed beside the
checkout package so the extension's relative loader paths remain valid.
The executable is copied through a unique sibling staging file, flushed and
SHA-256-verified before replacement. A failed replacement preserves the previous
installed file. Worker counts respect the existing test/build ceiling.

The installation is outside `native/target/`, so removing disposable Cargo build
output does not delete the installed executable. The wheel, exact-media Python
adapter, project assets and configuration are still runtime dependencies; this is
not a standalone binary distribution or permission to delete them. No global Rust
installation or automatic production cutover occurs.

## Development command surface

```powershell
.\native\runtime\numera-emoji.exe --help
```

The command surface includes panel, local/pack/ID collection, collection and PNG
pack publishing, the emoji bot, plan application/reordering/status, conversion,
roster/gallery, archive/export and identity maintenance. Individual commands expose
`--help`; `emoji-bot` uses environment configuration instead of command options.
Existing JSON schemas, content identity and configured logo policy remain contracts.

Rust owns SQLite, network transport, application decisions, HTTP, filesystem state
and reporting. `emojikit/media_bridge.py` is the exact codec/identity boundary for
Pillow, SVG/Lottie and FFmpeg operations; it must not perform catalog, publishing
or bot orchestration. Codec children receive only validated non-secret codec
settings (currently `EMOJI_FFMPEG_TIMEOUT`); exported environment settings take
precedence over project `.env`, while child dotenv loading and credentials remain
disabled. JavaScript UI and the Cloudflare Worker remain unchanged.

## Retained coin compatibility

Retained coin-provider tools use thin Python exports backed by Rust for Telegram
transport, retries, upload attribution and announcements. Exact image comparison
still uses the codec boundary. Public exception distinctions and the
`add_sticker` return contract (`int` after verified recovery, otherwise `None`)
remain; an unanswered mutation without `expected_before` fails closed rather
than being blindly retried. Worker announcements are never retried or silently
sent directly after an unknown outcome.

Conversion takes an exclusive lease on its output directory before inspecting
resume markers or replacing files. A competing converter refuses without changing
existing output. Recovery acceptance includes test-executable-only persisted-stage
barriers; no production executable or extension includes that fault-injection path.
These compatibility changes do not by themselves complete the primary cutover gate.

## Isolated development consumers

`scripts/panel_sandbox.py` runs the installed native panel against an independently
copied catalog, with scrubbed credentials and an explicit argument allowlist. It
retains the clone's lifetime lease until its child has stopped. Windows uses a
private kill-on-close Job; Linux and macOS pass the same locked file description
to the native server and monitor the wrapper's PID and start time (Darwin uses
libproc birth identity). Neither process
adopts a listener for another clone.

`coins/run_convert.ps1` uses the installed `make-emoji-pngs` command for both SVG
and PNG passes. `scripts/native_convert_owner.py` owns each attempt and stops its
process tree before reporting a timeout. A marker unchanged for 120 seconds stops
that attempt; a 3,600-second wall bound applies even when progress continues.
Quarantined sources still require review, and an ordinary failure is not silently
retried. These wrappers require a current native installation; they do not fall
back to retired Python applications.

## Verification and limitations

Source snapshots and generated parity fixtures live under `tests/oracles/` and
`tests/fixtures/backend/`. Selected command tests use detached roots and a
numeric-loopback API, never live Telegram. Required generated VP9 fixtures run in
CI; optional private real-video replay is separate local-only evidence. Native
Windows ownership tests and Linux/Python-version jobs are both required. The four
existing browser suites now serve their real page from the owned native panel;
synthetic media fixtures in those suites do not certify codec rendering. Separate
real PNG/TGS checks cover native preview warm-up and request-cache reuse. Warm-up
renders full then compact tiers in grid order and is canceled and joined at shutdown.

Version2 migration/rollback uses online SQLite backups, complete media and state
intents, verified signatures and journal replay. Inconsistent intents, changed
media, later catalog edits and a different active rollback bundle refuse rather
than overwrite data. Deterministic interrupted-stage fixtures are not complete
forced-process-death coverage. REAL/nonfinite SQL signature formatting is not yet
verified and refuses before writes; arbitrary producer Unicode-version compatibility
is not certified.

Native logs are new UTF-8 UTC files under `logs/` for each invocation, with registered
secret and token-shaped value redaction. Preserve diagnostic logs and rollback
backups while a failure is unresolved. Do not treat a log file or successful build
as proof that every command is ready for production.

Primary cutover and removal of superseded production implementations wait for the
complete acceptance ledger, copied-data rollback, installed launcher checks and
final supported-platform CI. Cleanup preserves operator data, media, archives,
required fixtures, Git history and necessary runtime dependencies.
