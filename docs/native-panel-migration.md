# Native panel migration: evidence boundaries

The migration branch has additive Rust panel tests and historical-source replay.
They are not acceptance of a new installed runtime. The installed package and
production media selector remain unchanged; no live pack is used for validation.
The browser assets and the entire Cloudflare Worker remain unchanged.

## What is authored

| Surface | Native counterpart | Evidence it is designed to obtain |
|---|---|---|
| Cards, branding and membership | `native/tests/reconcile_view_parity.rs`, `support/panel_view_cases.rs` | Real catalog, served page/assets, configured logo availability, hidden published items, actual recorded pack names, copy labels and stable similarity ordering. |
| Scoped saves and durable intent | `support/panel_persistence_cases.rs`, `support/panel_stale_cases.rs` | Actual HTTP status, persisted selection/targets/moves/holds, invalid saved-plan preservation, current membership and refusal of stale identities/visibility without partial writes. |
| Concurrent order, save and page | `support/panel_concurrency_cases.rs` in that same server | Requests acknowledged while the real view mutex is held must preserve the committed order, exclusions, nonempty page and media reads after release. This is native serialization, not CPython's emptied-list implementation. |
| Video identity migration | `support/panel_migration_cases.rs` | Real native codec, coordinated rekey, page reload, verified no-op repeat and rollback preserving the original plan, media bytes and publication ID. |
| Replacement and ownership failures | `native/src/panel_save_tests.rs`, Windows persistence variant | Real replacement refusal and exact retry; maintenance remains excluded through plan replacement. The Windows HTTP case and direct-save unit case are separate observations. |
| Plan policy | `native/tests/intent_parity.rs` | Explicit moves versus no opinion, holds, logo capacity, real plan roundtrip and pruning decisions only when the current catalog proves a key is gone. |
| Served-text contracts | `support/panel_asset_cases.rs` | Original actions and grid assertions over unchanged version-pinned scripts and HTML. These are text checks, not rendered gesture or performance evidence. |
| UI telemetry | `support/panel_logging_cases.rs` in the same persistence server | Actual 204/400 responses, closed-schema batch validation, no private-text logging, INFO/WARNING fields and all seven script sources in the real UTF-8 run log. |
| Request and fresh-read boundaries | `support/panel_request_cases.rs` in that same server | Token/origin/host/schema/permutation refusals without catalog mutation, framing headers, inert item JSON, external keyword refresh, hidden exclusion preservation and real unavailable-catalog responses. |
| Persistent connections | `support/panel_connection_cases.rs` in that same server | Real accepted-socket counts, two pings and a 204-followed-by-ping on one socket; unread oversized/missing-length bodies require close rather than reuse. |
| Startup and second launch | `native/tests/panel_branding.rs`, existing `tooling_sandbox_serve.rs` cohort | Ping/session while a bot lookup is acknowledged but unanswered; a finite second copied command reuses the actual owned first panel without changing its catalog. Shutdown must settle threads/processes and close the port. |
| Preview and warm-up | `native/tests/preview_warm.rs`, existing native lifecycle cohort | Actual same-binary codec previews, decoded dimensions/motion/alpha, original media recipes, unchanged cache bytes/timestamps, bad-file continuation, static holding trays, rate caps and process-owned cancellation. |

These tests are **uncompiled and unrun on the current migration tree**. Source
inspection, formatting, hashes and candidate mappings do not certify behavior.

## Independent historical evidence

The existing pinned Git corpus and bounded source worker are reused. The panel
selector admits exact original source identities, selects unchanged assertions
through the existing AST mechanism and uses original catalog, identity, plan and
atomic-write functions. It does not use native results to generate expectations.
Its current authored cohort is 23 view, five static-asset, 12 plan, 59 served-text
and two telemetry methods, plus three inert-JSON, two loopback, four port-diagnostic,
one preview-tier, eight scoped-save HTTP, four persistent-connection, 17 intent
and two event-pinned reorder methods: 142 methods and 50 delayed temporary contexts across 34 admitted sources. Those counts are
asserted by the harness, not observed execution results. Both HTTP cohorts use the
original handler and SQLite; each teardown is followed by bounded thread settlement
and closed-socket checks. The intent cohort also uses original writer/maintenance
OS locks and logical database signatures. Its original video-migration method is
explicitly excluded until the complete historical migration dependencies are wired;
the native codec/migration test is separate, not its expected-answer oracle.

The source worker retains its 512 KiB envelope ceiling, 15-second wall ceiling and
eight-second idle ceiling. Source and native fixture durations remain unmeasured.
Cleanup requires successful assertions, acknowledged teardown, known physical
workspace members and settled ownership; failure retains diagnostic evidence.

Adaptations are explicit: private operator dictionaries, a no-op historical native
prerequisite and catalog lease ports outside the intent cohort, fixed synthetic timestamps, shared native test
setup, and a pinned short video instead of the original freshly encoded red clip.
The native ownership and actual HTTP tests supply separate evidence; historical
stubbed ownership is never counted as lock validation. Native telemetry writes JSON
fields instead of the original sorted `key=value` text. Raw HTTP `NaN` rejection is
parser evidence, not the original Python object's nonfinite-number validation.
The unavailable-catalog page uses a diagnostic string instead of the original
last-refresh clock text; the native mutex replaces Python's shared-object update
mechanism. Neither is claimed as exact original private-object proof. The native
concurrency test acknowledges ingress while holding the actual view mutex; the
separate unchanged historical methods pin CPython's sort window with their original
events and watched lock. These are distinct observations, not interchangeable proof.
Preview rendering is exercised at the generic native cache/codec boundary; original
HTTP-render class replay remains separate. Rust future cancellation before the first
poll and during an owned codec call adapt the original Python stop-event contract.

## Refusal contract under test

A malformed or unsupported saved plan must answer **409**, preserving its bytes
and catalog selection. The migration branch now classifies save-time plan-read
failures that way; page reads already used 409. A real durable-write failure remains
an error, not an `ok` acknowledgement, and maps to **503** at the HTTP boundary.
The same scoped request can be retried after the filesystem obstruction is removed.
This authored repair still requires final runtime verification.

## Existing-session compatibility under test

Finite copied-native `panel --no-open` commands share the existing test listener.
They must reuse a matching session without rebuilding its catalog or codec, refuse
another catalog/view, and recognize a legacy panel only after a 404 session lookup,
an exact ping answer and all four original page markers. A malformed current
session never falls back to legacy recognition. Replies are read with a 4 MiB plus
one-byte bound instead of buffering an unbounded body before checking its size.
These source-supported compatibility repairs are uncompiled and unrun; suppressed
browser opening is not evidence for the original mocked browser-call assertion.
On Windows, a failed bind also requests a bounded numeric `netstat` diagnostic and
reports the listener PID when known. That observation never authorizes stopping a
process. Other platforms leave the PID unknown; missing tools and uncertain results
must not be invented as a holder.

## Recovery checks authored alongside the panel

The existing native SQLite snapshot suite now includes real exclusive source and
destination locks, a short explicit backup budget, timeout restoration, preserved
rows, successful retry, two WAL-aware snapshots and refusal before writing for a
zero budget. The normal backup default remains 30 seconds. A separate four-source
historical cohort retains all four original snapshot methods, including invalid
Python float budgets; Rust's typed duration is a different boundary.

Curation migration tests use real SQL, files, leases, journals and rollback, with
only the decoder controlled. Authored assertions cover structured-key rewrites
without changing labels, publisher-lock and destination-collision refusal,
no-op repeat, later SQL/media/plan changes, repeated restore, identical preexisting
destinations and distinct recovery files. Malformed plan records use checked
access rather than panicking during mutable JSON indexing.

The existing process-death cohort uses independently pinned VP9 fixtures and a
native decoder inside its bounded child session, not a first-party Python
generator. Fourteen one-file variants cover durable stages, before/after SQL
commit, individual publisher/frozen-plan replacements and journal retirement.
A three-file variant stops after the second actual transfer but before its SQL
path update, then replays and restores every identifier, state and media file.
Test-only barriers compile out of ordinary binaries and extensions. A separate
controlled snapshot-verification error tests refusal, not corrupt-page detection.

The cohort also inspects halfway SQL/state, refuses an altered directory binding,
retains recovery evidence after missing media or decoder loss, and normalizes
legacy absolute paths. Two state-stage variants seed a moved file or hard link
after actual child death, then exercise restore or forward replay. These are
recovery states, not proof of death inside a file-transfer syscall. Held children
exclude direct and canonical-alias catalog/maintenance ownership attempts. The
reverse direction holds actual publisher, catalog, archive, ingest and HTTP-save
entrypoints before migration refuses; child termination differs from the original
stdin-release protocol.

Native provenance assertions distinguish shared CID/FUID evidence from paths,
archive slots, conflicting or duplicate identifiers and undecodable media. An
already-current snapshot cannot repair a required old state key; the trusted
historical snapshot can supply that mapping. Separate copied-command scenarios
cover clean, pending, incomplete, collision, obsolete and interrupted reports,
including the recorded stage/start diagnostic. Those commands still use the
retained codec closure, so they do not establish Python-free packaging.

A separate twenty-source historical selector now admits all 33 original
identity and nine provenance methods, plus 17 original recovery/ownership/resume
methods, including their inherited setup and teardown. It reuses the pinned Git
corpus, AST selector, bounded worker and delayed temporary-directory ledger.
Original SQL, locks, snapshot, bundle, ingest, reconciliation and state logic remain
intact. Its original Catalog uses the original writer with a private native-
prerequisite port. Each original fixture supplies its own fake decoder; unused
content/download paths refuse. Full catalog order, inclusion, CID/FUID and known-ID
resume are checked before and after restore. Exact source hashes, 59 methods,
64 cleanup acknowledgements, known nested fixture directories and restored stdout,
thread and function inventories are required before success-only cleanup. These
are harness requirements, not observed results.

Three original lifecycle methods remain explicitly excluded: real child writer
release, process death after the second move and pending-journal writer/HTTP
behavior. Their native counterparts are separate evidence, not execution of those
original methods. All these checks are authored, uncompiled and unrun. Timing,
custody, installed-package and real-data acceptance remain required; candidate
mappings are not passing tests.

## Acceptance still required

- Compile and run the integrated hosted native and historical cohorts, including
  Windows replacement/lease behavior and the original scoped-save/persistence replay.
- Run the real-browser suite against the native service. Text assertions do not
  establish dragging, selection, animation, reduced motion, layout or performance.
- Complete native test isolation, platform custody, package installation, approved
  embedded identity and genuine signing, copied-data parity and rollback gates.
- Reconcile every original assertion and its fixture provenance before any source
  retirement or production media-selector change.

See [the panel workflow](guide-panel.md), [native ownership](native-backend.md),
[native test evidence boundaries](native-test-migration.md) and
[the first-party migration decision](adr/0004-first-party-native.md).
