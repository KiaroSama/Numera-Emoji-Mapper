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

- the command you ran (with secrets redacted),
- the workflow used (general or crypto-coin),
- your Python version and operating system,
- the full error output (never include real bot tokens or API keys).

## Security

Do not file security issues publicly. Follow the process in
[SECURITY.md](SECURITY.md).

## Local development

```powershell
py -3.11 -m venv .venv
# Everything the check gate needs, in one line (core, coins, ruff, playwright):
.venv\Scripts\python.exe -m pip install -r requirements.txt -r requirements-coins.txt -r requirements-dev.txt; .venv\Scripts\python.exe -m playwright install chromium
.\run.ps1 -Check      # environment doctor (non-interactive)
.\scripts\check.ps1   # retained Python checks: byte-compile + ruff + full unit suite
.\scripts\check.ps1 -Tests test_catalog,test_panel_plan -SkipCompile -SkipLint   # chosen suites only
```

`check.ps1` names the one install command above and exits 2 if anything it
needs is missing. `-Tests` runs suites as `tests.<name>`, so the test guard in
`tests/__init__.py` still applies.

Lint is `ruff check .` with no arguments; `ruff.toml` at the repo root owns the
rule set and the exclusions. Do not pass `--select`/`--exclude` on the command
line and do not silence a finding with `--exit-zero` — a lint stage that cannot
fail is worse than none.

Run `scripts\check.ps1` before every commit rather than a hand-written
`unittest` command: the suite must be started as
`python -m unittest discover -s tests -t . -p "test_*.py"`, and dropping `-t .`
disables the test-suite credential/network guard (see
[`tests/README.md`](../tests/README.md)).

These retained Python checks do not cover the additive Rust migration suites.
See the [native verification boundaries](../docs/native-backend.md) and
[test documentation](../tests/README.md) for native prerequisites, explicit Cargo
runner configuration and remaining acceptance gates. The authored Linux
[network bootstrap](../docs/native-launcher.md#native-test-runner-network-boundary)
has not been compiled or tested on a host. Windows lacks that OS egress boundary,
and macOS currently refuses. Optional [compiled-case observations](../docs/native-launcher.md#compiled-case-observation-candidate)
remain untrusted: never upload raw result reasons or the entire observation store,
and never convert a listed/ignored case or exit0 into verified coverage. The additive
`test-cargo` wrapper keeps one original Cargo invocation and checks its per-target
completion receipts; Linux/browser/Windows workflow selection is source-authored,
not executed acceptance. Raw split build logs and manifests stay local.
New migration assertions remain uncompiled and unexecuted; the contributor builder/installer refuses before building
or activating a replacement until signing, identity, logo and rollback admission
are integrated. All three hosted workflows require manual dispatch; a push, pull
request or version tag alone does not run verification.

Keep code comments and documentation in English, match the existing style, and
never commit `.env`, `secrets.md`, or any real credentials.
