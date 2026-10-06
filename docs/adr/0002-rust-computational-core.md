# 0002: Required native computation with unchanged data identity

Date: 2026-10-07
Status: Accepted

## Context
The exact greedy look-alike walk is quadratic Python work. Approximate indexes
would change the first-minimum chain and therefore the planned pack order.
Perceptual-hash candidate nomination is another batch of the same integer work;
it must never replace actual content verification. Rewriting media decoding or
resizing risks changing the catalog's durable content identities.

## Decision
Use one required Rust/PyO3 extension for exact similarity indices and batch
Hamming candidate indices. Python retains grouping, original object mapping,
actual file comparison, catalog ownership/persistence and Telegram operations.
No new dedup store, decoder, service, worker pool or production Python fallback.
Rust executes owned vectors without the Python GIL and returns indices only.
Missing or incompatible native installation fails before catalog state is opened.

Build with maturin release mode and the committed Cargo lockfile. The supported
standard-GIL Python 3.11/3.12/3.14 matrix and Windows/Linux builds verify the
extension. Source-generated test-only fixtures establish exact parity; read-only
production-shaped replay and repeated benchmark measurements gate adoption.

## Consequences
Source installation now needs Rust and a platform linker; the launcher and doctor
must name this prerequisite and cannot continue with a missing extension.
The operator chose mandatory native computation over optional acceleration.
The old runtime remains deployable from pre-port Git history for rollback.
No persisted schema, normalized pixel hash, published id or saved intent changes,
so rollback requires application restoration, not a data migration.
