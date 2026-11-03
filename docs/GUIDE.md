# Numera Emoji Mapper — Complete Guide (0 → 100)

The complete reference is organized by responsibility. Section numbers are stable;
all workflows, safety guarantees and historical explanations remain linked below.
Normal installation uses prebuilt native packages; no local Rust/Cargo build is required.

1. [Getting started, installation, launcher and safety (§1–11)](guide-start.md)
2. [Collection/build CLI (§12.1–12.5)](guide-cli.md) and [panel, bot, coin and Worker CLI (§12.6–12.9)](guide-cli-panel-coin.md)
3. [Catalog, media, identity and bot contracts (§13–19)](guide-data-media.md)
4. [Curate panel, saved intent and execution (§20)](guide-panel.md), with [native panel migration evidence boundaries](native-panel-migration.md)
5. [Troubleshooting, glossary, security and bug history (§21–28)](guide-reference.md)
6. [Command cheat-sheet and worked workflows (Appendices A–D)](guide-workflows.md)

See [native backend ownership and installation](native-backend.md),
[expanded first-party migration status](native-backend.md#expanded-first-party-migration-in-progress)
and [the migration decision](adr/0003-rust-local-backend.md). The additive
`source-inventory`, `package-native`, `package-verify`, `benchmark-similarity`,
`source-lottie-render`, `source-static-images`, `test-runner`, `runtime-check` and
`native-convert-owner` contributor commands are
documented with their pending coverage/runtime gates; they are not activated in
the installed package. `runtime-check --extension FILE` adds an explicit native
extension check without importing application Python; execution evidence is pending.
The explicit [`test-plan`, `test-runner` and `test-cargo` observation candidates](native-launcher.md#compiled-case-observation-candidate)
snapshot pre-build inputs and record hash-bound libtest dispositions as untrusted
observations. The Cargo wrapper retains one original invocation with split local
raw logs, never verified coverage or source-retirement approval; compilation,
execution and remaining platform workflow integration remain pending. Main Linux,
native browser, Windows and migration-checkpoint steps select the wrapper, but have
not executed it.
The additive [`menu`, `doctor`, transitional `check` and offline `install-prebuilt` commands](native-launcher.md)
have native execution wiring and authored tooling tests. The transitional
`check --python FILE|NAME` override requires a compatible CPython3.11 venv and
agreement between independent and embedded runtime probes; base/other-version
interpreters remain unsupported. Its ordered `find_spec` gate retains Python
truthiness and dotted-parent import behavior without importing dependency targets.
Compilation, actual console behavior and
installed acceptance remain pending. Native installation
refuses before environment creation while signing/identity/logo/rollback admission
is incomplete. Retained launch and installation procedures are unchanged. The separate `numera-build` contributor candidate is not an activated
build/update route; complete build, signing, platform and rollback gates remain open.
Native call-time log-retention, ID-inventory duplicate/prose diagnostics,
requested-ID codec integration and source redaction/retention replays are also
migration-branch candidates, not verified installed behavior; see the ownership
reference for their exact coverage limits.
Source inventory follows inherited/imported candidates
across pinned files without executing imports; dynamic collection remains
unverified. [Native test migration evidence](native-test-migration.md) separates
registered bindings, historical prerequisite assertions, CI ownership and static
lock-order policy from actual runtime and package acceptance. The
[native codec compatibility contracts](native-codec-contracts.md) document the
source-authored alpha, size-observation, error, warning-log and cleanup changes;
their new compiler/runtime acceptance is still pending. Tokens/configuration and
operator collections remain local. The additive coin-remap dependency diagnostic
and its pending runtime evidence are documented in the [coin CLI reference](guide-cli-panel-coin.md).
Do not test publishing against live packs.
