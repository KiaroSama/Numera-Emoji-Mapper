# Rust panel contracts

The panel server, catalog, save/order routes, task/event state and preview worker
are Rust. The browser HTML/CSS/JavaScript retains its language and behavior.
Cloudflare Worker code remains separate TypeScript.

## Observable behavior

The native server preserves the panel's session identity, CSRF/origin and content
validation, hidden selections, stable ordering, held/moved plan intent, pack
membership and configured branding. Catalog failures produce useful refusals;
a cached page retains the HH:MM of its last successful refresh, including the
startup warm-up path. A failed refresh never claims new data.

Concurrent save/order/page requests use the actual shared view mutex and SQLite
transactions. Tests suspend real lock/sort boundaries and observe committed
order, exclusions, served items and media. The Rust mutex replaces an old shared
Python-container implementation without replacing the concurrency scenarios.

An ordinary second panel launch may reuse a matching live session. An owned
sandbox must keep its own lifetime/lease and may not silently reuse an unrelated
listener. Sandbox snapshots, source immutability, child ownership and recovery
markers are independently exercised.

## Previews

Codec work uses the same native executable with bounded request/response files
and owned scratch. Initial warm-up primes the first eight static previews.
Animated previews remain on demand; the browser's default animation budget stays
24 visible cards. Visibility, scrolling, background-tab and failed-preview rules
remain browser behavior.

Tests retain real static/TGS/VP9 media, independent expected preview bytes where
required, alpha/motion/timing checks, cache reuse and bad-file continuation.
Cancellation must settle codec descendants before deleting their workspace.
Uncertain cleanup retains diagnostics and fails visibly.

## Verification

Rust panel/view/server cases live in `runtime/tests/` and private unit modules.
Playwright's 84 interaction and 13 performance cases run actual Chromium; another
59 cases check the actual served scripts. The browser fixture owns its catalog,
disables dotenv, blocks external page requests and closes its server/processes.

See [test identities and admission](native-test-migration.md) and
[panel test instructions](../assets/tests/README.md). Platform builds and full
final-head workflow results remain required even after a local panel test passes.
