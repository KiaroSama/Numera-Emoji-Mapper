**First, comply with the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit chain. Preserve native test admission, independent fixtures and exact-source CI.**

# Post-release media and Windows version recheck

## Assessment and scope

Reviewed main and the authenticated `v3.1.0` release:
`eb8c2e8cf9e296532464a24cff36939cb4070c50`.
Tracking Issue: [#106](https://github.com/KiaroSama/Numera-Emoji-Mapper/issues/106).
The earlier repairs in #105 and the launcher repair in #103 were already merged.
This follow-up contains new, reproduced producer/consumer boundary findings;
it does not reopen those completed repairs.

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

The shared `windows-binary-versions` action inspects all four actual built
executables through Windows `FileVersionInfo`, an oracle independent of the
resource generator. Both numeric tuples must equal Cargo major/minor/patch/0;
both strings must equal the full Cargo version. Both Windows architectures run
this check in CI and prebuilt-package jobs before their existing acceptance.
The action includes timestamped INFO/WARNING/ERROR/DEBUG logging and rejects
missing/malformed metadata. Existing icon, architecture and signing checks remain.
Its optional single-executable mode is exercised in PR Windows CI and verifies
the actual `bin/numera-emoji.exe` again after signing and package creation,
before uploading the signed artifact. This keeps the signed distributable
under the same version contract without changing the secret-handling or signing
flow. Signing itself remains restricted to the existing main/tag conditions.

## Plan, verification and integration gates

1. Preserve the ordinary-CLI red receipts and independent fixture identity.
2. Add guarded actual-process regressions, preserving all original inventories.
3. Apply the narrow directory filter, append the missing APNG priority, and
   remove the stale numeric Windows version overrides.
4. Correct the active input/lease documentation and describe the supported
   converter-to-catalog composition explicitly.
5. Rebuild the ordinary executable and run unchanged baseline/fixed probes.
6. Compile/lint the eight-case additive target and execute it in the existing
   early four-platform boundary step, followed by full unfiltered native
   acceptance. Verify actual Windows numeric/string version metadata in both
   native and package jobs and inspect final Windows package bytes.
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
- [PNG third edition: animation](https://www.w3.org/TR/png-3/#11acTL) and
  [image crate PNG decoder](https://docs.rs/image/0.25.10/image/codecs/png/struct.PngDecoder.html).
- [Windows fixed version fields](https://learn.microsoft.com/en-us/windows/win32/api/verrsrc/ns-verrsrc-vs_fixedfileinfo)
  and [FileVersionInfo](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.fileversioninfo):
  independent numeric and string artifact verification.
- [winresource constructor defaults](https://docs.rs/winresource/latest/src/winresource/lib.rs.html):
  the reviewed 0.1.34 version derives numeric and textual resources from Cargo.
- [GitHub Spec Kit](https://github.com/github/spec-kit) and
  [required current-commit status checks](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).
