# Upstream refresh for the Rust migration

Reviewed November 17, 2026; direct dependencies refreshed November 27, 2026;
compiler and direct-package versions rechecked November 28, 2026.
Versions below are pinned in the committed Cargo or
npm lockfiles; this review does not certify a build or test result.

| Component | Adopted version | Official source |
| --- | --- | --- |
| Rust | 1.100.1 | [Official patch release](https://github.com/rust-lang/rust/releases/tag/1.100.1) |
| rusqlite | 0.43.0 | [Release notes](https://github.com/rusqlite/rusqlite/releases/tag/v0.43.0) |
| serde_json | 1.0.154 | [Published crate](https://crates.io/crates/serde_json/1.0.154) |
| sha2 | 0.11.2 | [Published crate](https://crates.io/crates/sha2/0.11.2) |
| Tokio | 1.54.1 | [Published crate](https://crates.io/crates/tokio/1.54.1) |
| zip | 9.9.0 | [Release notes](https://github.com/zip-rs/zip2/releases/tag/v9.9.0) |
| libwebp-sys | 0.14.5 | [Published source revision](https://github.com/NoXF/libwebp-sys/commit/13c6aa7589a151f50d20bff705f698eb1f0a4ee5) |
| find-msvc-tools | 0.1.16 | [Published crate](https://crates.io/crates/find-msvc-tools/0.1.16) |
| ctor (test admission only) | 1.0.14 | [Published crate](https://crates.io/crates/ctor/1.0.14) |
| tempfile | 3.27.0 | [Published crate](https://crates.io/crates/tempfile/3.27.0) |
| Cloudflare Workers types | 5.20261128.1 | [npm package](https://www.npmjs.com/package/@cloudflare/workers-types) |
| Wrangler | 4.169.0 | [npm package](https://www.npmjs.com/package/wrangler) |
| TypeScript / Vitest | 7.0.2 / 5.1.2 | [Worker manifest](../worker/package.json) |
| Gitleaks | 8.30.1 | [Latest official release](https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1) |
| Playwright test | 1.65.1 | [Official release notes](https://github.com/microsoft/playwright/blob/v1.65.1/docs/src/release-notes-js.md) |

Rusqlite's upgrade includes lifetime safety fixes and rejects empty/comment-only
SQL. Zip's upgrade changes timestamp, extraction and metadata handling. The
storage, archive traversal, replacement, identity and offline installation
regressions therefore remain required checks. Worker dependency updates keep the
existing compatibility date and require type checking, Vitest and dry-run
packaging. They do not deploy the Worker.

The November 27 review queried official crates.io and npm metadata for all 39
distinct direct packages in the three Cargo manifests and two npm manifests.
Six had newer stable releases: the three Rust packages and Workers types,
Vitest and Wrangler listed above. Targeted Cargo updates change only those
three package versions; Worker lockfile changes also update their dependency
closure. This is not a claim that every transitive package is independently
on its latest major version. The `sharp` override and Worker compatibility
date are retained. Existing weekly Dependabot covers all five manifests and
GitHub Actions.

The early November 28 recheck found no newer stable version among those 39 direct
packages. Rust 1.100.1 was published at 2026-11-27 19:31:41 UTC and repairs an
Iterator implementation inference regression. The toolchain file, all actual
workflow compiler selections and current contributor documentation now use
1.100.1. Historical audit records and illustrative test fixtures retain their
original values. Four-platform native tests, linting and packaging remain
required; a compiler version update alone is not acceptance evidence.

The next November 28 recheck at 11:24:46 UTC examined those 39 direct packages
and the existing `sharp` override. Three newer stable releases were available:
zip 9.9.0, libwebp-sys 0.14.5 and Workers types 5.20261128.1. Targeted lockfile
updates adopt those versions without changing other package versions, native
feature selections or the Worker's compatibility date. Rust 1.100.1 and the six
used GitHub Action release lines remain current. Weekly Dependabot already
covers every manifest and Actions; no duplicate updater is needed.

Zip 9.9.0 adds optional `jiff` 1.x support; that feature is not enabled here.
Its Rust 1.89 minimum is below the pinned compiler, and the retained compression
feature is `deflate-flate2-zlib-rs`. Libwebp-sys 0.14.5 adds Emscripten SIMD build
flags and frees resources in its own unit tests, as shown in the
[complete published-source comparison](https://github.com/NoXF/libwebp-sys/compare/11b096bd65f0d51326673ae3e2e5af2dae02368f...13c6aa7589a151f50d20bff705f698eb1f0a4ee5).
This project retains `default-features = false` and `std`; it does not opt into
new CPU instructions or change the vendored codec revision. Archive traversal,
package installation, codec pixels, signatures and all four native platforms
remain acceptance gates. The published old/new zip license and libwebp native
copyright/patent texts compare byte-for-byte equal; the source comparison also
leaves the binding license unchanged. Third-party notice version references
are refreshed while their complete texts are retained.

Zip 9.8.2 repairs archive end-record validation and duplicate ZIP64 metadata.
SHA-2 0.11.2 repairs backend selection and updates internal implementations;
see its [upstream changelog](https://github.com/RustCrypto/hashes/blob/master/sha2/CHANGELOG.md).
Archive/package, hash/inventory and four-platform build tests therefore remain
mandatory. The refreshed crates' upstream license files were byte-identical
to the previous versions, so notices retain the existing full texts.

Axum 0.8.9, the current direct release, requires exactly `matchit =0.8.4`.
Matchit's newer releases cannot replace that transitive pin without changing
Axum's supported graph. The lockfile retains the upstream requirement.

The Gitleaks release API confirms 8.30.1 remains the latest release. The Linux x64
archive's official SHA256 is
`551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb`, matching
the Rust security tool's pin. Exact introduction identities are individually
reviewed; metadata-only reporting remains unchanged.

Playwright 1.65 adds inert-element actionability checks and changes some cookie,
accessible-name and request-parameter behavior. Its official supported Node
versions include the CI's Node24. Browser tests retain their original cases,
budgets and expectations; they must run against the upgraded Chromium rather
than treating an installed package or successful discovery as acceptance.

## Lottie renderer

The bundled engine remains
[`47b71a7e9072ac67b2e18c408d3668fed278fb93`](https://github.com/laggykiller/rlottie/tree/47b71a7e9072ac67b2e18c408d3668fed278fb93),
the current `arm_compat` head and the engine pinned by rlottie-python 1.3.8.
The repository's default `master` is 15 commits behind this engine. Its default
branch would therefore downgrade the renderer.

Ready-made `rlottie` 0.5.4 / `rlottie-sys` 0.2.12 bind different engine revisions
and introduce system probing, network clones and bindgen requirements. Velato
0.12.0 is a Rust alternative, but its documented unsupported Lottie features
include text, time remapping and several shape/effect features. Neither is a
verified replacement for the existing pixel and animation contracts. The ready
native dependency is retained under the project's native-dependency policy.
Its provenance, build changes and redistribution licenses are in
[NUMERA-SOURCE.md](../runtime/vendor/rlottie/NUMERA-SOURCE.md).

## Logo sources

The credited asset repositories were reviewed at:

- [spothq/cryptocurrency-icons](https://github.com/spothq/cryptocurrency-icons/tree/1a63530be6e374711a8554f31b17e4cb92c25fa5):
  release 0.18.1, revision `1a63530be6e374711a8554f31b17e4cb92c25fa5`.
- [Cryptofonts/cryptoicons](https://github.com/Cryptofonts/cryptoicons/tree/f7c78904c6dbad90722e53925f57bbca88419d3f):
  November 4 revision `f7c78904c6dbad90722e53925f57bbca88419d3f`, latest
  `@cryptofonts/core` release 1.0.3.

This repository tracks the logo-processing tool and curated keyword input;
downloaded logos and the operator's published inventory are local data. Neither
asset repository is a vendored application-code dependency. Updating them means
an explicit operator asset refresh with its provenance, rather than silently
replacing local logos during a code migration. CoinGecko, CoinPaprika and
CoinMarketCap remain runtime providers through the shared bounded Rust transport.
