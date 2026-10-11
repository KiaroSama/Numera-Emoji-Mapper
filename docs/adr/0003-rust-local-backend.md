# 0003: Rust-owned local backend with bounded Python compatibility

Date: 2026-10-11
Status: Accepted; integrated retirement and four-platform package acceptance verified

## Context

ADR 0002 moved exact similarity computation to Rust while Python still owned
application decisions. The approved migration extends native ownership to the
whole local backend without changing durable catalog identity, persisted schemas,
published IDs, saved intent, the JavaScript panel or the Cloudflare Worker.
A facade that forwards entire commands to Python would not meet that objective.

Media replacement carries a different risk: decoding, transparent pixels, Lottie
rendering and VP9 alpha determine content keys already used by live collections.
Coin-provider/domain workflows are outside the migration scope but depend on
shared transport, state and rendering APIs. Both need explicit boundaries rather
than broad deletion or a second implementation retained as an automatic fallback.

## Decision

Use the installed `numera-emoji` executable as the primary command owner for
collecting, publishing, curation, plan application, conversion orchestration,
reporting, archiving, bot polling and identity migration/recovery. Rust owns
SQLite, validation, tri-state outcomes, lock/atomic-state operations, verified
mutation retries, HTTP/cache policy and filesystem/report decisions. Windows menu
and doctor route to the installed runtime; normal installation never invokes Cargo.

Retain three declared Python categories:

- Exact codec/identity leaves behind a bounded media IPC contract, preserving
  existing Pillow/SVG/Lottie/FFmpeg results; no application/catalog/network callback.
- Thin native-backed shared APIs plus configuration/logging glue for unchanged
  coin-domain tools. Requests and optional coin-remap NumPy remain scoped there.
- Development clone/snapshot/build/install/process-owner support and test-only
  source references/oracles. These are not production application fallbacks.

Distribute matching executable/extension and binary dependency wheels in checked
platform archives. Installation uses Python 3.11 offline, verifies hashes/platform
and refuses to modify an existing environment. Updates use a separate extraction;
codec dependencies, public assets and required extension libraries remain runtime
inputs even when disposable Cargo targets are removed.

Retire superseded Python applications with related imports, launcher routes,
documentation, package manifests and tests in one integrated change. Preserved
source-generated oracles, runtime-callpath evidence, copied-data readback/rollback,
process-death recovery and supported-platform/package acceptance gate closure.
Pre-retirement green CI is not final-tree acceptance.

## Consequences and rollback

There is one production backend owner and no silent Python fallback. End users
need no Rust toolchain; contributors still require a linker and explicit locked
source builds. Exact media compatibility keeps Python at runtime, and coin-domain
and development responsibilities must remain visible rather than being called
"media-only" or counted as retired applications.

Normal catalog schemas remain TEXT/INTEGER. Migration signatures deliberately
refuse SQL REAL values before writes; arbitrary extension-table and producer
Unicode-version compatibility is not certified. No supported normal-catalog case
requiring REAL was established; the limitation stays documented and fail-closed.

Keep a verified pre-retirement application revision in Git history and preserve
data/configuration backups. Rehearse that runtime's readback/restore on an independent
copy with its matching dependencies; do not overwrite unrelated later changes.
Retained test references verify native-produced rollback bundles but are not an
end-user runtime. Production rollback is a deliberate version/installation change,
not automatic delegation. Final cleanup preserves required runtime dependencies,
fixtures, recovery evidence, history and a verified current installation.

See [native backend](../native-backend.md) for commands, installation boundaries,
current evidence and remaining integrated acceptance requirements.
