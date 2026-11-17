# 0004: Rust-owned local implementation with unchanged external codecs

Date: 2026-10-12
Status: Accepted boundary; migration in progress

## Context

ADR 0003 deliberately retained local Python media/identity, coin-domain and
contributor/test tooling. That accepted backend scope was not a whole-repository
rewrite. The expanded requirement includes every locally authored executable,
algorithm, test, generator and tool, while keeping browser assets and the entire
Cloudflare Worker in their existing languages.

Media replacement must preserve durable content keys and transparent pixels.
Reimplementing third-party codecs would add unnecessary compatibility risk;
merely embedding our Python algorithms in Rust strings would not transfer
ownership. Test source volume also cannot be removed just to change language
statistics without retaining its assertions and independent expectations.

## Decision

Reuse the accepted Rust application core. Rust owns the remaining local policy,
algorithms, external API calls, process envelopes, coin workflows, tooling and
tests. Unchanged third-party Python libraries may remain external dependencies;
typed PyO3 calls are permitted, but owned Python dispatchers or algorithm strings
are not an accepted final implementation. FFmpeg remains an external process.

Keep HTML/CSS/JavaScript/TypeScript browser assets and the entire Cloudflare
Worker unchanged in language and behavior. Do not add an external dependency
merely because it was named as an example.

Port narrow public seams against independent fixtures generated from a pinned
source revision. Same-platform codec bytes must match their source oracle;
portable pixel/identity contracts must also match across platforms. Temporary
source-oracle support remains only until its native generation/drift replacement
and complete case mapping are verified. A limited hosted checkpoint is not final
migration or four-platform acceptance.

## Consequences and retirement

The existing installed application stays active while migration is incomplete.
No partial codec switch, silent old-language fallback, live Telegram mutation or
operator-data edit is authorized by this migration. Foreign codecs execute in
owned, bounded workers with verified reaping before scratch cleanup.

Remove old first-party runtime/test/reference/generator sources and obsolete
local code copies only after complete applicable parity, case coverage, package,
real copied-data and rollback gates pass. Preserve original implementations in
Git history, independent fixtures, configuration, operator data and diagnostics
for unresolved failures. Four ready-built supported platform packages remain
required; end users never compile Rust. Python may remain exclusively for
external libraries, so this decision is not a claim of zero Python dependency.
