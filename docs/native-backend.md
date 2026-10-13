# Native local backend

The installed Rust backend is now the primary local command path. The Windows
menu and doctor invoke it; superseded Python application modules have been
retired from production. Final integrated source-retirement acceptance and all
four platform package checks have passed; the revision-bound evidence is below.
See [ADR 0003](adr/0003-rust-local-backend.md) for the decision and rollback boundary.

## Expanded first-party migration (in progress)

The accepted backend migration above does not mean the whole repository is
Rust-owned. A subsequent migration includes the remaining local media/identity,
coin-domain, test, generator and tooling code. Browser assets and the entire
Cloudflare Worker remain unchanged; third-party codec implementations remain
external dependencies.

The initial static-image/identity/conversion, SVG renderer binding, pure Lottie
tint/bounded TGS loading/canonical animated identity/loaded-document validation
and Lottie preview/file-preparation/file-validation/vector-comparison handling,
static preview/tint/tri-state comparison, source media detection/collision keys
and coin-alias/inventory-text/frozen-group-plan/alias-replay/provider-status-policy
and bounded provider-GET/retry/rebuild-resume-validation slices are additive. An initial video seam owns
FFprobe, alpha-capable VP8/VP9 decoding and the frozen identity stream. The next
additive slice includes canonical first-frame dHash, bounded file/toolchain-keyed
working sets and complete native-frame/timestamp comparison. Video validation,
blank sampling, VP9 conversion, stream-copy preparation and WebP preview are
covered at independent source-fixture seams. Internal worker video fingerprint,
comparison, validation, preparation, conversion and preview dispatch are being
connected; the production adapter and installed packages remain unchanged.
Gallery thumbnails and animation metadata are also being mapped at their own
source-fixture seam, retaining their distinct filters/sizes/quality.
Full failure/platform and original-case parity remain pending.
These slices use
pinned source-generated fixtures. The internal `codec-worker` entry point is
not yet the production media adapter. Its limited hosted checkpoint is build
and seam evidence only, not complete parity, a new installable release or
permission to remove the old source. Existing installed packages remain active.

## Ready-package installation

Download a matching archive and checksum from a successful **Prebuilt packages**
workflow run, or release assets after every platform job succeeds. See
[README installation](../README.md#install-a-ready-built-package--no-local-rust-build).
Windows x64, Linux x64 (glibc 2.39+) and macOS 15+ Intel/Apple Silicon are the
package targets. The bundled offline wheels require Python 3.11. FFmpeg/FFprobe
on PATH are required for video; Rust/Cargo and C++ build tools are not end-user
prerequisites. The older v2.0.0 source ZIP is not a prebuilt runtime.

The archive includes the executable, matching abi3 wheel, binary dependency
wheels, retained coin tools and public assets. `scripts/install_prebuilt.py`
verifies manifest/platform/hashes and installs offline with `--only-binary=:all:`
into its own virtual environment. It refuses an existing environment before
changing it. Extract updates into a separate folder; preserve your configuration,
collection, archives and recovery backups before switching installations.

```powershell
py -3.11 scripts\install_prebuilt.py
.venv\Scripts\python.exe scripts\launch_prebuilt.py --help
.\run.ps1 -Check
.\run.ps1
```

```bash
python3.11 scripts/install_prebuilt.py
.venv/bin/python scripts/launch_prebuilt.py --help
.venv/bin/python scripts/launch_prebuilt.py panel
```

Copy `.env.example` to `.env` and supply your own configuration before Telegram
commands. `scripts/launch_prebuilt.py` selects the package interpreter and root;
Windows retains a private Job until its child exits. Temp/cache paths stay under
`logs/tmp/` and `.cache/`; linked/reparse escapes refuse. Neither the menu nor
doctor invokes Cargo or falls back to retired Python application modules.

## Native command surface

Pass these commands to `scripts/launch_prebuilt.py` with the package interpreter,
or invoke the installed `native/runtime/numera-emoji` executable (`.exe` on
Windows) with the appropriate package Python environment:

| Responsibility | Commands |
|---|---|
| Convert and build prepared images | `make-emoji-pngs`, `build-pack` |
| Collect and publish a catalog | `add-media`, `fetch-pack`, `fetch-emoji-ids`, `build-collection` |
| Curate and apply saved intent | `panel`, `plan-status`, `plan-apply`, `sync-order` |
| Reports, archives and recovery | `status`, `pack-manifest`, `pack-archive`, `identity-repair` |
| Premium emoji ID bot | `emoji-bot` |

Individual commands expose `--help`; `emoji-bot` uses environment configuration
instead of command options. `plan-apply` and `sync-order` require `--apply` for
live changes; `identity-repair` supports `report`, `migrate-video-keys` and
`restore --bundle PATH`, with `--apply` required for migration/restore writes.
`pack-archive --export PACK --zip PATH` produces a copy-only ZIP. Existing JSON
schemas, content keys, published IDs, saved intent and configured logo policy
remain compatibility contracts. Commands are not permission to mutate live packs.

Rust owns SQLite, catalog/dedup decisions, transport and mutation proof, application
orchestration, HTTP/view/save/cache policy, state/filesystem operations and reports.
The JavaScript panel and Cloudflare Worker retain their existing stacks.

## Retained Python responsibilities

Three categories remain explicit; they are not all media-only:

1. **Exact media/identity:** `media_bridge`, `media`, `media_lottie`, `identity`,
   `video_decode` and `repaint` preserve Pillow/SVG/Lottie/FFmpeg bytes, pixels and
   durable identity. The bridge never runs a collector, catalog, publisher, bot,
   panel server or report application. Rust owns preview cache/admission/warm-up;
   Python renders only the supplied codec request. Codec children receive validated
   non-secret settings such as `EMOJI_FFMPEG_TIMEOUT`; dotenv and credentials are
   disabled in those children.
2. **Thin/shared coin bindings:** `telegram_api`, `announce`, `packstate` and
   `pack_rows` call Rust for transport/retries/upload proof, announcements,
   leases/atomic replacement and Markdown tables. Shared configuration, logging,
   error types and similarity exports remain for the unchanged coin-domain tools.
   Requests serves those provider tools; NumPy is only needed by coin remapping.
3. **Development/reference support:** sandbox cloning, bounded SQLite snapshots,
   path/state enumeration and build/install/process-owner harnesses remain Python.
   Pinned source oracles and relocated references stay under `tests/`, excluded
   from runtime packages. They are regression/rollback evidence, never production
   fallback or a second application dispatcher.

Retained coin-provider tools continue as `python -m coins.<tool>`. Public exception
classes and `add_sticker` results remain source-compatible: an integer after
verified recovery, otherwise `None`. An unanswered mutation without sufficient
proof refuses rather than blindly retrying. Worker announcements are not retried
or silently sent directly after an unknown outcome.

## Developer installation and isolated consumers

Only contributors changing Rust need Rust 1.99+, the platform linker and an
explicit source build:

```powershell
.venv\Scripts\python.exe scripts\build_native.py
```

The locked build installs the extension and executable under `native/runtime/`,
separate from disposable `native/target/`. Repaired wheel shared libraries remain
beside the extension. Staged hash-verified replacement preserves a working binary
on failure. Executable, matching extension/libraries, codec dependencies and assets
are all required; this is not a standalone-binary distribution.

`scripts/panel_sandbox.py` owns a copied catalog and runs the installed native
panel with scrubbed credentials, a strict argument allowlist and a lifetime lease.
Windows uses a private kill-on-close Job; Linux/macOS inherit the same locked file
description and monitor wrapper PID/birth identity. An unrelated listener is never
adopted. `coins/run_convert.ps1` invokes native `make-emoji-pngs` for SVG and PNG;
its owner stops the process tree after 120 seconds without marker progress or a
3,600-second wall ceiling. Ordinary failures are not silently retried.

## Verification status and limitations

Final integrated core [run 38174075123](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/38174075123)
at `0811e9b` passed all six jobs after production retirement: Python 3.11/3.12/3.14,
Windows native safety, the native-backed browser suite and Worker checks.
Four-platform [package run 38173007469](https://github.com/KiaroSama/Numera-Emoji-Mapper/actions/runs/38173007469)
at `c010464` passed actual extracted installation, CLI, exact media, retained coin
manifest and lifecycle checks. The later `0811e9b` changes only the lock-test source
inventory; production/package inputs are unchanged. Tag publication was skipped.

Copied real catalog readback and private-copy native migration/restore/source rollback
have separate local evidence. After removing disposable build and verification
extractions, the retained Windows installation passed its actual doctor, native CLI,
real 100×100 PNG conversion and coin entry-point checks. Protected input and runtime
hashes were unchanged; no Cargo build or live Telegram mutation was used. Recovery
copies, source history and required test fixtures were preserved.

Persistent acceptance includes source-generated fixtures, actual CLI/loopback
cases, native-server-backed browser suites, exact PNG/TGS/VP9 codecs, cross-language
locks, seven persisted migration process-death/restart boundaries and save-time
competing ownership. Synthetic browser thumbnails alone do not prove codec parity.
Optional private fixtures remain separate from public CI coverage.

Version2 rollback uses WAL-safe snapshots, complete media/state intents, signatures
and journal replay. Changed media, later catalog edits or a different active bundle
refuse rather than overwrite. SQL REAL values refuse before writes: the original
catalog schema uses TEXT/INTEGER and no supported producer requiring REAL was
established. Arbitrary extension-table/producer Unicode-version compatibility is
not certified. This limitation is not evidence that normal catalog rollback failed.

Native logs are new UTF-8 UTC files under `logs/`, with secret/token redaction.
Preserve real operational logs and unresolved diagnostic/recovery evidence. A
separate historical verification-receipt association remains unresolved; current
CI cannot retroactively identify it. Final closure is not claimed here.

For rollback, preserve a verified pre-retirement application version from Git
history and your data/configuration backup. Test restoration on an independent
copy with that version's required dependencies before changing a live installation.
A native-produced rollback bundle is checked by retained source references; those
references are not a supported end-user application. Never restore over unrelated
later data changes or reintroduce a silent Python production fallback. Cleanup
waits for integrated acceptance and preserves data/media/archives, required
fixtures, recovery evidence, Git history and one verified current installation.
