# Native codec compatibility contracts

These historical contracts describe the codec compatibility assertions and
independent source fixtures. The public CLI and panel now select the Rust media
worker; third-party Pillow, SVG and Lottie codecs still use CPython. Current
runtime and platform results are tracked in [CI acceptance](rust-ci-status.md),
and historical source mappings in [native test migration evidence](native-test-migration.md).

## Gallery reads and preview lifetimes

Static gallery requests use the existing typed Pillow renderer directly, without
flattening its exception through the legacy String API. The original gallery
goldens remain unchanged. A corrupt-image assertion requires
`UnidentifiedImageError`, the handled-error response and exit 1, no output, and
unchanged source bytes. Video gallery copies retain the actual external
`Path.read_bytes` exception before existing exclusive staging and publication.
The same five original goldens and controlled read failures cover exact bytes,
`OSError` handling, escaping `TypeError`/`KeyboardInterrupt`, occupied-output
preservation, and restored external bindings. Staging, write, publication, and
cleanup failures still have explicitly unverified exception classification.

Video panel previews preserve actual Pillow encoding errors through the typed
worker while retaining the legacy String API's generic encoding failure. Three
unchanged source-generated preview goldens exercise the typed core; controlled
save faults exercise the actual worker, and a legacy control checks its message.
The external patch stays owned while native waits run detached from Python and
is restored afterward.

Preview frames close explicitly in forward order after the encoding body settles.
The first close exception replaces a pending save/getvalue failure. The existing
consuming image owner prevents retrying that failed explicit close. Native RAII
still attempts remaining-frame cleanup on failure or unwind, closes already-built
frames if later construction fails, and closes the buffer. These stronger cleanup
guarantees are deliberate adaptations, not exact source close-call-count parity.
Ten controlled lifetime variants and a legacy-error control reuse the same serial
gallery cohort and pinned clip. Compilation, runtime precedence, filesystem/IPC,
and platform acceptance remain unverified.

## Static image context exits

Static metadata, previews, gallery thumbnails, reencoding and tinting use the
source's actual image context enter/exit calls, including the entered image and
body exception triple. An exit failure overrides a pending body failure. Preview
encoding happens after that context exits; gallery encoding stays inside it.
Ordinary reencode/tint errors still produce their original skipped warning, while
interruptions escape. Fingerprints and comparisons are not converted into context
workflows where the source did not use one.

The shared image owner additionally closes resources on failure or unwind. This
stronger cleanup is an adaptation, not exact close-call-count parity or support
for arbitrary context managers that suppress exceptions. Existing static lifetime
assertions cover controlled entry, exit, body/exit precedence and failure-before-
output behavior alongside the retained codec and animation cases. Compilation,
real Pillow closure faults and runtime behavior remain unverified.

## Static fitting call order

Typed conversion and provider images share one fitter with an explicit source
policy. It observes mode, conditionally converts to RGBA, uses `split()[3]`, tests
the bounding box's Python truthiness and crops only when truthy. It then reads
size, resolves the resize method before `Image.LANCZOS`, resizes, creates the
transparent canvas and pastes without a mask. Zero dimensions return a blank
canvas without resizing. The legacy conversion API keeps its previous channel,
bounding-box and eager-canvas ordering through the same fitting body.

The original mapped pixel test remains at its existing identity and now covers
both policies. Five conversion and seven provider goldens are unchanged; seventeen
controlled external-call variants cover failure ordering without extra encodes.
Fitting errors stay outside the provider's optional decode catch. All attributes
are restored before assertions. Sizes still use the existing native unsigned
integer admission, and deterministic closing of owned images/bands is a stronger
native ownership adaptation, not arbitrary-object or close-call-count parity.
Compilation, actual Pillow behavior and runtime acceptance remain unrun.

## Identity resampling lookup

Canonical static normalization and dHash share one LANCZOS call boundary. It
resolves the image's resize descriptor before observing the direct
`Image.LANCZOS` alias, preserving the original first error and avoiding an
unreached argument lookup. Pixel dimensions, rounding, hash algorithms and
existing String/Option/error-catch policies are unchanged.
Existing static fault tests keep their mapped identities while a cohesive shared
fixture adds ten static/frame call-order variants, without extra encodes or
processes. Attributes are restored before assertions. Compilation and runtime
proof remain unrun.

## Premultiplication call order

dHash observes the original image's mode once through actual Python equality.
Its shared band-processing body is unconditional: comparison already supplies
normalized RGBA and must not inspect mode again. The body resolves `Image.merge`
before its arguments and resolves `ImageChops.multiply` separately for red,
green and blue. This preserves the first external error without caching a
callable the original would look up again. Pixel math and existing typed,
String, Option and ordinary-exception catch policies remain unchanged.

Ten no-encode fault scenarios reuse the existing static and comparison test
cohorts. They distinguish lookup from call, ordinary errors from interruptions,
and owned resources from borrowed inputs; external attributes are restored
before assertions. Four-band Pillow admission and stronger RAII cleanup remain
native adaptations, not general iterable-unpacking or finalizer parity. All new
compiler, Pillow and runtime assertions remain unexecuted.

## Native comparison arithmetic

The Rust-owned comparison keeps byte differences and channel means native rather
than calling Pillow `ImageChops.difference` and `ImageStat.Stat`. For admitted
64×64 RGBA byte rasters, channel sums are at most 1,044,480 and division by 4,096
is exact in binary64; the four-channel mean therefore preserves the original
`<= 8` decision. This does not reproduce faults injected into removed Pillow
statistics calls. Necessary pixel extraction still preserves its actual Python
error, while malformed native rasters remain unknown errors rather than false.
Four no-encode public frame comparisons author both directions at means 7.5, 9,
exactly 8 and 8 + 1/16,384. Each requires dHash zero before checking its literal
comparison result, so a structure refusal cannot masquerade as the colour boundary.
These assertions are uncompiled and unrun; runtime and package acceptance remain
unverified.

## Static tint lookup ordering

The shared static reencode/tint encoder resolves the fitted colour image's
`putalpha` method before requesting the original alpha channel, matching Python's
function-before-arguments evaluation. A failing method descriptor must not call
`getchannel` or reach encoding. Existing ordinary failures still warn and skip;
interruptions escape, and preparation refuses an unapplied requested tint.
The original four tint goldens are retained alongside a serialized descriptor-fault
cohort; external bindings are restored before assertions. The fixture retains
failed inputs/output instead of deleting them during assertion unwinding. These
source-authored checks have not compiled or executed.

## Strict usability and collision identities

Strict video `usable` validates before checking blankness. Typed direct probes
preserve command errors without decoder-selection wrapping; only decoder
selection adds the original contextual refusal. Typed blankness uses the same
frame core, cache, and complete-frame alpha policy as the legacy String API.
A completed list of known Telegram-policy violations becomes `MediaError`.
The shared probe now retains actual Python JSON/coercion errors; native
filesystem/runtime failures stay explicitly unverified. Existing encoded validation
clips, controlled-tool recovery, and pinned worker IPC inputs author policy
refusal, valid blank/nonblank results, direct escape, and input preservation.
No additional encode is introduced. These paths remain uncompiled and unrun.

Collision requests retain actual `Path.read_bytes` exception identity and share
one Rust hash/lookup policy. Invalid keys fail before reading; `OSError` is
handled, while `TypeError` and `KeyboardInterrupt` escape the worker catch
boundary. The direct legacy collision helper keeps its generic String error;
worker String calls expose the typed message. Eight unchanged original goldens
and controlled read faults author these checks. Runtime proof is pending.

## Metadata operation ordering

Worker metadata shares one typed image-context implementation without forcing
unrelated observations. `dimensions` loads and reads size but does not inspect
mode or blankness. `image_info` loads, reads size and mode, then checks blankness.
Static `usable` goes directly to blankness without explicit metadata load/getters.
The public metadata API keeps its existing schema for callers requesting it.
Controlled getter/load failures and exact observation-order assertions reuse the
same lifetime cohort; compilation and runtime proof remain unrun.

## Static alpha blankness

Source-shaped static callers share actual Pillow `convert("RGBA")`, `split()[3]`,
`getbbox` and `histogram` calls. Only a `None` bounding box skips the histogram.
Visible alpha buckets above 10 are summed with Python integers and compared with
8, without native integer overflow. Malformed histogram shape or counts remain
native uncertainty, never a successful blank result. Metadata exits its image
context before reporting that structural uncertainty.

Typed conversion checks blankness after fitting and before PNG encoding. Providers
check before fitting; audit identity bytes precede blankness. Publication treats
only an ordinary image-open failure as optional evidence; alpha failures and
interruptions still propagate. Its existing Python binding represents structural
uncertainty with the explicit native `RuntimeError` diagnostic
`native alpha histogram is unverified`; native callers retain an unverified codec
error directly. Legacy combined inspection and String conversion keep their
byte-based policy. Video raw-frame blankness is unchanged.

Existing goldens and serial fault cohorts author call order, short-circuiting,
actual exception classes, large counts, malformed histograms, context exit and
no-output assertions. Shared fixture extraction preserves the existing lifetime
and animation cases. Compilation, actual Pillow faults and runtime parity remain
unverified; this is not arbitrary image-object protocol equivalence.

## Video size observation

Typed video validation performs one successful probe, then actual
`Path.stat().st_size`, before its ordered Telegram checks. Python comparisons and
formatting retain the size object without unsigned narrowing. Ordinary stat or
getter errors and interruptions keep their actual class. Legacy validation retains
native metadata and its generic diagnostic; the public `u64` policy API delegates
to the same ordered validation body.

The existing controlled recovery cohort authors failed-probe-before-stat,
post-probe stat/getter faults, restored bindings and failure-before-blank-decoding
checks. Four additional controlled probes bring that case to six, with no extra
encode or target. Its deadline is unchanged and waits use the remaining budget.
These assertions and the literal cap/large-integer controls remain uncompiled and
unrun; process, timing and platform acceptance are pending.

## TGS size observation

TGS file validation retains the actual `Path.stat().st_size` object for Python
comparison and diagnostic formatting before either byte read. It does not narrow
a signed or arbitrary-size integer to `u64`. Stat, getter and comparison errors
remain outside the loader's narrower catch. Encoded-byte and document APIs keep
their existing `u64` signatures and share the same cap policy and decoded-document
validation body; the file path does not repeat the size comparison after reading.

The existing serial file cohort authors signed/large values, cap edges, actual
stat/getter failures, read short-circuiting and restored bindings through direct
validation and worker dispatch, using one unchanged valid TGS fixture. It adds no
process or encoding pass. These assertions are uncompiled and unrun; native
filesystem admission, cleanup and complete error compatibility remain unverified.

## Gzip context completion

The shared Lottie decoder and deterministic encoder now use the actual gzip
context protocol: type-level `__enter__`/`__exit__`, the entered object and the
original exception type, value and traceback. Exit errors override body errors.
JSON decoding, size follow-up and encoded-buffer inspection wait for successful
exit. Existing caller-specific raw-byte fallback, validation wrapping and optional
rewrite warnings consume that final error; they are not separate gzip policies.

A failed enter or native unwind retains fallback close ownership without masking
the primary error. After an attempted exit, native cleanup does not retry close.
These are explicit native ownership adaptations. Standard `GzipFile` does not
suppress exceptions; a custom truthy exit retains a pending body error rather than
inventing successful missing bytes. The existing serial load suite authors actual
gzip proxy faults, close precedence, restored APIs and unchanged source/package
goldens. Compiler, runtime and platform evidence remains unrun.

## Media header reads

Typed detection and static extension selection use the original bounded binary
`open`/`read(16)` boundary. Actual open, read and context-exit errors retain their
Python class; an exit failure overrides a pending read failure. Video and animated
extensions return without reading. The legacy String APIs retain their native
reads and generic diagnostic, sharing the same classification policy.

The existing format cohort retains all 22 source header/extension goldens and adds
restored external open/read/exit faults, read-size and closure-order assertions,
including typed worker detection. These are authored and unrun. Native input
admission remains separate; fallback closure before an entered context is a
stronger resource-cleanup adaptation, not complete arbitrary context-manager parity.

## Request membership errors

Typed request validation preserves Python set-membership errors for JSON lists
and objects used as operation, truthy format or preview size. Invalid preview
keys fail before size hashing. Falsy formats still select detected media; a
hashable unsupported format may reach conversion target dispatch, as in the
original worker. Shared preview admission serves both typed and legacy messages.
The existing worker-contract and IPC targets contain the type/order assertions;
they remain uncompiled and unrun.

## Comparison byte shortcut

Typed worker `compare` checks file sizes and then byte equality before initializing
video tools. Actual `Path.stat` and `Path.read_bytes` failures retain their Python
exception identity and escape the identity probe's ordinary-error catch.
`same_image` never takes this byte shortcut. Different sizes skip both byte reads.
Typed static/vector comparison preserves both content keys before perceptual
hashes or colour checks. Equal keys return without a hash. Actual ordinary
`Exception` failures become an unknown result; interruptions and native admission
or malformed-raster failures remain errors. Vector mismatch stays unknown, not
false. Shared raster primitives serve the legacy and typed APIs; Option-returning
legacy wrappers remain deliberately lossy. The direct legacy worker path is not
switched to the typed comparison dispatcher.

Equal undecodable video bytes, actual path-error classes, first-error short
circuiting, size mismatch, key/hash/colour order and input preservation have
authored assertions in the existing comparison target. External patches are
restored and serialized against that target's other tests. The existing IPC
cohort also requires successful null responses for undecodable static/video and
invalid vector-root comparisons. These assertions remain uncompiled and unrun.

## Video timeline comparison

Typed video comparison uses the same bounded decoder, timestamp parser, raster
primitives and success-only timeline cache as the legacy API. Timestamp extraction
uses the original byte regex and actual Python `float(bytes)` conversion, without
requiring unrelated FFmpeg metadata to be valid UTF-8. Duration, frame-count,
alignment and final-frame refusals retain the source `UndecodableVideo` policy.
Only the outer typed identity comparison converts ordinary Python `Exception`
failures into an unknown result. Interruptions and native admission, custody,
idle-expiry or malformed-raster failures remain errors.

The existing video cohort reuses its pinned clips, generated comparison clips and
cache for typed positive/negative/unknown assertions and restored Pillow failures;
no additional encode is introduced. Legacy controls remain. These assertions are
uncompiled and unrun. The source eagerly constructs its Pillow frame lists while
native comparison constructs pairs on demand, so complete frame-construction
exception order is not claimed identical. Inner typed decoder selection now uses
the same external header read as typed detection. Its actual error ancestry stays
separate from probe metadata, so only the outer comparison may classify an ordinary
header error as unknown. Typed frames, fingerprints, previews and conversion share
that selection path; legacy native reads remain unchanged. The existing header
cohort adds direct selection/frame faults and four cold-probe comparison assertions;
they remain uncompiled and unrun.

## Fingerprint policy

Typed static fingerprints suppress only ordinary Python `Exception` failures in
phash computation. `KeyboardInterrupt` and `SystemExit` retain their actual class
and escape instead of becoming a successful fingerprint with a null phash.
The shared dHash and pixel-key algorithms are unchanged. The source-shaped
static fingerprint seam retries only the key after an ordinary initial decode
failure, then returns a null phash; an interrupt never triggers that retry.
Native malformed normalization or dHash buffers remain explicit unknown errors.
The existing static-error target uses restored, serialized Pillow faults and
unchanged goldens to author retry/count/error/input assertions. They are
uncompiled and unrun. Legacy metadata/String APIs retain their schemas and
flattening behavior; complete filesystem/lifetime compatibility remains unverified.

Typed video fingerprints share the existing decode/cache flow. A successfully
probed unnamed codec or established missing alpha decoder produces the original
handled `UndecodableVideo` refusal; unknown native failures stay unverified.
The canonical sampled stream is decoded once. Its phash catches only ordinary
`Exception`: interruptions and `SystemExit` escape. Only an empty successful
sample hashes the source's actual `Path.read_bytes`. A failed decode never
becomes that fallback.

Existing source goldens, controlled-tool counts, and serial external-library
faults author these contracts. The direct legacy fingerprint API retains its
previous String/Option behavior. Typed fingerprints preserve actual command
failures as described below; complete exception compatibility and
compiler/runtime/platform evidence remain acceptance gates.

## Lossless probe metadata

One Rust-owned parser orchestrates external UTF-8 replacement decoding,
`json.loads`, and the original ordered Python coercions. The metadata carrier
retains arbitrary signed integers, Python strings including lone surrogates,
and nonfinite floating-point values. Decoder selection, strict validation,
remuxing and timelines consume that carrier without a JSON round trip. The
independent video/audio stream passes preserve their original stopping order.

Direct parse/coercion failures retain actual exception identity; decoder-selection
contexts wrap only ordinary `Exception`, not interruptions or native unknowns.
The public JSON probe APIs remain checked compatibility projections: nonfinite
floats or strings that JSON cannot represent losslessly raise an explicit
unverified error, never become null or a default. Their representable schema is
unchanged. The older JSON validation entry points retain finite/u64 admission
and delegate to the same validation policy; they are not the lossless source seam.

Source validation and remux preserve their original float comparisons, including
NaN, while timelines still require finite, positive, bounded duration. Legacy
remux retains its finite-safety refusal. Parser/coercion/error assertions and the
existing controlled-tool cohort's malformed-JSON, null-format, first-NaN and
second-NaN remux cases are authored. All compilation, runtime, process/platform
and package compatibility proof remains unrun; native I/O and command-text
adaptations are not claimed source-identical.

## Video command errors

Typed video fingerprint, strict usability, and preview paths construct the unchanged external
`subprocess.CalledProcessError` from the actual command, exit status, stdout,
and stderr. Unix signal deaths have negative return codes; Windows exit codes
retain their unsigned DWORD value. Direct decode failures escape the worker's
handled-error boundary. Only the original decoder-selection probe and capability
contexts wrap ordinary command failures as `UndecodableVideo` with the original
context prefixes. Integral positive wall limits produce the source-shaped
`MediaError` timeout before any such contextual wrapping.

Native idle expiry, admission, custody, capture-read and cleanup failures
remain explicitly unverified. JSON/coercion failures use the lossless probe
contract above. Unknown outcomes never become an empty sample or
missing decoder. External exception construction or formatting failures retain
their own error rather than a manufactured contextual wrapper. Existing String
APIs preserve their generic diagnostics and nested admission/refusal topology;
there is still one capture, probe, and decode flow.

Canonical native tool paths and additional FFmpeg flags differ from the original
arguments, so command-text diagnostics are not claimed byte-identical to the
original command text. Literal outcome assertions and the existing controlled-tool
failure/recovery cases are authored, not executed. A pinned truncated-video IPC
case distinguishes decoder-selection refusal from direct strict-validation escape;
its exit/no-response observation alone does not certify the exception class.
Separate core-class assertions retain that distinction. Preview's shared body
preserves typed command failures while the legacy route keeps its old generic
messages and encoding-error behavior. Staging, final-write and native I/O
exception classification remains incomplete. The production adapter is not switched.

## SVG conversion errors

Typed static conversion catches ordinary exceptions only around the external SVG
renderer invocation and turns an unavailable rendering into the source
`MediaError` refusal. Renderer import errors, interruptions and failures during
subsequent `bytes(...)`, buffer, Pillow open or conversion steps retain their own
class. Legacy conversion selects its original renderer/error policy through the
same fitting, PNG and persistence implementation.

The existing SVG lifetime and static conversion cohorts retain their goldens and
add restored external fault matrices, legacy controls and no-output/input checks.
The source adapter and caller integration are source-reviewed, not compiled or
runtime-verified. Independent cleanup of image and buffer remains a stronger
native adaptation; its actual platform behavior is still an acceptance gate.

## Video conversion

Typed conversion shares the original five-step CRF loop with the legacy String
API. Decoder selection retains contextual refusals; actual encode failures use
the direct command error contract, and exhausting the byte limit yields
`MediaError`. Native staging and filesystem failures remain explicitly unverified;
probe metadata follows the lossless contract above. The typed worker returns the produced output's fingerprint using the
same decoder instance. It does not start another codec worker or encode again.

A staged sibling is retained while capture custody is uncertain, including
cancellation. Only a settled capture permits normal cleanup; a failed explicit
cleanup is not retried by `Drop`. Literal staging assertions cover the keep flag,
not actual cancellation or platform custody. Existing controlled conversion
successes, byte-limit refusal, and truncated-input worker assertions are authored;
compiler/runtime proof remains pending. A fingerprint failure after publication
may leave the produced output visible, as in the original ordered workflow.

## Video preparation ordering

Typed video preparation checks the reencode flag first. With reencoding disabled,
tint-only errors do not require video tools or runtime initialization. With
reencoding enabled, the existing remux runs before tint validation; only then does
an untinted result receive its typed fingerprint. Invalid tint strings retain
`MediaError`, nonstring tints retain `ValueError`, and a supported tint syntax
still refuses video repainting with the original warning and `ValueError`.
The worker drains collected warnings even when preparation returns an error.
The real worker writes each redacted warning once to its existing structured
per-run file logger, retaining the original standalone stderr line. The structured
logger also echoes its own warning line; these two console formats are deliberate,
while the file warning and summary count occur once. Direct typed calls without a
run logger retain only the standalone warning. The existing IPC cohort isolates
its copied executable root, reads exactly one new operational log after each
owner finishes, and checks warning/error attribution and counts. Those filesystem,
process and runtime assertions remain unrun.

The no-tool refusal matrix, unchanged-video worker result, truncated-input error,
and an existing copied-clip remux-before-invalid-tint case are authored. That
ordering case requires changed container bytes and the original decoded-pixel
digest without an extra remux. Legacy direct preparation remains unchanged.
Typed remux now returns its observed rewrite and optional warning separately.
The legacy boolean API and its one-probe behavior remain available through the
same shared remux body. A typed overduration result triggers the original second
probe only when the first duration exceeds three seconds; the warning uses that
second observation, even if it is shorter. An ordinary second-probe failure
produces the skipped warning instead. Equality precedes the size cap, so identical
oversized bytes remain silently unchanged.

Actual Python `Exception` membership, captured before error flattening, decides
which settled codec failures produce skipped warnings. Worker-handled status is
not that catch boundary. Interrupts, native idle expiry and unknown custody remain
errors, with staged evidence retained. The initial byte read and final atomic
replacement remain outside the skip catch. Native admission, staging,
atomic-write and cleanup error compatibility remains incomplete; actual
native argv/captured bytes are retained rather than substituted with an unexecuted
original command. Controlled-tool cases author exact warnings, conditional probe
counts, unchanged source bytes, process deaths and retained admission evidence.
Compiler/runtime/platform proof remains pending.

## Static reencode initial read

Typed static reencoding performs the actual source `Path.read_bytes` before its
optional encoding catch. Read failures retain their Python class and escape rather
than becoming a skipped warning; they also precede tint validation in preparation.
The legacy API keeps its native read and generic read diagnostic through the same
reencode body. Exact lossless encoding, equality and atomic replacement are unchanged.
Native input admission and final-write error compatibility remain unverified.
The shared atomic writer returns native I/O errors, not observed Python write
exceptions; converting those into Python subclasses would not establish source
exception identity. Preparation therefore keeps its explicit unverified errors
rather than replaying a potentially truncating write. On Unix, parent-directory
synchronization follows replacement, so a returned write error does not always
mean the destination still contains its old bytes. No automatic retry or rollback
is implied by that error; post-replacement failure acceptance remains open.

The existing static-boundary cohort retains its 18 original observations and adds
restored initial-read faults for ordinary exceptions and interruptions, including
preparation and typed worker dispatch. Assertions require no encoding call, no new
warning and unchanged inputs. They are uncompiled and unrun; formatting and source
inspection do not establish the actual error or filesystem behavior.

## Unhandled worker diagnostics

An error outside the worker's handled catch still exits 1 without a response.
Its redacted class/message now also reaches the existing structured per-run file
logger before the unchanged standalone stderr diagnostic. This preserves the
cause when the adapter discards stderr. The existing IPC dictionary cases author
exactly one structured cause and one counted failure summary; these assertions
are unrun. This is native observability, not source traceback-byte parity.

## Settled process outcomes

The shared capture implementation retains the actual finished exit status,
wall expiry, or native idle expiry together with bounded stdout/stderr. A
structured settled result is returned only after owner finalization, capture
admission, and scratch cleanup succeed. Admission, observation, output-limit,
read, custody, and cleanup errors remain outer errors. They cannot become an
optional refusal, blank sample, or successful negative answer.

Existing public capture APIs and nested String adapters retain their messages.
No separate process backend or duplicate capture loop is introduced. Reads check
pre-open regular/nonlinked files, Windows reparse attributes, opened-file type,
and size limits. These checks do not prove complete TOCTOU, hardlink,
ancestor-replacement, or opened-object identity protection. Literal outcome and
owned-file assertions are authored; actual process and platform proof is unrun.

## Final destinations and retained evidence

The additive native adapter permits absolute final artwork outside its configured
root. IPC and temporary storage remain project-contained, and direct worker
outputs stay inside their admitted envelope. Destination ancestors must be
physical and nonlinked. Existing outputs are never overwritten. Exclusive
publication follows verified descendant completion and scratch cleanup. An
existing real-consumer cohort checks an original preview golden at a sibling
private destination. Cross-volume, runtime, and platform proof remains unrun.

Failed, malformed, and cancelled worker envelopes are retained only after the
process owner has finished. The complete envelope moves into an exclusively
created project-local
`logs/native-codec-diagnostics-<pid>-<counter>/envelope` directory. The returned
worker error stays unchanged; a separately redacted notice names the retained
path. Unknown ownership or refused relocation leaves the original scratch intact.
A partially failed explicit cleanup is not retried by `Drop`. Successful calls
still clean their original workspace before publishing final artwork.

Gallery, converter, local-ingest, and ticker guards retain any nonempty codec
container independently of ownership markers. Private-download cleanup already
has this rule. Only an empty or absent codec container permits outer cleanup.
Existing fixture tests author exact bytes, acknowledged process deaths, malformed
responses, cancellation, blocked diagnostics destinations, and these outer guards.
They remain uncompiled and unrun; diagnostic files are not parity or authenticated
run receipts.

Output-producing calls persist an exact-byte request/accepted-response witness
before deleting their successful worker envelope. Original scratch cleanup still
precedes final publication. A final collision, publication failure, or cleanup
refusal retains the witness; full success removes it. No-output calls create no
witness. Persistence failure stops before original cleanup or publication.

The witness payload is at most 128 KiB: up to 128 KiB written and another 128 KiB
read back, excluding metadata and sync I/O. Latency and platform behavior remain
unmeasured. Witness-cleanup failure after publication returns an error without
rolling back visible artwork; partial cleanup may retain only a remainder. This
is diagnostic preservation, not transactional rollback, a crash-persistence
promise, or authenticated execution evidence.
