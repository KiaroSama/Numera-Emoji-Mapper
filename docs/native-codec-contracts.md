# Native codec compatibility contracts

The production media worker executes Rust and bundled native libraries, with no
CPython, PyO3 or Python subprocess. Raster operations live in
`runtime/src/native_raster.rs`; SVG uses resvg; Lottie uses the native canonical
JSON implementation and rlottie renderer. Video uses bounded FFmpeg/FFprobe
children. Historical Python implementations remain independent test oracles in
the optional compatibility harness.

Existing frozen fixtures establish catalogue keys, decoded pixels, alpha,
validation and comparison behavior. Do not regenerate expected identities from
the replacement. PNG/WebP encoder bytes may differ; an encoding difference does
not permit a decoded-pixel or identity change.

## Gallery reads and preview lifetimes

The gallery reads one selected source and publishes into an exclusive staged
output. Static thumbnails fit within 88×88 with Lanczos, do not enlarge smaller
images, and encode WebP at quality 65. Static preview sizes are 52, 72 or 104;
previews use bicubic thumbnails and lossless WebP with exact hidden RGB.

Lottie/video previews use bounded frame sampling. Animated WebP previews use
libwebp's animation encoder, including duplicate-frame coalescing, spatial deltas
and the terminal timestamp that determines the last frame's duration. The
separate exact full-frame encoder preserves lossless pixels, hidden RGB and each
supplied frame boundary. These are different encoding policies with separate tests.

## Static image context exits

Native image buffers and codec resources have ordinary Rust ownership. WebP
pictures, writers, muxes and decoder allocations have RAII guards, including
error paths. There is no production Pillow object whose close method can replace
an operation's result. Historical object-lifetime fault tests remain in the
compatibility harness; native tests exercise publication, allocation and cleanup.

## Static fitting call order

Convert to straight RGBA, derive bounds from nonzero alpha, trim when bounds
exist, and fit proportionally into a transparent 100×100 canvas. Dimensions use
ties-to-even rounding and are at least one pixel. Paste without a mask so partial
alpha is not applied twice. Zero-sized input yields a blank canvas.

Provider admission checks source blankness **before** fitting: enlarging eight
visible source pixels must not turn a placeholder into an accepted emoji.
Conversion checks the fitted canvas before publishing.

## Identity resampling lookup

Normalized static identity is RGBA resized directly to 64×64 with the historical
Lanczos kernel, then SHA-256; the key is `s:` plus the first 32 hexadecimal digits.
The Rust implementation preserves the separable 22-bit fixed-point coefficient
rounding, eight-bit rounding after each pass and the vertical-first pass for
exceptionally tall inputs. A same-size resize preserves the source bytes.

JPEG uses bundled libjpeg integer IDCT and fancy chroma upsampling: the image
crate's alternative decoder rounds valid pixels differently and would change
catalogue keys. RGB, grayscale, CMYK and progressive/chroma cases have independent
historical references. Sixteen-bit PNG conversion preserves integer-luminance
clamping and high-byte color conversion.

## Premultiplication call order

RGBA resizing rounds multiplication by alpha when converting to premultiplied
samples, resamples, then removes premultiplication with the historical integer
division. Comparison/dHash multiplication uses floor(`channel*alpha/255`)
instead. Those two arithmetic rules are intentionally distinct.

Original RGBA sources are premultiplied for dHash. Other source modes convert
to luminance without that extra multiplication. Provider hashes explicitly
convert to RGBA first. The mode distinction is preserved by decoded raster
metadata and must not be inferred solely from the RGBA buffer.

## Native comparison arithmetic

dHash uses historical luminance coefficients and a 9×8 Lanczos raster. Each of
64 row-major bits records whether the left pixel exceeds the right one.
Static comparison accepts an equal normalized key; otherwise it requires both
a dHash distance at most 6 and mean absolute premultiplied RGBA difference at
most 8 over 64×64. Unknown decode evidence remains undecidable, not a proven match.

## Static tint lookup ordering

Static tint replaces RGB with the requested color and retains the original alpha,
including hidden RGB at alpha zero. Lottie tint preserves alpha and gradient
opacity while recoloring static colors, animated `s`/`e` values and the declared
color stops. Production tint input is six hexadecimal digits, optionally
prefixed by `#`. Video tint is refused.

## Strict usability and collision identities

Strict static usability is the inverse of blankness. Publication admission is a
separate policy: missing decode evidence alone must not be labeled blank.
Collision identities hash the complete original bytes, retaining the first key
component and final lookup component: `prefix:<digest>:lookup`, where the digest
is the first 32 hexadecimal digits of SHA-256.

## Metadata operation ordering

`image_info` returns `{size:[w,h],mode,blank}`; `dimensions` returns `[w,h]`;
`usable` returns a boolean. These operations do not add fingerprint fields.
Fingerprints return identity separately. All are executed by the native worker.

## Static alpha blankness

A raster is blank when at most 8 pixels have alpha greater than 10. Alpha 10 is
not visible under this rule; nine pixels at alpha 11 are visible. Hidden RGB does
not establish visibility. Frozen sparse-provider fixtures cover these boundaries.

## Video size observation

Video validation uses the source file size and observed metadata in a stable
refusal order. Accepted output is 100×100 VP9/WebM, at most 256 KiB, at most 3 seconds,
at most 30 fps and without audio. Unknown size or metadata is not silently replaced
with a successful validation result.

## TGS size observation

TGS validation keeps the 64 KiB compressed cap, 512×512 canvas, required Lottie
fields, finite timeline, at most 60 fps and at most 3 seconds. Packaging never rescales
an out-of-spec vector canvas. Unsupported masks remain a validation concern even
when a player can render them.

## Gzip context completion

Native gzip parsing validates the stream through completion and bounds decoded
work. Canonical JSON and reproducible gzip metadata preserve historical animated
identity. Decode failures remain codec failures rather than partially accepted
objects.

## Media header reads

Read at most 16 header bytes. EBML identifies video; gzip identifies animated;
PNG, GIF87a/GIF89a and RIFF/WEBP identify static. Otherwise use the supported
extension hints. Static output extension is`.webp` or`.gif` when those headers
match and`.png` otherwise, including JPEG input. Video and animated target
extensions remain`.webm` and`.tgs`.

## Request membership errors

The worker retains its JSON request schema, parameter bounds and meaningful
error kinds. Unsupported operations/formats and invalid preview parameters
refuse before media work. Preview keys cannot contain path separators. Native
validation reproduces the retained list/dictionary type-error distinctions where
the public request contract depends on them.

## Comparison byte shortcut

`compare` can accept exact original-byte equality before decoding. `same_image`
uses media identity and does not turn equal corrupt files into a decoded match.
File-read failure occurs outside the optional comparison result.

## Video timeline comparison

Video keys use the frozen 10 fps 64×64 RGBA identity stream. Video comparison uses
30 fps, checks every sampled frame and allows only the defined one-frame length
slack. It never treats key equality alone as proof: a short changed segment can
fall between identity samples. Decode uncertainty remains undecidable.

## Fingerprint policy

Static fingerprints preserve source-mode dHash and normalized pixel keys.
Animated identity canonicalizes Lottie JSON; its perceptual hash is null. Video
fingerprints retain their established sampled-stream policy. Source paths must
be absolute regular non-symlink files; reparse-point protections apply on Windows.

## Lossless probe metadata

Native probe parsing retains wide integers, nonfinite-number handling and string
edge cases needed by the public metadata/validation contract. Unsupported or
unrepresentable results refuse rather than being narrowed into plausible values.

## Video command errors

The native process owner bounds children, output capture and timeouts, settles
owned process trees and records stderr before returning a codec failure. It does
not delegate lifecycle control to a language interpreter. Alpha-capable VP8/VP9
decoders are selected from probed content, not file extensions.

## SVG conversion errors

SVG conversion renders at width 256 using resvg and the historical font-size 16
default, then decodes straight RGBA and fits the 100×100 output. Malformed SVG
returns `failed to render SVG: <name>`. Output dimensions and allocation are
bounded. SVG/SVGZ text is capped at 8 MiB before parsing. Referenced rasters
share native decode limits, a 64-image count budget, a 128 MiB aggregate input
budget and a 64-million-pixel aggregate budget. Nested SVG images retain valid
data/external leaf rendering, use the same bounded SVGZ decoding and stop beyond
eight nested image levels. Optional missing/unsupported references remain ignored
as before; real resource-limit violations refuse the render. SVG rendering is a
conversion step, not a new generic raster-fingerprint format. The committed
gradient conversion retains its exact fitted pixel hash.

## Video conversion

Conversion uses explicit alpha-capable input decoding, Lanczos fitting and
transparent padding before VP9 encoding. CRF escalation must respect the byte
cap. The final file is validated before exclusive publication; refusal retains
the original rather than publishing a partial result.

## Video preparation ordering

Optional reencoding precedes fingerprinting. Stream-copy remux is used only when
it retains content and meets byte/duration requirements. A remux that exposes an
under-declared duration beyond the cap keeps the original with a warning.
Interrupted or uncertain child work must settle before a result is reused.

## Static reencode initial read

Reencoding validates the source and takes one bounded snapshot before the
optional decode/encode catch. Codec refusal keeps original bytes and reports a
warning; initial read failure remains an error. Successful reencoding writes
lossless exact WebP atomically and is byte-idempotent for that encoder. Tint also
writes atomically and retains alpha.

## Unhandled worker diagnostics

Handled codec failures use the existing `{ok:false,error:...}` response envelope;
success uses `{ok:true,result:...}`. Structural/internal uncertainty retains a typed
failure and request-specific diagnostics. Redact secrets before console, response
or log output. Do not turn all failures into null, false or an empty successful
response.

## Settled process outcomes

A command outcome is usable only after its owned process tree and capture work
have settled. Timeout/cancellation cannot leave a writer racing publication or
cleanup. If cleanup cannot be proved safe, retain the owned evidence and stop.

## Final destinations and retained evidence

Publish only into an absent final destination through an exclusive staged file.
Never overwrite unrelated bytes, follow a linked destination or delete evidence
owned by another request. Responses, warnings and cleanup failures remain tied to
the exact request. Existing recovery journals and verified rollback bundles
continue to govern catalogue/media changes across codec updates.
