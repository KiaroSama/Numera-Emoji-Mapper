# Panel tests

The standalone Rust panel is tested with JavaScript Playwright and Chromium.
Node.js 22.13 or later is required for the built-in SQLite fixture driver.

```sh
cargo build --release --manifest-path runtime/Cargo.toml
npm ci --prefix assets
npx --prefix assets playwright install chromium
npm test --prefix assets
```

`npm run --prefix assets test:browser` runs the original 97 browser and
performance methods: panel browser (29), save queues (23), curation (32), and
performance (13). `npm run --prefix assets test:contracts` runs the 59
served-text page contracts: actions (34) and grid (25). The complete suite has
156 cases. Each test title retains its original module, class, and method;
`case-map.json` records the exact identities and assertion types.

`NUMERA_BROWSER_BINARY` overrides the default
`runtime/target/release/numera-emoji` executable (with `.exe` on Windows).
`NUMERA_BROWSER_EXECUTABLE` selects an installed Chromium executable;
`NUMERA_EMOJI_MAPPER_BROWSER_CHANNEL` selects a Playwright browser channel.
Missing browsers or binaries fail; the original explicit
`NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1` opt-out skips the 97 browser cases.

The fixture owns an isolated project and catalog under `logs/tmp/browser/fixtures/`,
starts the Rust HTTP server, and reads committed SQLite state after real saves.
It inherits only process-launch environment variables, disables dotenv, and
blocks nonlocal page requests. No credentials or remote APIs are needed.
Synthetic item models retain the original page scripts, CSS and HTTP routes.
Each test closes its browser contexts; the worker stops its server and removes
its isolated fixture. Failed-run screenshots and diagnostics are under
`logs/tmp/browser/results/`.

The browser fetch gate, denied-storage script, performance probe and seeded
bulk reference retain the original fixtures. The WebP fixtures match the
original generated 104-by-104 images: 28 frames, 66 ms each, a 32-by-32 blue
square moving three pixels per frame, quality 60, encoding method 4. The
checkerboard test still decodes real Chromium screenshot pixels and checks the
same four positions with the same tolerance. Performance logs retain the
`PANEL_PERF` format; timing values are reported and stable budgets are asserted.
