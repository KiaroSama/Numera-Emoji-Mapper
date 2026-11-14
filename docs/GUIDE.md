# Numera Emoji Mapper — Complete Guide (0 → 100)

The local application runs from the standalone Rust executable in the package
root: `.\numera-emoji.exe` on Windows or `./numera-emoji` on Linux/macOS.
Normal use requires no Python, virtual environment, pip, NumPy or Rust toolchain.
FFmpeg and FFprobe are needed for video emoji. Keep the executable beside its
packaged assets, libraries and configuration template.

1. [Getting started, installation, launcher and safety (§1–11)](guide-start.md)
2. [Collection/build CLI (§12.1–12.5)](guide-cli.md) and [panel, bot, coin and Worker CLI (§12.6–12.9)](guide-cli-panel-coin.md)
3. [Catalog, media, identity and bot contracts (§13–19)](guide-data-media.md)
4. [Curate panel, saved intent and execution (§20)](guide-panel.md)
5. [Troubleshooting, glossary, security and bug history (§21–28)](guide-reference.md)
6. [Command cheat-sheet and worked workflows (Appendices A–D)](guide-workflows.md)

Start with `numera-emoji doctor`, then `numera-emoji menu` or a command's
`--help`. `run.ps1` and `run.sh` launch an existing executable; they never build
it or install prerequisites. [Launcher and packaging details](native-launcher.md)
cover package verification, offline copying and explicit contributor builds.

Rust owns the local backend, media worker, raster/SVG/Lottie handling and coin
matching. The panel remains browser JavaScript; `worker/` remains Cloudflare
TypeScript. The panel's default animation budget remains 24 visible cards.

[Native codec contracts](native-codec-contracts.md) explain stable pixel identities,
alpha handling, validation and recovery. Encoded PNG/WebP bytes may differ from
historical encoders while decoded pixels and catalogue identities remain stable.

The optional `numera-compat` developer harness retains historical Python/PyO3
oracles and test tools outside the production dependency graph. Historical ADRs,
audit reports and migration notes describe their recorded checkpoints; they are
not current installation instructions. See [backend ownership](native-backend.md)
and [test evidence](native-test-migration.md) for those boundaries.

Tokens, configuration, collections and recovery journals remain local. Use isolated
fixtures for development; do not test publishing against live packs.
