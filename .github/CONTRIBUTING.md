# Contributing

Numera Emoji Mapper is free software under the **GNU General Public License v3 or
later** (see [LICENSE](../LICENSE)). Pull requests are welcome. By submitting one
you confirm that you have the right to contribute it under GPL-3.0-or-later;
contributors keep their own copyright.

## Reporting issues and security

Include the application version or source revision, Linux/Windows version and CPU architecture,
command, general/coin workflow and complete redacted error output. For source
build failures include the Rust compiler and linker versions. Never include real
tokens, API keys or private collection data. Report vulnerabilities privately as
described in [SECURITY.md](SECURITY.md).

## Source development

First-party application and contributor tools live in `runtime/src/`. The panel
retains HTML/CSS/JavaScript in `assets/`; the Cloudflare Worker retains TypeScript
and SQL in `worker/`. Native C/C++ dependency internals are built through Cargo;
they do not introduce a first-party interpreter or extension layer.

Install the compiler pinned in `rust-toolchain.toml` (Rust 1.100.0), rustfmt,
Clippy, CMake and a C/C++ compiler/linker. Windows builds use the matching x64 or arm64 MSVC tools. Install
FFmpeg and FFprobe for the video cohorts. Node 24 and Chromium are contributor
requirements for the browser/Worker suites, not installed application requirements.

From the repository root:

```sh
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
cargo fmt --manifest-path runtime/Cargo.toml --check
cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings
cargo test --config tests/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release --no-fail-fast -- --test-threads=1
```

The explicit runner configuration uses the release `numera-runtime-tests`
executable, with `.exe` on Windows. Build release binaries first. The runner
scrubs credentials, disables dotenv, confines temporary storage, owns descendant
processes and uses a private network on Linux. Missing prerequisites must fail
visibly; do not bypass the runner or disable a guard to obtain a passing result.
See [test documentation](../tests/README.md) and the current CI workflow.

The built `numera-dev` binary provides `build`, `check`, `test`, `panel-sandbox`
and `benchmark-similarity`; use its `--help` for selection and timeout options.
`numera-emoji doctor` is an offline operator check, while `numera-dev check` runs
contributor checks. Neither command installs prerequisites automatically.

## Browser and Worker checks

Browser tests use the real Rust server and Chromium:

```sh
npm ci --prefix assets
npx --prefix assets playwright install chromium
npm test --prefix assets
```

Run the separate Worker checks with its committed lockfile:

```sh
npm ci --prefix worker
npm run typecheck --prefix worker
npm test --prefix worker
```

For the panel's JavaScript type check, run
`npx tsc -p ../assets/jsconfig.json --noEmit` from `worker/` after `npm ci`.
Use [browser test documentation](../assets/tests/README.md) for binary/browser
selection. Missing Chromium is a failed prerequisite, not a successful skip.

## Coverage and review

Preserve each original case, category, fault and variant. The 1,516-case baseline
contains 1,360 backend cases, 84 browser cases, 13 browser performance cases and
59 served-source contracts. The 34 applicable original-main Rust declarations
and 119 Worker cases are separate inventories. A Rust declaration or inventory
match alone does not prove its assertions passed.

Keep independent fixture provenance; do not derive replacement expectations from
the new implementation. Preserve real lock, crash, recovery, cancellation,
credential/network, browser and package assertions. See the
[inventory tool](../tools/test-inventory/README.md) and
[migration evidence](../docs/native-test-migration.md).

Run the relevant checks and report what actually executed, including platform
limits and explicit skips. CI acceptance belongs to the exact source revision;
a passing earlier run or another branch does not establish the current result.
Never use real Telegram packs or operator credentials for verification.

Keep source comments and documentation in English. Do not commit `.env`,
`secrets.md`, operator collections, logs, caches or real credentials. Historical
ADRs and audits retain their recorded context; they are not current setup guides.
