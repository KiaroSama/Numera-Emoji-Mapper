**First, comply with the owner's Rules, AGENTS instructions, hooks and prescribed Spec Kit chain. Preserve protected test admission, independent fixtures and exact-source CI.**

# Doctor diagnostics and configured-secret recheck

## Scope and authority

Baseline: main and authenticated v3.3.0 source
`7b1d91254dc18df3f3ccf94b71d5f77ea504dcfa`. The owner had already merged
PR #109 and completed Issue #108. This repair does not reopen their R17–R21
findings. It addresses three independently reproduced diagnostic regressions.
The audit agent may publish the implementation PR and evidence, but must not
merge, close records or resolve review threads.

This record connects requirements, clarification, research, technical plan,
regressions, implementation and verification. No tracked owner Rules, AGENTS,
`.specify` workspace or configured hook path was present in the inspected
checkout. No unavailable Spec Kit command is claimed to have run. The integration
agent must apply the owner's actual Rules and prescribed chain.

All human author and committer addresses must be `Kiaro.Sama.Dev@gmail.com`;
genuine bots retain the owner's exception. All 597 reachable baseline commits
passed that identity audit, so no history reconstruction was needed.

## R22: doctor described a different catalog from the real command

The released doctor alone used `EMOJI_DATA_DIR`. Real media commands defaulted
to installation-relative `collection` and accepted the explicit `--data-dir`
option. With an ambient external value, released doctor printed that external
catalog and private workspace, but an actual `add-media` invocation created
one SQLite item under the installation's default catalog. Supplying the media
command's explicit selector created the item in the external catalog instead.
Thus the physical-volume instructions could inspect one location while the
subsequent operation used another.

Doctor now accepts `doctor [--data-dir DIR]`, defaults to `collection`, resolves
relative paths against the installation and ignores the unsupported ambient
selector. This preserves existing selection by actual operations. Parsing
rejects missing, blank, duplicate, unknown and ambiguous dash-leading arguments
before loading configuration, using constant errors without argument echoes.
Prefix a literal dash-leading relative directory with `./`. Exact help remains
configuration-free.

Regression coverage compares doctor with actual media imports and SQLite rows
for default, relative and external selections while an ambient value conflicts.
It verifies the unused ambient path stays absent. The native-launcher and CLI
guides now pass the same explicit path to doctor and the operation. An existing
PowerShell panel example also separates `--data-dir` and its value: the panel
parser does not accept `--data-dir=...`. Two bounded baseline parser controls
confirmed this documentation correction without starting a server.

## R23: planned parent traversal and file leaves were misreported

Released doctor rejected valid paths such as `missing/../catalog` although
actual media imports succeeded there. The old missing-prefix resolver could
not peel a trailing parent component with `Path::file_name`. Separately,
doctor printed an existing regular file at `logs/tmp/external-data` as the
proposed private workspace; the real import correctly refused it and preserved
the file's sentinel bytes.

The diagnostic `describe_directory` helper now walks path components in order.
It retains the original ancestor link check and inspects each resulting
component with `symlink_metadata`. Existing components must be real directories;
links/reparse points and regular files are refused. Missing components remain
planned paths and are not created. A parent component pops the planned suffix,
then the newly exposed path is checked again. Consequently
`missing/../linked/../catalog` cannot conceal a link through lexical cancellation.
Existing real directories are canonicalized through the established resolver.
Additional parents cannot move an absolute filesystem root above itself.

The workspace description applies the same directory-type validation after
the existing confined resolver. Allocation, confinement, Unix owner/mode checks,
Windows DACL validation, cleanup and retained-evidence rules are unchanged.
Doctor neither creates catalog/workspace directories nor repairs permissions.
An unresolved location is still an advisory diagnostic under the existing
exit-status contract; a displayed location is not a writability, ACL or race-free
future-operation guarantee.

On Windows, a prefix is accumulated until its root component before filesystem
inspection. Installation-relative and root-relative paths retain existing
`Config::path` behavior. Still-drive-relative input such as `C:catalog` is
explicitly unresolved instead of guessing a drive-specific current directory.
The Windows regression covers this boundary. Link fixtures use junctions on
Windows and symlinks on Unix; the dangling-link case is explicitly Unix-only.

## R24: redaction registered the raw secret but consumers used its trimmed form

`Config::value` trims the repository's established Unicode whitespace predicate;
the old `secret_values` returned only the raw configured value. A synthetic
opaque credential with surrounding ASCII or Unicode padding therefore appeared
unmasked when its effective value occurred in a diagnostic path. Actual dotenv
controls reproduced the same failure with double-quoted ASCII inner padding
and single-quoted Unicode inner padding. Plain outer padding was already trimmed
by dotenv and passed its control.

`Config::secret_values` now registers both the original and exact effective
value for existing TOKEN, SECRET, PASSWORD and API_KEY key categories. A
`BTreeSet` removes duplicates; effectively blank values are ignored. Stored
configuration, source precedence and consumer normalization remain unchanged.
The existing longest-first redactor still determines replacement order.

Two new unit regressions verify all four key categories, raw and effective
forms, the project-specific Unicode predicate, unchanged configuration and
blank/nonsecret controls. Integration assertions place the synthetic effective
credential in a real doctor path and inspect stdout, stderr and every owned
log. They enable actual owned-dotenv loading and preserve its bytes. The earlier
doctor regression retains its identity and meaningful path assertions, but now
sets `NUMERA_EMOJI_MAPPER_NO_DOTENV=0`; its former helper disabled the very dotenv
file that the test created. No real credential or operator file was used.

## Practical verification and source binding

Authenticated released Linux x64 executable SHA256:
`7f220da67d232f432c0e1692ed98d311e28b1f4229505c5118e71b8b4d1c8322`.

Candidate ordinary-CLI executable SHA256:
`b10c810607475eb6e26873c3f369134bea1768f2390cb94f2f3bea34bc889602`.
This candidate was built from the reviewed working patch over the baseline,
not from an unchanged baseline checkout. Eight source/test/guide file hashes
were independently reviewed and matched before and after verification.

| Independent practical matrix | Released baseline | Repaired candidate |
| --- | --- | --- |
| Planned catalog/workspace and redaction paths | 20 pass, 6 reproduced failures / 26 cases | 26 / 26 pass |
| Doctor versus actual media destination | 5 diagnostic comparisons capture disagreement/refusal | 5 / 5 pass |
| Actual dotenv plain/quoted padding | 1 pass, 2 reproduced leaks / 3 cases | 3 / 3 pass |
| Frozen publication JSON contract | 69 scenarios, 1,032 assertions pass | Same 69 scenarios and 1,032 assertions pass |
| Additional frozen configuration edges | Candidate-only independent expectations | 8 scenarios, 81 assertions pass |

The last matrix covers duplicate/overlapping secrets, raw and effective forms
in one path, blank public whitespace, inherited precedence over an actually
loaded quoted dotenv file, and constant invalid-selector errors. Candidate
expectations were frozen before execution; all candidate matrices passed on
their first execution. The path probe intentionally uses the repaired explicit
selector while preserving its 26 logical expected outcomes. Original failed
receipts remain retained. Existing bytes, modes and link targets were checked;
normal diagnostic run logs are distinguished from catalog/workspace creation.

Local Rust 1.100.0 release build, formatting and all-target release Clippy with
warnings denied passed. The local build used process-scoped two-job compilation,
LTO disabled and 16 codegen units; it is supplementary executable evidence,
not a replacement for normal hosted release builds. An initial missing CMake
prerequisite was installed and the unchanged-source build then succeeded.

The single local protected-runner admission attempt stopped before any test:
`cannot enumerate private interfaces: Operation not permitted (os error 1)`.
Its exit code was 2. No guard was disabled, no protected executable was run
directly, and no local protected-test pass is claimed. Full execution therefore
belongs to the genuine supported hosted CI environments.

Five common-platform doctor tests, one Windows doctor test and two configuration
unit tests are added to the existing full harness. All original 1,516 identities,
independent media oracles, admission guards and security boundaries remain.
The existing workflows already execute these bodies; no dependency, workflow,
version, tag or badge churn is required for this repair.

## Final integration acceptance

The implementation PR's final verification comment must bind the published
source/tree, all current Checks, actual source checkouts and real test receipts.
Require full native Linux/Windows x64/arm64 execution, browser, Worker, launcher,
inventory, package and security gates, plus the separate PR Advanced Security
aggregate. A successful CodeQL upload alone is insufficient. A pending run,
an earlier source or a compilation-only result is not execution acceptance.

The owner-authorized integration agent must review every repository PR and
apply the owner's Rules until each is merged or closed with a complete,
documented disposition. Reconcile required unmergeable work manually instead
of discarding it. Commit and push with the required identity, merge a necessary
subsidiary branch only when authorized, then require the primary checks visible
on the latest resulting target commit. Fix any reproducible defect discovered
during that work in the same session. Do not leave a known partial repair for
another prompt, and do not keep retesting after the finite acceptance criteria
are satisfied without a new counterexample or relevant change.

Retain the dynamic Git-tag badge and tags link; existing signed v3.3.0 assets
remain immutable. Optional follow-ups for owner assessment are a versioned
publication JSON Schema and GitHub-native Secret scanning/push protection.
Neither is evidence of a failing JSON command. Keep the physical-volume and
interactive Windows/Explorer checks explicit: synthetic CI does not establish
live operator, live Telegram/Cloudflare or interactive desktop behavior.

## Primary references

- [Rust Path components](https://doc.rust-lang.org/std/path/struct.Path.html#method.components)
  and [file_name](https://doc.rust-lang.org/std/path/struct.Path.html#method.file_name).
- [Rust symlink_metadata](https://doc.rust-lang.org/std/fs/fn.symlink_metadata.html)
  and [create_dir_all](https://doc.rust-lang.org/std/fs/fn.create_dir_all.html).
- [SQLite read-only WAL behavior](https://www.sqlite.org/wal.html#read_only_databases).
- [GitHub Spec Kit](https://github.com/github/spec-kit) and the owner's actual
  Rules for the required project-specific chain.
- [Guarded test instructions](../../tests/README.md),
  [contributor requirements](../../.github/CONTRIBUTING.md) and
  [security policy](../../.github/SECURITY.md).
