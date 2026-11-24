# Test inventory and first-party language policy

This Rust tool checks the structural migration inventory. It does not execute product tests or prove their behavioral assertions. Runtime, browser, fault-process and platform CI remain separate requirements.

The public source manifest is `tests/fixtures/migration/reference-cases.json`. Its1516 identities were collected with the original `unittest.defaultTestLoader`, including26 inherited executions, at reference `a39a049c02f8dcc90d18fe8771869f82dd655221`. The actual main revision `9c3cbb43305b0e507085003d4a33806e4a60b677` has1515; the reference adds `tests.test_pack_locks_exclusion.JournalRecordsAreComplete.test_separate_child_journals_keep_overlapping_owners_visible`.

The manifest pins each original identity, defining class, inheritance, source-file SHA256, method-source SHA256, test category and genuine platform skip. The sorted original ID digest and immutable source receipt digest are pinned in the Rust tool. Updating target wiring cannot silently replace originals or change the84 browser,13 browser-performance,59 served-source contract and1360 backend boundaries.

The separate `original-main-rust.json` records35 original Rust declarations:34 applicable declarations (32 product behaviors and2 technical subprocess entrypoints) and the explicitly retired macOS-only test. These declarations never inflate the1516 Python identity count. Linux and Windows each originally collect32 of those Rust declarations. The separate inventory preserves their original platform gates.

Run from the repository root:

```sh
# Build the actual release isolation runner first (see the contributor guide).
cargo test --config .cargo/runtime-tests.toml \
  --manifest-path tools/test-inventory/Cargo.toml --locked
cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- \
  --mode rust --output logs/checkpoint/rust-inventory.json
cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- \
  --mode all --playwright-list logs/checkpoint/browser-inventory.json \
  --output logs/checkpoint/complete-inventory.json
cargo run --manifest-path tools/test-inventory/Cargo.toml --locked -- \
  --mode language --output logs/checkpoint/language-policy.json
```

The browser input must be actual `playwright test --list --reporter=json` output from the unfiltered156-case collection. All mode requires that input; Rust mode explicitly leaves JavaScript collection unverified. Discovery containing skipped specs, duplicate/missing identities, different files, or repeated project executions fails.

The tool obtains actual runtime test crate roots from offline, locked Cargo metadata. It follows their Rust modules with `syn`, including `#[path]`, inline modules, platform cfg and conditional ignore reasons. Helpers, comments, fixture strings, and unwired source files cannot supply a test declaration. Every mapped Rust original needs its own actual nonempty `#[test]` function with matching target, module identity and body-token hash. Item cfg must not remove a Python-origin identity on either supported OS: use a genuine conditional ignore and preserve the platform body. A documented reviewed native adaptation is explicit manifest data.

The inventory models the required CI command's release profile and Cargo default feature closure, rather than the profile used to compile this tool. A target whose `required-features` are inactive cannot supply a test. Cargo metadata supplies the feature definitions, including implicit dependency features; release `debug-assertions` is read from the runtime manifest. The report records this context. Linux x64 GNU and Windows x64 MSVC predicates are modeled explicitly. Unsupported custom predicates remain unknown through negation and cannot establish reachability. Effective `cfg_attr` test/Tokio attributes participate in discovery; potentially test-bearing declarations remain visible to the unwired-source guard even when their condition is inactive.

The report includes assertion/call facts and source hashes for review. A body containing an assertion is not automatically a meaningful parity test. Individual original variants, positive controls, real faults and process fixtures still need behavioral review and execution evidence; the report never claims those are proven by the inventory.

Rust and all modes also scan actual test declarations throughout `runtime/src` and `runtime/tests`, plus first-party shared files reached during consolidation. Every declaration, including supplemental tests and technical subprocess entrypoints with no original identity, must have a real Cargo test context on Linux or Windows. Dropping a module, disabling it on both platforms, or leaving a new private test file unwired fails this check. Ordinary `#[test]` and `#[tokio::test]` declarations are parsed as Rust; raw strings and comments are not declarations. Genuine unsupported-platform retirements require a separate exact file/function/body receipt in `supplemental-retirements.json`; the table currently has no exemptions. This graph check does not count helpers as original identities and does not prove execution.

The explicit test configuration applies the isolation runner only to tests. A global target runner in `.cargo/config` or `.cargo/config.toml` is rejected because Cargo also applies it to ordinary `cargo run`. The checker parses real TOML, including quoted, dotted and inline target tables. Production inventory commands run directly.

Language mode checks the Git candidate tree: tracked files still present and visible untracked files, excluding ignored local/build material. It does not read private files selected accidentally; it rejects their publication paths. Links and reparse points, including Git symlink entries materialized as text, are rejected. CI runs this mode alongside the original-case inventory.

First-party application, tooling, fixtures that execute code, and tests use Rust. JavaScript/HTML/CSS files are limited to `assets` and the panel declaration file; `assets/tests/launcher-process.js` separately exercises the reviewed source-checkout launcher. The Worker keeps TypeScript below `worker/src` and `worker/test`, and SQL migrations. The only PowerShell exception is the root `run.ps1`, pinned to its reviewed SHA-256 in `language-policy.json`. Release archives do not include it. Other Python, PowerShell, shell launchers and first-party native-language source are rejected even when placed under a fixture directory. Rust code is parsed to detect actual retired interpreter imports and static process handoffs, including lexical immutable let/const/static values and straightforward process Command type aliases. File/module/block scopes and local shadowing are preserved. Quoted negative-test examples and historical source paths are inert data. This is a reviewed source-policy check, not a proof about arbitrary computed programs, cross-file dataflow or dynamically generated commands.

`language-policy.json` pins the reviewed rlottie vendor subset to its actual upstream commit and SHA256 bytes, with required provenance and redistribution licenses. A vendor directory is not a general language exemption. The same policy records exact frozen data fixtures, including intentionally corrupt media, malformed/nonfinite JSON and source-parser input strings. New ordinary JSON, Unicode tables, CSV, SHA256 receipts, media, documentation examples and declarative Cargo/CI/Worker/license templates remain valid through their respective data checks. A renamed executable script does not gain an exception from a data extension. Real Rust include templates ending in `.rs.in` are parsed as Rust.

The tool's tests create actual temporary Git/Cargo candidate trees. Positive controls accompany real unwired/disabled-module, inline-runner, vendor-tamper, interpreter-handoff, disguised-data, link and executable-mode negatives. Release/profile, conditional-test-attribute and required-feature controls compare against actual temporary `cargo test --release -- --list` collections. A small Rust build script resolves the real compiler and rustdoc paths from Cargo's compiler sysroot so those isolated fixtures work without operator toolchain configuration. A green language report does not retire any original test or establish Linux/Windows behavioral parity.

While authoring, place the complete original identity immediately above its actual test:

```rust
// OriginalID: tests.test_example.Contract.test_actual_behavior
#[test]
fn actual_behavior() {
    // Exercise the actual production boundary and preserve the original assertions.
}
```

`--refresh --mode rust` refreshes mutable mappings from those declarations and updates body hashes. It refuses ambiguous claims and parse errors. Missing authorship remains an error, even when other mappings are refreshed. Refreshing a body hash requires reviewing the resulting diff; it is not test execution or automatic retirement approval. The original source facts remain immutable.

No original Python sources, private research files or interpreter are needed by this tool. Temporary tool tests use isolated synthetic Rust projects. The standalone tool has its own Cargo lock and build directory.
