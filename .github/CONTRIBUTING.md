# Contributing

Numera Emoji Mapper is free software under the **GNU General Public License v3 or
later** (see [LICENSE](../LICENSE)).

## External contributions

Pull requests are welcome. By submitting one you confirm that you have the right
to contribute the code and that it is offered under the same GPL-3.0-or-later
terms as the rest of the project; contributors keep their own copyright.

## Reporting issues

Bug reports and feature suggestions are welcome via the GitHub issue tracker.
When reporting a bug, please include:

- the command you ran, with secrets redacted,
- the workflow used (general or crypto-coin),
- the application version/build and operating system,
- the full error output, without real bot tokens or API keys.

For source-build problems, also include the Rust version and compiler/linker
error. A Python version is relevant only when reporting an optional compatibility
harness failure.

## Security

Do not file security issues publicly. Follow the process in
[SECURITY.md](SECURITY.md).

## Production Rust development

The standalone application lives in `runtime/Cargo.toml` and reuses shared Rust
modules from `native/src/`. Install the manifest's Rust toolchain (currently
Rust 1.99), a platform C/C++ compiler and linker, and CMake for bundled native
codecs. FFmpeg and FFprobe must be on `PATH` for the full video test cohort.
Python, pip, virtual environments and NumPy are not production build or runtime
requirements.

From the repository root, use the same production build and test commands as CI:

```powershell
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
cargo fmt --manifest-path runtime/Cargo.toml --check
cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings
cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked -- --test-threads=1
```

Build `--release --bins` before testing: the explicit Cargo runner configuration
uses `runtime/target/release/numera-runtime-tests` (`.exe` on Windows), including
when Cargo builds the tests in the debug profile. The runner scrubs credentials,
disables dotenv, selects loopback fixtures and owns test process trees. On Linux
its private-network bootstrap must be available; report a failed prerequisite
rather than bypassing the official runner or counting an unexecuted test as
passing. See the current CI job and [test documentation](../tests/README.md) for
platform setup and the exact cohorts.

For an operator check use the root packaged executable's `doctor` command, or
`run.ps1 -Check` / `run.sh --check`. Launchers only run an existing executable;
they do not build, install dependencies or run contributor suites. Production
package and launcher details are in [native-launcher.md](../docs/native-launcher.md).

## Historical compatibility and browser checks

The separate `native/Cargo.toml` crate retains the optional `numera-compat`
Python/PyO3 oracle and fault-injection harness. Its Python environment, pinned
requirements, Ruff and Playwright/Chromium are developer-test dependencies,
separate from the production graph. Follow [tests/README.md](../tests/README.md)
for that environment and each historical cohort before invoking its checks.

The retained Python check entry point is:

```powershell
.\scripts\check.ps1
.\scripts\check.ps1 -Tests test_catalog,test_panel_plan -SkipCompile -SkipLint
```

`-Tests` runs package-qualified `tests.<name>` suites so the credential/network
guard in `tests/__init__.py` remains active. If invoking discovery directly, keep
its exact form, including `-t .`:

```text
python -m unittest discover -s tests -t . -p "test_*.py"
```

Compatibility Rust suites use `.cargo/tests.toml` and the separately built
`numera-compat` runner, rather than the production runner. Keep prerequisite
failures visible and follow their documented setup; installing compatibility
requirements is not a way to repair the standalone application's doctor.

For retained Python lint, use `ruff check .` without command-line rule overrides:
`ruff.toml` owns selection and exclusions. Do not silence failures with
`--exit-zero` or `continue-on-error`.

The panel remains browser JavaScript and needs real browser tests when behavior
changes. The Cloudflare Worker remains TypeScript; run its committed-lockfile
checks separately:

```bash
cd worker
npm ci
npm run typecheck
npx vitest run
```

## Test preservation and safe verification

Keep every original test case and test type/cohort. Update the analogous native
test when implementation behavior changes; do not delete, disable, ignore or
replace historical assertions to obtain a green run. Preserve independent
fixtures and oracle evidence rather than generating expected identities from
the replacement implementation.

Run relevant checks before claiming success and report exactly what executed.
A listing, ignored case, observation manifest or process exit alone is not proof
that a cohort's assertions passed. Keep raw diagnostic and observation logs local
unless they have been reviewed and redacted.

Use isolated data and loopback services for tests. Never load operator credentials
or mutate live Telegram packs to test a change. Do not weaken network/credential
guards to work around an environment failure. Preserve existing lock, migration,
recovery and cleanup assertions alongside codec, browser and package checks.

Keep code comments and documentation in English, match the existing style, and
never commit `.env`, `secrets.md`, operator collections, logs or real credentials.
