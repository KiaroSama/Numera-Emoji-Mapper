"""Animated (Lottie / .tgs) emoji: loading, packaging, validation, previews.

Split from emojikit/media.py, which was at the file-size ceiling. media.py
re-exports every public name here, so ``media.validate_tgs`` and friends keep
working; new Lottie code belongs in this module.

Imports nothing from media.py on purpose: media imports this module at load
time, so a reverse import would be a cycle.
"""

from __future__ import annotations

import gzip
import io
import json
import math
import os
from pathlib import Path

from emojikit.errors import MediaError

# Animated emoji are the exception to media.SIZE: Telegram requires a 512x512
# Lottie canvas for .tgs, the same as animated stickers -- only the static and
# video sections of core.telegram.org/stickers narrow the canvas to 100x100.
TGS_SIZE = 512
TGS_MAX_SECONDS = 3.0
TGS_FPS = 60
TGS_MAX_UNPACKED = 8 * 1024 * 1024   # bound decompression of a hostile .tgs
TGS_MAX_BYTES = 64 * 1024        # animated emoji hard cap

# gzip magic: every .tgs starts with it (media.py sniffs formats with it too).
_GZIP_MAGIC = b"\x1f\x8b"


# --------------------------------------------------------------------------- #
# Animated (TGS) packaging + validation
# --------------------------------------------------------------------------- #
def _load_lottie(src: Path) -> dict:
    """Load a Lottie animation from .json or .tgs into a dict."""
    raw = src.read_bytes()
    if raw[:2] == _GZIP_MAGIC:
        # Bounded: a few KB of gzip can expand to gigabytes, and the compressed
        # size check happens after this.
        with gzip.GzipFile(fileobj=io.BytesIO(raw)) as gz:
            raw = gz.read(TGS_MAX_UNPACKED + 1)
        if len(raw) > TGS_MAX_UNPACKED:
            raise MediaError(f"{src.name}: Lottie expands beyond "
                             f"{TGS_MAX_UNPACKED} bytes")
    data = json.loads(raw.decode("utf-8"))
    if not isinstance(data, dict):
        # A list/scalar root would otherwise surface as AttributeError far away
        # from here.
        raise MediaError(f"{src.name}: Lottie root is {type(data).__name__}, "
                         f"expected an object")
    return data


#: Preview defaults, measured against the real 146-animation catalog.
#:
#: Quality: lossless averaged 507 KB per animation (72 MB for the set); lossy
#: q60 is visually indistinguishable at thumbnail size -- checked on a QR-code
#: emoji, the worst case for lossy artefacts.
#:
#: Frame rate is the one that matters for how the panel FEELS, because the cost
#: is the browser decoding every frame of every card on screen, and this grid
#: can show 60+ cards at once. Measured per animation: 30fps = 54 frames /
#: 119 KB, 20fps = 37 / 80 KB, 15fps = 28 / 60 KB, 12fps = 22 / 48 KB. 15 halves
#: both the decode work and the bytes against 30 and still reads as motion on a
#: 104 px tile. Override with ``python -m emojikit.panel --preview-fps`` rather than editing
#: this; the cache is keyed by content hash, so changing it means clearing
#: ``<data-dir>/preview/``.
PREVIEW_SIZE = 104
PREVIEW_FPS = 15
PREVIEW_QUALITY = 60


def lottie_still_webp(src: Path, out: Path, *, size: int = PREVIEW_SIZE,
                      quality: int = PREVIEW_QUALITY) -> Path:
    """Frame 0 of a Lottie animation as a single-frame WebP.

    The panel shows this for cards that are off screen. An animated image is not
    free just because you cannot see it: the browser holds its decoded frames,
    and at ~60 frames of 104x104 RGBA that is ~2.5 MB each -- 361 MB if all 146
    of this catalog's animations buffer at once. Swapping the off-screen ones to
    a still bounds live animation to roughly what fits on screen.
    """
    from rlottie_python import LottieAnimation      # optional; see requirements

    data = _load_lottie(src)
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_name(out.stem + ".tmp" + out.suffix)
    anim = LottieAnimation.from_data(json.dumps(data))
    try:
        frame = anim.render_pillow_frame(frame_num=0, width=size, height=size)
    finally:
        anim.lottie_animation_destroy()
    frame.save(tmp, lossless=False, quality=quality, method=4)
    os.replace(tmp, out)
    return out


def lottie_preview_webp(src: Path, out: Path, *, size: int = PREVIEW_SIZE,
                        fps: int = PREVIEW_FPS,
                        quality: int = PREVIEW_QUALITY) -> Path:
    """Rasterise a Lottie animation to an ANIMATED WebP for previewing.

    A browser plays an animated WebP natively, on the compositor, at one DOM
    node. The alternative -- a lottie.js SVG player per item -- costs ~704 DOM
    nodes each: a 146-item catalog measured 1 426 document nodes with none
    mounted and 8 476 with ten, so a full grid was six figures of nodes that
    every scroll rebuilt. That is what made the curate panel unusable, and no
    amount of lazy-mounting fixes it, because the cost is the renderer.

    Goes through ``_load_lottie`` rather than handing the .tgs to rlottie
    directly: that is where the decompression bound lives, and a preview path
    that skips it would be a gzip bomb away from unbounded memory.
    """
    from rlottie_python import LottieAnimation      # optional; see requirements

    data = _load_lottie(src)
    out.parent.mkdir(parents=True, exist_ok=True)
    # Keeps the .webp suffix: Pillow picks its encoder from the extension, so a
    # ".webp.tmp" name fails with "unknown file extension".
    tmp = out.with_name(out.stem + ".tmp" + out.suffix)
    anim = LottieAnimation.from_data(json.dumps(data))
    try:
        # Only resample when the clip is long enough to survive it. Ten of this
        # catalog's animations are a single 1/60 s frame (op=1); asking for 30fps
        # sampled them down to an EMPTY frame list and rlottie then died on
        # im_list[0]. Their native rate is already cheap, so leave them alone.
        opts = dict(width=size, height=size, lossless=False,
                    quality=quality, method=4)
        try:
            duration = float(anim.lottie_animation_get_duration())
        except Exception:      # noqa: BLE001 - a rate we cannot read is one we do not force
            duration = 0.0
        if duration * fps >= 1:
            opts["fps"] = fps
        # Written to a temp name and renamed: a half-written preview served to
        # the browser would cache a broken image against a content key that
        # never changes again.
        anim.save_animation(str(tmp), **opts)
    finally:
        anim.lottie_animation_destroy()
    os.replace(tmp, out)
    return out


def to_animated_tgs(src: Path, out: Path) -> Path:
    """Package a Lottie animation (.json or .tgs) into a valid 512x512 .tgs.

    NOTE: animated emoji are VECTOR (Lottie) only. Raster sources (GIF/MP4/WEBM)
    CANNOT become animated emoji -- convert those to *video* emoji instead. This
    function only validates/repackages an existing Lottie animation.
    """
    out.parent.mkdir(parents=True, exist_ok=True)
    lottie = _load_lottie(src)
    w, h = int(lottie.get("w", 0)), int(lottie.get("h", 0))
    if (w, h) != (TGS_SIZE, TGS_SIZE):
        # Rewriting the canvas is not a safe repair: a Lottie's positions,
        # anchors, animated transforms, masks and nested precompositions are all
        # expressed in canvas units, so scaling only the top-level layer
        # transform moves and clips the artwork. Reject instead of shipping a
        # broken animation.
        raise MediaError(
            f"{src.name}: animated emoji require a {TGS_SIZE}x{TGS_SIZE} Lottie "
            f"canvas, got {w}x{h}. Re-export the animation at "
            f"{TGS_SIZE}x{TGS_SIZE}.")
    data = json.dumps(lottie, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    # mtime=0 keeps the gzip output deterministic for stable content hashing.
    with open(out, "wb") as fh:
        with gzip.GzipFile(filename="", fileobj=fh, mode="wb", mtime=0) as gz:
            gz.write(data)
    validate_tgs(out)
    return out


def _all_layers(lottie: dict) -> list[dict]:
    """Every layer in a Lottie, including those inside precomposition assets.

    Scanning only ``lottie["layers"]`` misses most of them: the real rejected
    sticker kept its masked layers inside a precomp, so a top-level scan saw a
    single innocent precomp layer and nothing else.
    """
    layers = list(lottie.get("layers") or ())
    for asset in lottie.get("assets") or ():
        if isinstance(asset, dict):
            layers.extend(asset.get("layers") or ())
    return [x for x in layers if isinstance(x, dict)]


def validate_tgs(path: Path) -> None:
    """Raise MediaError unless a .tgs satisfies Telegram's animated contract.

    Checks the whole published contract, not just the byte cap: a .tgs is a
    GZIP package, the canvas must be 512x512, and the timeline must run at
    60 fps for at most 3 seconds. See core.telegram.org/stickers.
    """
    size = path.stat().st_size
    if size > TGS_MAX_BYTES:
        raise MediaError(f"{path.name}: TGS {size} > {TGS_MAX_BYTES} bytes")
    if path.read_bytes()[:2] != _GZIP_MAGIC:
        raise MediaError(f"{path.name}: TGS must be gzip-compressed Lottie")
    try:
        lottie = _load_lottie(path)
    except (OSError, ValueError) as exc:
        raise MediaError(f"{path.name}: not a valid TGS/Lottie ({exc})") from exc
    for key in ("v", "fr", "ip", "op", "layers"):
        if key not in lottie:
            raise MediaError(f"{path.name}: Lottie missing required key {key!r}")

    w, h = lottie.get("w"), lottie.get("h")
    if (w, h) != (TGS_SIZE, TGS_SIZE):
        raise MediaError(f"{path.name}: canvas {w}x{h}, expected "
                         f"{TGS_SIZE}x{TGS_SIZE}")
    try:
        fr = float(lottie["fr"])
        ip, op = float(lottie["ip"]), float(lottie["op"])
        frames = op - ip
    except (TypeError, ValueError) as exc:
        raise MediaError(f"{path.name}: non-numeric fr/ip/op ({exc})") from exc
    # NaN parses as a float and then defeats every comparison below: `nan <= 0`,
    # `nan > 60` and `nan > 3.0` are all False, so a Lottie with a NaN frame
    # rate or a NaN out-point sailed through this entire function and only
    # failed deep inside a publish. Infinity was caught by the range checks by
    # luck, not by design; both are rejected here for the same reason.
    for name, value in (("fr", fr), ("ip", ip), ("op", op)):
        if not math.isfinite(value):
            raise MediaError(f"{path.name}: {name} is {value}, not a finite number")
    if fr <= 0:
        raise MediaError(f"{path.name}: frame rate {fr} must be positive")
    if fr > TGS_FPS:
        raise MediaError(f"{path.name}: {fr} fps > {TGS_FPS} fps")
    duration = frames / fr
    if duration <= 0:
        raise MediaError(f"{path.name}: empty timeline (ip={lottie['ip']}, "
                         f"op={lottie['op']})")
    if duration > TGS_MAX_SECONDS + 0.01:
        raise MediaError(f"{path.name}: {duration:.2f}s > {TGS_MAX_SECONDS}s")

    # Telegram's UPLOADER refuses a subtract mask; its player shows one happily.
    # A sticker can therefore be live in a published pack for years and still be
    # rejected when you try to upload the same bytes -- verified by downloading
    # one from a live pack and sending it straight back, untouched. Without this
    # check the file passes every local test, enters the catalog, and only fails
    # deep inside a publish with "Bad Request: wrong file type", which says
    # nothing about which of the 200 items is at fault or why.
    for layer in _all_layers(lottie):
        for mask in layer.get("masksProperties") or ():
            if mask.get("mode") == "s":
                raise MediaError(
                    f"{path.name}: layer {layer.get('nm', '?')!r} uses a "
                    f"SUBTRACT mask, which Telegram refuses on upload "
                    f"(add masks are fine). Re-export without it.")
