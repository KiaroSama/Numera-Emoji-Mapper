# Native local backend

The production application is `numera-emoji`, built from
[`runtime/Cargo.toml`](../runtime/Cargo.toml). The CLI, panel HTTP server, catalog,
provider tools, image conversion, preview generation and media worker run without
CPython, a Python ABI, wheels or a virtual environment. The browser interface
remains JavaScript and the Cloudflare Worker remains TypeScript.

## Application and media engines

Rust owns application policy, SQLite persistence, Bot API transport, retries,
publication proof, intent recovery, filesystem leases and atomic publication.
Pure application modules are shared with the historical regression crate under
`native/src`; sharing a Rust source file does not introduce its old interpreter
binding into the production dependency graph.

The native media stack consists of:

| Media | Engine | Compatibility boundary |
|---|---|---|
| Raster images | `image`, bundled mozjpeg and libwebp | Historical resizing, alpha arithmetic, blankness, content keys and perceptual hashes |
| SVG | `resvg` | Native rasterization before normal image fitting |
| TGS/Lottie | Rust document handling and statically linked rlottie | Canonical identity, validation, tint, sampled renderer frames and timing |
| Video | Rust-owned FFmpeg/FFprobe processes | Alpha-aware decoding, full sampled timeline, VP9 conversion and bounded process custody |
| Logo matching | Rust distance calculations | Exact byte distances, stable first-match ordering and persisted cache identity |

The vendored rlottie source is the renderer revision used by the historical
`rlottie-python` dependency. Its provenance and licenses are retained in
[`runtime/vendor/rlottie`](../runtime/vendor/rlottie/NUMERA-SOURCE.md). These native
libraries compile into the application; end users do not install their language
runtimes. FFmpeg and FFprobe remain external executables for video operations.

PNG and WebP encoders can produce different compressed bytes from historical
Pillow output. Decoded pixels, durable content identity and declared frame timing
are the acceptance boundary. Tests retain independent historical inputs and
expected identities rather than deriving new expected keys from the new engine.

Logo matching accumulates squared byte distances exactly in `f64`. It preserves
stable first-match ties and the frozen remapping cases. This removes NumPy's
architecture-dependent `float32` cancellation for identical or nearly identical
high-intensity images; arbitrary BLAS rounding artifacts are not a compatibility
contract.

## Preview ownership and caching

The panel dispatches codec requests to the same executable. Requests and replies
use distinct, bounded JSON files in one physical project-owned directory. New
outputs must not already exist or alias the request, response or cleanup marker.
A failed or uncertain owner retains its evidence instead of deleting possibly
active scratch.

Opening a catalog primes still previews for the first eight cards. Animated
previews are generated on demand and cached; repeated requests reuse completed
files. The browser keeps its default **24 simultaneous animations**, along with
its visibility, scrolling and background-tab controls. Native backend conversion
does not replace the browser's own animation decoder.

FFmpeg children retain the established process owner, wall/idle deadlines,
bounded captures and cleanup checks. On Windows the owner waits for verified
process handles to signal termination as well as the Job's active-process count.
Linux test execution retains private network namespaces and descendant custody.

## Install a package

Download a matching artifact from **Prebuilt packages**, or a published release,
and extract it into a new folder. Packages support Linux x64, Windows x64 and
Intel/Apple Silicon macOS. They contain the application, public assets, native
manifest, launchers, license and sample configuration. Schema2 packages contain
no Python wheels, interpreter or virtual environment.

```bash
./numera-emoji --help
./numera-emoji doctor
./run.sh panel
```

```powershell
.\numera-emoji.exe --help
.\run.ps1 -Check
.\run.ps1
```

The extracted executable can also verify and copy an installation offline:

```bash
./numera-emoji package-verify
./numera-emoji install-prebuilt --destination /absolute/path/to/new-installation
```

The installer verifies manifest/platform/hashes and refuses links, traversal,
unlisted package payloads and an unsafe replacement. Configuration, logs and
operator data belong to the installation; preserve them before switching an
existing installation. A manifest hash is an integrity check, not a publisher
signature.

Copy `.env.example` to `.env` and configure Telegram/provider credentials before
using commands that access those services. `--help` and local conversion do not
require credentials. FFmpeg and FFprobe must be on PATH for video commands.

## Build from source

Source builds require the pinned Rust toolchain, a C/C++ compiler and CMake for
bundled native libraries. They do not require Python:

```bash
cargo build --manifest-path runtime/Cargo.toml --locked --release
runtime/target/release/numera-emoji --help
```

Use the standalone test runner for production tests:

```bash
cargo build --manifest-path runtime/Cargo.toml --locked --release --bin numera-runtime-tests
cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked -- --test-threads=1
```

The launchers select an existing executable. They do not build an application or
install dependencies during normal operation.

## Historical regression and contributor tools

`native/Cargo.toml` retains the original PyO3 library and a `numera-compat`
developer executable. They run historical source observations, fault-injection
cases and original regression suites. `tests/` and retained reference modules are
acceptance evidence; they are excluded from production packages. The ordinary
application never falls back to them.

The compatibility crate's public `numera-emoji` binary is a small shim calling
the standalone Rust library, so public CLI, collection and browser integration
tests exercise the production implementation. Source-observation and legacy
tooling cases explicitly invoke `numera-compat`. Building that optional developer
toolkit still requires its pinned CPython/codec environment.

CI retains all original test cohorts, adds standalone engine/package/operator
checks, and builds the production executable on every supported platform. See
[CI acceptance](rust-ci-status.md) for the current verification status.
