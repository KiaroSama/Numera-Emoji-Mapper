**First, comply with the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit chain. Preserve native test admission, independent fixtures and exact-source CI.**

# Post-release media, Windows metadata and CI recheck

## Assessment and scope

Reviewed main and the authenticated `v3.1.0` release:
`eb8c2e8cf9e296532464a24cff36939cb4070c50`.
Tracking Issue: [#106](https://github.com/KiaroSama/Numera-Emoji-Mapper/issues/106).
The earlier repairs in #105 and the launcher repair in #103 were already merged.
This follow-up contains reproduced application boundary findings and a bounded
review of live test-file publication; it does not reopen those completed repairs.

The owner requested a complete recheck and implementation PRs, with no merge
or closure by the audit agent. One repair branch is necessary for that PR.
This record connects assessment, requirements, technical plan, additive tests,
implementation and verification. No tracked owner Rules directory or `.specify`
workspace was found; it does not claim that an unavailable external Spec Kit
installation was executed. The integration agent must use the owner's actual
Rules and prescribed chain before final disposition.

## R11: converter control files become invalid media inputs

### Reproduction and cause

The ordinary released CLI converted two valid synthetic images with
`make-emoji-pngs --in SOURCE --out OUTPUT`, exiting 0. The output contained two
valid PNGs plus `.convert.lock`. The OS lease was independently verified as
released, while its persistent file correctly remained.

`add-media --in OUTPUT --as static` then exited 3 with:

```text
Done. new=2 dedup=0 failed=1
```

Its decoder warning named `.convert.lock`. Both image rows and media files were
correctly committed; the metadata caused a false partial-failure result.
The selection in `runtime/src/local_cli.rs` admitted every immediate regular
file, so `.svg_cur` and `.svg_skip.txt` could cause the same failure after an
interrupted/quarantined conversion. A lock-only input folder also attempted
decoding, returned 4, and created an empty catalog instead of using the existing
early no-input refusal, exit 2.

The retained automated ordinary-CLI regression ran before the production edit:
four commands, 34 assertions, five expected contract failures. It independently
verified image pixels, two durable catalog rows, no orphan media, unchanged
lock inode/bytes, released OS ownership and settled codec scratch. The driver
SHA256 is `f1e13f2f5ff83e4266a0a819475788e470d6c3b3bc5ee1a6aa1edeb243b0af73`.

### Required behavior and implementation

Exclude exactly `.convert.lock`, `.svg_cur` and `.svg_skip.txt` from automatic
`--in` discovery, retaining the existing `is_file`, flat traversal and sorting.
Append explicit positional paths afterward without that filter. Emit a DEBUG
discovery event without reading or logging metadata contents. Leave all three
files and OS ownership untouched.

An extension allowlist would break established content detection for unknown
and extensionless inputs. A general dotfile, `.lock`, `.json` or `.txt` filter
would suppress legitimate artwork or conceal actual bad inputs. Decoding
errors must still count toward exit 3/4. Do not delete the converter lease,
change its lifetime, or interpret its diagnostic PID/age as current ownership.

### Additive acceptance

The guarded `supplemental_local_inputs` target exercises actual CLI processes:

- Real conversion followed by directory import and repeat deduplication.
- All three controls remain byte-for-byte unchanged; a held lease remains
  owned, rejects a second acquisition and becomes acquirable after release.
- A control-only folder refuses before creating catalog storage.
- Explicit controls still fail alone and alongside directory inputs.
- Valid hidden, extensionless, unknown-extension and near-matching filenames
  remain selectable; directory order precedes explicit argument order.
- Discovery remains flat, repeated paths still deduplicate, and Unix file-link
  canonicalization retains its existing behavior.
- Unrelated invalid hidden/text/lock files still report failures while valid
  progress remains committed in a reopened catalog.

## R12: advertised APNG sources are silently omitted

### Reproduction and cause

The current CLI guide advertises `.apng` in general `make-emoji-pngs` input.
`convert_cli::PRIORITY` included only seven other extensions, so the directory
selector never admitted an APNG source. A genuine two-frame APNG-only folder
returned exit 0, `made 0`, `failed=0`, and no output PNG.

The 691-byte synthetic fixture has valid chunk CRCs, `acTL=2`, two `fcTL`
chunks and an `fdAT` chunk; independent FFprobe reported two APNG frames.
Fixture SHA256:
`8507f1d0aabb10f569c0fa26d74a463221a511b2700253e03e70944c7877f028`.

Two controls isolate the discovery boundary: explicitly passing the same APNG
to `add-media --as static` succeeds, and the identical bytes under a `.png`
name are discovered and converted by `make-emoji-pngs`. Both outputs' 40,000
decoded RGBA bytes exactly match the independently specified first/default
frame. No new decoder or animation feature is required.

### Required behavior and implementation

Append `apng` to the general-mode priority list after the existing seven
extensions. Preserve every existing relative priority and legacy coin-mode
SVG/PNG selection. Existing lowercase normalization admits mixed-case APNG
extensions. This command produces a static PNG from the existing first/default
frame decoder; it does not preserve animation. `add-media --as auto` retains
its existing APNG-to-video classification.

Use the genuine fixture in additive real-CLI regressions with an independent
pixel oracle. Verify lowercase/uppercase discovery, idempotence and preservation
of a previously preferred source when it shares the APNG's stem. Do not replace
original codec fixtures or derive expected pixels from the implementation.

## R13: active guides describe obsolete age-based lease reclamation

`docs/guide-reference.md` and `docs/guide-workflows.md` still told operators to
wait 120 seconds after a dead publisher. The native `Lease` uses OS locking and
retains its file; file age and recorded PID are diagnostic, not ownership proof.
Correct the active guides to describe OS ownership and surviving owned handles,
and instruct operators to leave lease files in place. Historical changelogs
retain their original context. No runtime lock behavior is changed.

The troubleshooting row also distinguishes contention from other filesystem
or I/O failures using the detail after `OS lock acquisition failed:`. Waiting
for an owner applies only to a held lease; other reported failures need their
underlying condition resolved. Neither branch permits removing the lease file.

## R14: Windows numeric versions remain hard-coded to 3.0.0.0

The authenticated Windows x64 and ARM64 `v3.1.0` release executables both
contain numeric `FILEVERSION` and `PRODUCTVERSION` values of `3.0.0.0`, while
their separate textual `FileVersion` and `ProductVersion` values are `3.1.0`.
A bounded independent PE resource parser inspected the actual `RT_VERSION`
resources, fixed-information signature, numeric words and string tables.
No Windows executable was run locally. Both files' hashes match their package
manifests. This causes numeric version consumers to identify the newer release
as 3.0.0.0; it is not evidence of an installer failure or a tested Explorer UI.

`runtime/build.rs` supplied `0x0003_0000_0000_0000` overrides after constructing
`WindowsResource`. Remove both overrides and retain the resource library's
Cargo-derived defaults for numeric and textual versions. Do not replace them
with another manually maintained constant or overwrite an existing release tag.

The shared `windows-binary-versions` action invokes the native Rust command
`verify-binary-version --executable FILE` for all four actual built executables.
Its bounded PE reader inspects every `RT_VERSION` language payload and string
table independently of the resource generator. Both numeric tuples must equal
compiled Cargo major/minor/patch/0; both strings must equal the full Cargo
version. The expected version is never obtained from the inspected file.
Missing files/resources, malformed version blocks and mismatched values fail.
Both Windows architectures run this check in CI and prebuilt-package jobs.
The command uses the existing four-level timestamped Rust logger; the action
contains only command orchestration. Existing icon, architecture and signing
checks remain. Its optional single-executable mode is exercised in PR Windows
CI. The signing workflow uses the known application in `bin` as the verifier
and verifies that actual `bin/numera-emoji.exe` again after signing and package
creation, before signed-artifact upload. Signing itself remains restricted to
the existing main/tag conditions; no secret-handling or signing flow is changed.

### CI policy failure found and repaired during review

Candidate `08a5fea5db9c381bcc9e1b86dabfb71fc2e6b971` failed the existing
language-policy gate because the first version of this action embedded a
PowerShell verification program. The failure occurred before focused and full
native regression execution, so those jobs were not counted as passes. The
same ordinary inventory command reproduced the single violation locally.
Moving verification into native Rust and retaining only the CI command loop
removed that violation without changing the policy, allowlists or test guard.
The final published head must pass that same gate and the complete downstream
tests. Eleven focused version tests cover both architectures, multiple resource
identities/languages and string tables, every numeric component, independent
text mismatches, advertised translation lookup, absent/duplicate metadata,
malformed UTF-16/types/lengths,
truncation/range bounds, literal paths and CLI refusal. A separate library case
checks expected-version parsing and Windows component bounds. The focused
target runs in the early native boundary step; a syntactically valid workflow
alone is not acceptance.

The rebuilt ordinary Linux CLI passed a frozen 39-case version contract against
both authenticated released Windows executables and controlled byte copies.
The genuine stale numeric versions were rejected; changing only the two numeric
tuples to literal `3.1.0.0` made the positive controls pass. Independent File/
Product numeric/text changes, absent keys/resources, invalid signatures, lengths,
UTF-16, truncated/empty files and CLI argument errors were rejected. Every
original archive and test input retained its pre-execution SHA256. No Windows
code or guarded test binary was executed by this probe. Driver SHA256:
`7b3d1eec45a46b428515ba546aa0bc15c5357a7ace9d94fc9a235cd8098d21ae`.
Independent review also reproduced a dangling `Translation` pair accepted by
an intermediate native verifier: the sole table was `000004b0`, but the resource
advertised `041104b0`. The final verifier requires each advertised pair to resolve
to a validated table, without requiring `VarFileInfo`, equality with the PE
resource language, or advertisement of every valid table. The unchanged four-
case real-byte regression first had two intended-negative failures and then
passed all four cases after correction. Both correct originals remained accepted;
both dangling declarations were refused with the specific missing-table error.
The frozen 39-case regression also passed again on this final native binary.
Translation driver SHA256:
`5b42e7a2184eb32c260b478db647fc7c168330eeef1d7f0b7960e455e0a40cc5`.
Seven additional ordinary-command checks preserved the existing architecture/
approved-logo behavior on both original and corrected Windows files, rejected
the opposite architecture, and accepted the rebuilt Linux executable.

Formatting, all-target Clippy, release builds of all four binaries and compilation
of the two focused test targets passed locally. The unchanged identity inventory
reported 1,516 originals, 1,893 wired Rust functions and zero errors; the language
inventory reported 1,129 candidate files and zero violations. Compiled test
targets still require protected execution on the final hosted revision.

### Full-suite documentation correction

The complete protected Linux suites on candidate `d13dfa0` exposed a shared
documentation failure after the new media and version regressions passed.
Both `baseline_tooling` and `supplemental_tooling` correctly rejected the
converter-only `--out` flag inside the `add-media` reference section §12.4.
Its two-command example was semantically valid as a workflow, but mixed two
different command interfaces in a section whose contract describes one.
The guide now keeps the conversion example in §12.1, refers readers there,
and shows only the actual `add-media` command in §12.4. Neither assertion,
flag parser nor allowlist changed. Both existing tooling targets also run in
the early four-platform boundary step, followed by the unchanged complete
suite, so a reference mismatch is visible before full-suite compilation.
The failed candidate is retained as diagnostic evidence, not acceptance;
the corrected published revision requires its own full hosted gates.

## R15: recovery-journal observation race

The completed Windows ARM64 push suite on `d13dfa0` also exposed a separate
test-harness defect. The live-holder exclusion case failed at
`baseline_recovery_support/locks_tests.rs:168` with
`EOF while parsing an object`, line 1, column 15. Each child appends a JSON
value and newline with `writeln!`, which can perform multiple writes. The
parent's `read_to_string(...).lines()` also returned an unfinished final
record and parsed it as complete. This is an observation-framing failure;
the parse error does not establish that the OS lease admitted two owners.
The corresponding PR run passed, but that does not resolve the observed race.
Uploaded diagnostics retained the error and fixture path, not the journal
bytes, so neither the exact interrupted byte sequence nor the optimized
call site is claimed to have been recovered.

The repaired observer treats newline-terminated records as committed while
child handles remain owned. Malformed committed records still fail. After
all children exit and their handles are drained, `wait()` validates every
settled journal strictly, including callers that do not request spans again;
truncated final records cannot disappear into a passing result. The real
lease operations, separate child journals, original identities, independent
PID/timestamp assertions, overlap canary, hold durations and 20/60-second
budgets remain unchanged. Deterministic framing regressions cover partial
publication, completion, malformed committed data and strict settlement.
The existing recovery target now also runs in the early guarded boundary
step, and failed synthetic journal files are included in CI diagnostics.
Final protected execution must establish this repair on all four platforms.
The two new framing tests compiled in the locked release target, scoped
warnings-as-errors Clippy and formatting passed, and the refreshed inventory
retained all 1,516 original identities with 1,895 wired Rust functions and
zero errors. The unchanged language policy still reports zero violations.

## R16: readiness files become visible before their PID contents

A bounded adjacency review of the R15 publication/observation boundary found
three equivalent one-shot readiness windows. These are source-confirmed
scheduling windows, not newly reproduced CI assertion failures:

- `baseline_lifecycle.rs`: the detached grandchild writes `descendant-ready`
  directly; its observer parses a PID immediately after file existence.
- `original_main_rust_support/worker.rs`: leaf, tree and codec fixtures publish
  PID files directly. Process-lifecycle readers parse after existence, while
  preview/gallery cancellation can terminate the writer before its write ends.
- `coin_workflows_support/mod.rs`: the child publishes two PIDs directly; its
  observer reads and requires exactly two entries as soon as the file exists.

`std::fs::write` creates the file before writing its bytes. A reader scheduled
between those operations can therefore see an empty marker; no partial small
OS write needs to be assumed. The repair writes each complete marker to a
distinct sibling staging file, lets the write return and close its handle,
then renames it to the readiness path. The worker appends the staging suffix
to the whole filename so `ready.txt` and `ready.leaf` cannot share a staging
name. Each fixture already has a unique scratch directory and one publisher.
This follows the existing construction-marker pattern and does not claim
crash durability or change production persistence.

The published PID bytes, observers, original test identities, process ownership,
independent handle/PID assertions, cancellation order, deadlines, polling and
hold durations stay unchanged. Synchronous config-observer responses are not
readiness markers and remain unchanged. No extra retry or larger budget masks
the defect. Existing lifecycle, preview, gallery and coin-workflow cases cover
the repaired publication through their real child processes in the full
four-platform suite. Local compilation and review are not substituted for that
protected hosted execution.

The exact three-file repair passed independent source review, formatting,
all-target warnings-as-errors Clippy, standalone worker compilation, and
locked release `--no-run` compilation of the library and four lifecycle/
original-main targets. No protected body or compiled worker was executed
locally. No test declaration, expected result or platform gate changed.
Fresh inventory still reports 1,516 original identities, 1,895 wired Rust
functions and zero errors; all 1,129 language-policy candidates pass unchanged.

The same bounded inspection found no additional live JSONL stream requiring
R15's framing fix: worker-admin metadata is read after owned process completion;
media readers require complete framing; construction and converter handshakes
already require completed publication or exact expected content. These findings
close this specific fault-class inspection without creating a new broad audit.

## Plan, verification and integration gates

1. Preserve the ordinary-CLI red receipts and independent fixture identity.
2. Add guarded actual-process regressions, preserving all original inventories.
3. Apply the narrow directory filter, append the missing APNG priority, and
   remove the stale numeric Windows version overrides.
4. Correct the active input/lease documentation and describe the supported
   converter-to-catalog composition explicitly.
5. Rebuild the ordinary executable and run unchanged baseline/fixed probes.
6. Compile/lint and execute the eight media cases and eleven version cases in the
   existing early four-platform boundary step, followed by full unfiltered
   native acceptance, including the expected-version parser case. Verify actual Windows numeric/string version metadata in both
   native and package jobs and inspect final Windows package bytes.
   Include both existing CLI-reference targets and the recovery target with
   its two deterministic framing cases in early and complete acceptance.
   Preserve complete readiness-marker publication and execute the existing
   detached-descendant, process-lifecycle, preview, gallery and coin-workflow
   ownership cases on their applicable platforms in complete acceptance.
7. Require browser, Worker, inventory, package and security checks on the exact
   published repair head. Record applicable conditional skips honestly.
8. Open the complete implementation PR linked to #106. The authorized owner
   integration agent reviews it and completes all PR dispositions under Rules;
   the audit agent does not merge or close it.

The local native runner's private-interface enumeration was refused by this
session's environment before any test body; the actual Chromium executable also
received `socket() ... Operation not permitted` at startup. These refusals were
recorded, not bypassed or counted as passes. Ordinary application-command
regressions remain separate from protected test execution. Hosted results must
identify the repair head; earlier main's successful run is not repair acceptance.

The unchanged ordinary-CLI directory regression passed after the media fixes:
33/33 assertions, with the correct no-input path avoiding the extra catalog
inspection that existed only on the failing baseline. The APNG controls passed
lowercase/uppercase discovery, exact pixels, PNG precedence, invalid-PNG
fallback and unchanged output bytes/mtime on repetition. These local product
commands do not claim protected test execution or Windows acceptance.

Preserve the existing weekly Dependabot ecosystem coverage, dynamic main/push
CI badge, and tag-backed release badge. Human published author/committer emails
must use `Kiaro.Sama.Dev@gmail.com`; genuine bot identities retain the owner's
exception. Any newly discovered relevant failure must be resolved in this PR
with its original postcondition, rather than deferred to another repair cycle.

## References

- [Current CLI guide](../guide-cli.md), [workflow guide](../guide-workflows.md),
  [troubleshooting](../guide-reference.md), and [test requirements](../../tests/README.md).
- [Independent previous repair record](2026-11-24-deep-review.md).
- [Rust File locking API](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
  and [Windows LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex).
- [Rust line termination](https://doc.rust-lang.org/std/primitive.str.html#method.lines)
  and [formatted writer semantics](https://doc.rust-lang.org/std/io/trait.Write.html#method.write_fmt):
  a live append is not a complete record until its framing delimiter is present.
- [Rust file writes](https://doc.rust-lang.org/std/fs/fn.write.html) and
  [same-filesystem rename](https://doc.rust-lang.org/std/fs/fn.rename.html):
  publish one-shot readiness files only after their contents are complete.
- [PNG third edition: animation](https://www.w3.org/TR/png-3/#11acTL) and
  [image crate PNG decoder](https://docs.rs/image/0.25.10/image/codecs/png/struct.PngDecoder.html).
- [Windows fixed version fields](https://learn.microsoft.com/en-us/windows/win32/api/verrsrc/ns-verrsrc-vs_fixedfileinfo),
  [VS_VERSIONINFO layout](https://learn.microsoft.com/en-us/windows/win32/menurc/vs-versioninfo)
  [StringTable layout](https://learn.microsoft.com/en-us/windows/win32/menurc/stringtable),
  and [translation-based lookup](https://learn.microsoft.com/en-us/windows/win32/api/winver/nf-winver-verqueryvaluew):
  independent numeric and string artifact verification.
- [winresource constructor defaults](https://docs.rs/winresource/latest/src/winresource/lib.rs.html):
  the reviewed 0.1.34 version derives numeric and textual resources from Cargo.
- [GitHub Spec Kit](https://github.com/github/spec-kit) and
  [required current-commit status checks](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).
