# Test identities, types and native admission

The migration baseline is actual unittest discovery at
`feature/first-party-rust` revision
`45a96f9de70da8429b83c047e0626b6ed6694b20`: **1,516 unique application IDs**.
Main revision `9c3cbb43305b0e507085003d4a33806e4a60b677` collected 1,515.
The reference adds one overlapping-owner journal case. Discovery includes 26
inherited cases; declaration counts alone do not describe the executed suite.

The public manifest under `tests/fixtures/migration/` records immutable source
identities, receipts and explicit target mappings. The Rust inventory tool checks
actual wired test functions, duplicate/unknown IDs and platform dispositions.
It does not certify semantics merely because names or totals match.

## Case ownership

| Original category | Cases | New execution boundary |
|---|---:|---|
| Backend/application | 1,360 | Rust unit, integration, CLI, transport, SQLite, process and codec tests |
| Browser interaction | 84 | Actual Playwright/Chromium |
| Browser performance | 13 | Original performance scenarios and budgets |
| Served-source contracts | 59 | JavaScript tests over actual served assets |

The separate Worker remains TypeScript and retains its Vitest suite. Original
main Rust tests are inventoried separately: 35 declarations, of which 34 apply to
Linux/Windows (32 behavior tests and two technical child fixtures). One macOS-only
case is retired with that platform. Child workers and additional native
regressions do not inflate the 1,516-case application inventory.

Each mapped case retains its original identity and assertion category. Original
loops/subcases stay within their original case. Genuine platform/capability skips
remain discoverable and state their reason; a no-op passing body is not a skip.
Windows kernel-handle behavior needs actual Windows execution. Source counts,
local Linux passes and final hosted results are separate evidence.

## What native adaptation means

Tests exercise the actual application flow and inject failures at the original
boundary. Private lazy factories preserve normal construction order and defaults;
there is no alternate test implementation of business policy. HTTP fixtures use
numeric loopback. Crash tests inspect actual durable journals, catalog rows,
owned descendants and recovery behavior.

Language boundaries change explicitly:

- Rust compiler admission replaces extension import/type extraction; positive
  controls link and execute the actual production crate.
- Results and native exit codes replace Python exception objects. The existing
  native ambiguous-mutation fence remains: an unproved add/create transport
  result is not blindly retried. Its legacy no-proof case is an explicit policy
  adaptation, not literal old output parity.
- Explicit native secret lists redact even short values. Missing required logos
  return an error rather than a nullable compatibility wrapper.
- Rust traits and structural controls replace dynamic fake-client signatures.
  Typed HTTP, process and codec tests retain independent behavioral assertions.
- OS-owned leases replace stale-time ownership guesses. A diagnostic heartbeat
  never authorizes taking a live lock. Uncertain sandbox/descendant cleanup keeps
  its recovery marker and evidence rather than claiming safe automatic deletion.
- Installed native binaries, notices and dependent libraries replace wheel/ABI
  repair and shell-launcher transport. No interpreter is used by these tests.

Frozen media/JSON expectations retain their independent source receipts. Codec
expected values are not regenerated from the new implementation to make a test
pass. Exact preview bytes are preserved where the contract requires them;
decoded pixel identity and timing remain explicit elsewhere.

The retired PyO3/source-oracle apparatus tested an interpreter bridge that no
longer ships. Its product behavior is covered by the mapped native cases and
retained native regressions; Python AST/import replay is replaced by Rust
source-graph/compiler admission and frozen provenance. Removing that apparatus
is separate from deleting an application behavior or its original test ID.

## Running tests

From the repository root, with FFmpeg and FFprobe installed:

```bash
cargo build --manifest-path runtime/Cargo.toml --locked --release --bins
cargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release -- --test-threads=1
cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- --mode rust
```

The CLI provenance tests read bounded historical Git blobs at the immutable
reference above; they never import or execute the retired source. Use a full Git
checkout. If a shallow clone or a later squash merge omitted that commit, fetch
it before entering the isolated test environment:

```bash
git fetch --no-tags --depth=1 https://github.com/KiaroSama/Numera-Emoji-Mapper.git 45a96f9de70da8429b83c047e0626b6ed6694b20
```

CI checks the object's availability and fetches it only when missing. Frozen JSON
and media expectations remain committed data with independently checked hashes.
Installed application packages do not require Git.

The runner clears credentials, disables dotenv, confines temporary state and owns
descendants. Linux adds a private user/network namespace with loopback and drops
privileges. Missing required isolation fails before application cases begin.
Windows uses the native process owner and explicit admitted test context; no
arbitrary-socket kernel isolation claim is made for Windows.

Every libtest crate checks admission before its test bodies start. Bare
`cargo test` and direct libtest execution refuse to run without that context.
Select `.cargo/runtime-tests.toml` explicitly as above, or use `numera-dev test`
or `numera-dev check`. There is no global Cargo runner: ordinary `cargo run`
keeps application arguments, configuration and networking unchanged.

Browser discovery and execution:

```bash
cd assets
npm ci
npx playwright install chromium
npx playwright test --list --reporter=json > ../logs/checkpoint/browser-inventory.json
npm test
cd ..
cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- --mode all --playwright-list logs/checkpoint/browser-inventory.json
```

The combined inventory must bind to actual Playwright discovery: 97 UI cases
(including 13 performance cases) plus 59 source contracts. CI executes all Cargo
targets on Linux and Windows, and all Playwright specs once. A green declaration
counter is not a substitute for those executions.

Hosted workflow logs record the exact checked-out head SHA. Use checks and
artifacts on that commit when assessing completion; an earlier green run does
not certify later edits. No document substitutes for final Windows/CI evidence.
