"""Media handling for Telegram custom emoji: detect, hash and convert.

Telegram custom emoji come in three formats (see https://core.telegram.org/stickers
and the Bot API ``InputSticker.format`` field):

============  =========  ===========================================  ==========
format        extension  nature                                       hard caps
============  =========  ===========================================  ==========
``static``    .png/.webp 100x100 image (RGBA)                          -
``animated``  .tgs       gzip-compressed Lottie (vector) animation     <=64 KB
``video``     .webm      VP9 video, 100x100, <=3 s, 30 fps, no audio    <=256 KB
============  =========  ===========================================  ==========

This module provides:

* :func:`detect_format` -- decide static/animated/video from magic bytes.
* :func:`to_static_png` / :func:`to_video_webm` / :func:`to_animated_tgs` --
  conversion ("build from scratch") into each format.
* validation helpers backed by ffprobe.

Identifying a file -- content keys, perceptual hashes, "is this the
same picture?" -- is :mod:`emojikit.identity`, which imports this module
and never the other way round.

Raster/vector image decoding uses Pillow (+ resvg for SVG). Video and
animated-video work requires ``ffmpeg``/``ffprobe`` on PATH; every such child
runs under a wall-clock limit (``EMOJI_FFMPEG_TIMEOUT``, default 300 s) so one
corrupt file cannot stall an ingest or publish run.
"""

from __future__ import annotations

import gzip
import io
import json
import logging
import os
import shutil
import signal
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from PIL import Image

# Safe at module level: video_decode reaches back into this module only
# from inside its functions, so there is no import-time cycle.
from emojikit import video_decode

log = logging.getLogger("emojikit.media")

SIZE = 100                       # static/video custom emoji canvas (px)
WEBM_MAX_BYTES = 256 * 1024      # video emoji hard cap
WEBM_MAX_SECONDS = 3.0
WEBM_FPS = 30

# Every ffmpeg/ffprobe child runs under a wall limit. A truncated container or a
# wedged decoder makes the tool wait on its input forever, and an unbounded child
# stalls the whole ingest/publish run with no output and no error.
FFMPEG_TIMEOUT = 300             # seconds per child; a 3 s emoji encode is <1 s
_KILL_GRACE = 5                  # seconds allowed to kill and reap a stuck child

# Shared "is there anything to see?" rule, also used by the publisher: alpha at
# or below VISIBLE_ALPHA is invisible in practice, and a handful of stray pixels
# is noise, not artwork.
VISIBLE_ALPHA = 10
BLANK_MAX_VISIBLE = 8

# Container/codec magic bytes used for fast format sniffing.
_EBML_MAGIC = b"\x1a\x45\xdf\xa3"      # Matroska/WebM
_PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
_RIFF_MAGIC = b"RIFF"
_GIF_MAGIC = (b"GIF87a", b"GIF89a")

RASTER_EXTS = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".apng"}


# Re-exported, not defined here: `video_decode` raises them and this module
# imports it, so they live in a leaf module that cycles through nothing. Every
# existing `media.MediaError` caller keeps working unchanged.
from emojikit.errors import MediaError, UndecodableVideo  # noqa: E402,F401

# Lottie / .tgs handling lives in emojikit.media_lottie; re-exported so every
# existing ``media.<name>`` caller keeps working.
from emojikit.media_lottie import (  # noqa: E402,F401
    _GZIP_MAGIC, PREVIEW_FPS, PREVIEW_QUALITY, PREVIEW_SIZE, TGS_FPS,
    TGS_MAX_BYTES, TGS_MAX_SECONDS, TGS_MAX_UNPACKED, TGS_SIZE, _load_lottie,
    lottie_preview_webp, lottie_still_webp, to_animated_tgs, validate_tgs)


# --------------------------------------------------------------------------- #
# Tool discovery
# --------------------------------------------------------------------------- #
def ffmpeg_path() -> str:
    exe = shutil.which("ffmpeg")
    if not exe:
        raise MediaError("ffmpeg not found on PATH (required for video emoji).")
    return exe


def ffprobe_path() -> str:
    exe = shutil.which("ffprobe")
    if not exe:
        raise MediaError("ffprobe not found on PATH (required for video emoji).")
    return exe


def ff_timeout() -> float:
    """Per-child ffmpeg/ffprobe wall limit; override with EMOJI_FFMPEG_TIMEOUT."""
    # Lazy import: emojikit stays importable without the CLI layer (and this is
    # called once per child process, so the sys.modules lookup is free).
    from emojikit.build_pack import safe_int_env
    return safe_int_env("EMOJI_FFMPEG_TIMEOUT", FFMPEG_TIMEOUT, minimum=1)


def _kill_tree(proc: subprocess.Popen) -> None:
    """Kill a stuck child *and its descendants*, then reap it.

    Killing only the direct child can leave a grandchild holding the output
    pipes open, so the follow-up read blocks for exactly as long as the hang we
    are trying to bound.
    """
    try:
        if os.name == "nt":
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)],
                           capture_output=True, check=False, timeout=_KILL_GRACE)
        else:
            # start_new_session below makes the child its own group leader, so
            # this kills its whole tree without touching our own process group.
            os.killpg(proc.pid, signal.SIGKILL)
    except (OSError, subprocess.SubprocessError):
        pass                              # already gone, or not ours to signal
    try:
        proc.kill()
        proc.communicate(timeout=_KILL_GRACE)
    except (OSError, subprocess.SubprocessError):
        pass


def _run(cmd: list[str], *, capture: bool = False,
         timeout: float | None = None) -> subprocess.CompletedProcess:
    """Run one ffmpeg/ffprobe child under a finite wall limit.

    A hang is reported as :class:`MediaError` like any other conversion failure,
    so a single corrupt file is skipped instead of freezing the run. Non-zero
    exits keep raising ``CalledProcessError`` as before.
    """
    limit = ff_timeout() if timeout is None else timeout
    log.debug("exec (timeout %ss): %s", limit, " ".join(cmd))
    pipe = subprocess.PIPE if capture else None
    # POSIX: own session so _kill_tree can signal the group. Windows uses
    # taskkill /T instead, which needs no creation flag (and setting one would
    # stop Ctrl+C from reaching the child).
    extra = {} if os.name == "nt" else {"start_new_session": True}
    proc = subprocess.Popen(cmd, stdout=pipe, stderr=pipe, **extra)
    try:
        out, err = proc.communicate(timeout=limit)
    except subprocess.TimeoutExpired as exc:
        _kill_tree(proc)
        raise MediaError(
            f"{Path(cmd[0]).name} timed out after {limit}s") from exc
    except BaseException:                 # Ctrl+C must not orphan the child
        _kill_tree(proc)
        raise
    if proc.returncode != 0:
        raise subprocess.CalledProcessError(proc.returncode, cmd, out, err)
    return subprocess.CompletedProcess(cmd, proc.returncode, out, err)


# --------------------------------------------------------------------------- #
# Format detection
# --------------------------------------------------------------------------- #
def detect_format_bytes(head: bytes) -> str:
    """Return 'static' | 'animated' | 'video' | 'unknown' from leading bytes."""
    if head.startswith(_EBML_MAGIC):
        return "video"                       # .webm
    if head.startswith(_GZIP_MAGIC):
        return "animated"                    # .tgs (gzip-compressed Lottie)
    if head.startswith(_PNG_MAGIC) or head.startswith(_GIF_MAGIC):
        return "static"
    if head.startswith(_RIFF_MAGIC) and head[8:12] == b"WEBP":
        return "static"                      # static .webp sticker
    return "unknown"


def detect_format(path: Path) -> str:
    """Detect the Telegram emoji format of a file from its content."""
    with open(path, "rb") as fh:
        head = fh.read(16)
    fmt = detect_format_bytes(head)
    if fmt != "unknown":
        return fmt
    # Fall back to extension hints for ambiguous local sources.
    ext = path.suffix.lower()
    if ext == ".tgs":
        return "animated"
    if ext in {".webm", ".mp4", ".mov", ".mkv", ".m4v", ".avi"}:
        return "video"
    if ext in RASTER_EXTS or ext == ".svg":
        return "static"
    return "unknown"


def telegram_sticker_format(sticker: dict) -> str:
    """Map a Bot API Sticker object to an emoji format using is_animated/is_video."""
    if sticker.get("is_animated"):
        return "animated"
    if sticker.get("is_video"):
        return "video"
    return "static"


def is_repaintable(sticker: dict) -> bool:
    """True when Telegram REPAINTS this emoji instead of showing its own colours.

    The flag lives on the **Sticker**, not on the StickerSet: `getStickerSet`
    on a set whose every sticker is repaintable still answers None at the set
    level (measured on `TopicIcons`: 160/160 stickers True, set field None), so
    reading the set is exactly how this gets missed.

    Such an emoji carries no colour of its own -- the client paints it with the
    text or accent colour -- so the stored asset is typically flat black.
    Republished into a set WITHOUT the flag it arrives black, which is not a
    conversion fault and no re-encoding fixes it. The flag cannot be added
    afterwards either: the Bot API exposes it only on the Sticker object and in
    createNewStickerSet, with no setter, and it is a whole-set property.
    """
    return bool(sticker.get("needs_repainting"))


def ext_for_format(fmt: str) -> str:
    return {"static": ".png", "animated": ".tgs", "video": ".webm"}.get(fmt, ".bin")


def media_extension(path: Path, fmt: str) -> str:
    """Real file extension for a media file, refining static into PNG vs WEBP.

    Telegram static stickers are usually WEBP; using the correct extension keeps
    the upload MIME type consistent with the actual bytes.
    """
    if fmt == "video":
        return ".webm"
    if fmt == "animated":
        return ".tgs"
    with open(path, "rb") as fh:
        head = fh.read(16)
    if head.startswith(_PNG_MAGIC):
        return ".png"
    if head.startswith(_RIFF_MAGIC) and head[8:12] == b"WEBP":
        return ".webp"
    if head.startswith(_GIF_MAGIC):
        return ".gif"
    return ".png"


# --------------------------------------------------------------------------- #
# Static image fitting (shared with the legacy make_emoji_pngs pipeline)
# --------------------------------------------------------------------------- #
def _trim(img: Image.Image) -> Image.Image:
    if img.mode != "RGBA":
        img = img.convert("RGBA")
    bbox = img.split()[3].getbbox()
    return img.crop(bbox) if bbox else img


def fit_100(img: Image.Image) -> Image.Image:
    """Trim transparent borders and center the image on a 100x100 RGBA canvas.

    THE ONE fitting implementation. ``make_emoji_pngs`` and the coin providers
    carried byte-identical copies, so the bug below had to be found three times
    to be fixed once; they call this now.
    """
    img = _trim(img)
    w, h = img.size
    if w == 0 or h == 0:
        return Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    scale = min(SIZE / w, SIZE / h)
    nw, nh = max(1, round(w * scale)), max(1, round(h * scale))
    img = img.resize((nw, nh), Image.LANCZOS)
    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    # NO MASK. `paste(img, pos, img)` makes img its own mask, which COMPOSITES
    # it over the canvas -- and the canvas is transparent black, so every pixel
    # came back multiplied by its own alpha twice: colour once and alpha again.
    # A half-opaque red (255, 0, 0, 128) fitted to (128, 0, 0, 64), i.e. a
    # quarter-opaque dark red, and only fully opaque art survived unharmed,
    # which is why this stood for so long. The destination region is fully
    # transparent, so the correct operation is a verbatim RGBA copy: it keeps
    # unassociated colour, real opacity, and the partial alpha LANCZOS leaves
    # along an antialiased edge.
    canvas.paste(img, ((SIZE - nw) // 2, (SIZE - nh) // 2))
    return canvas


def _load_image(src: Path) -> Image.Image:
    """Decode a raster or SVG source into an RGBA Pillow image."""
    if src.suffix.lower() == ".svg":
        # The SVG rasterizer is only needed for SVG; import lazily.
        from emojikit.make_emoji_pngs import _render_svg  # type: ignore
        img = _render_svg(src)
        if img is None:
            raise MediaError(f"failed to render SVG: {src.name}")
        return img
    return Image.open(src).convert("RGBA")


def is_blank_image(img: Image.Image) -> bool:
    """True if an image has no meaningful visible pixels.

    Shared rule so every producer and the publisher agree on what "blank" means.
    """
    alpha = img.convert("RGBA").split()[3]
    if alpha.getbbox() is None:
        return True
    # histogram() counts in C; the per-pixel generator this replaced walked
    # 10 000 Python iterations per image, and this runs on every conversion AND
    # on every static item of every publish. Bucket i holds the number of pixels
    # with alpha == i, so summing from VISIBLE_ALPHA + 1 upwards is exactly
    # "how many pixels are more opaque than the visibility floor".
    return sum(alpha.histogram()[VISIBLE_ALPHA + 1:]) <= BLANK_MAX_VISIBLE


def to_static_png(src: Path, out: Path) -> Path:
    """Convert any supported image into a 100x100 transparent PNG.

    Raises MediaError if the result would be blank: a transparent emoji is
    invisible forever, and ingesting one pollutes the catalog with an item that
    can never be used but still occupies one of the 200 slots in a pack.
    """
    out.parent.mkdir(parents=True, exist_ok=True)
    img = fit_100(_load_image(src))
    if is_blank_image(img):
        raise MediaError(f"{src.name}: image is blank (no visible pixels)")
    img.save(out, format="PNG", optimize=True)
    return out


# --------------------------------------------------------------------------- #
# Video (WEBM/VP9) conversion + validation
# --------------------------------------------------------------------------- #
_VF = (
    f"fps={WEBM_FPS},scale=w={SIZE}:h={SIZE}:force_original_aspect_ratio=decrease"
    f":flags=lanczos,format=rgba,pad={SIZE}:{SIZE}:(ow-iw)/2:(oh-ih)/2"
    f":color=0x00000000,format=yuva420p"
)


def to_video_webm(src: Path, out: Path, *, max_bytes: int = WEBM_MAX_BYTES) -> Path:
    """Encode any animation/video/image into a Telegram-compliant VP9 WEBM emoji.

    Output is 100x100, <=3 s, 30 fps, no audio, transparent-padded, VP9 with
    alpha. CRF is escalated until the file fits ``max_bytes``.
    """
    ff = ffmpeg_path()
    out.parent.mkdir(parents=True, exist_ok=True)
    # VP9 stores alpha as a SEPARATE WebM layer, and ffmpeg's default vp9
    # decoder drops it silently -- the filter chain then never sees an alpha
    # channel and the transparent-pad colour lands on an opaque frame. It
    # flattened a cue-ball emoji to a black square before anyone noticed.
    #
    # The choice used to be `suffix == ".webm"`, which is wrong twice: it forces
    # a VP9 decoder onto a VP8 WebM, and it skips a download saved without an
    # extension, which is exactly how a transparent source reaches this path.
    # The container knows its own codec, so ask it.
    decoder = video_decode.decoder_args(src)
    last_size = -1
    for crf in (32, 40, 48, 56, 63):
        cmd = [
            ff, "-y", "-t", str(WEBM_MAX_SECONDS), *decoder, "-i", str(src),
            "-an", "-vf", _VF,
            "-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p",
            "-b:v", "0", "-crf", str(crf),
            "-auto-alt-ref", "0", "-deadline", "good", "-cpu-used", "2",
            str(out),
        ]
        _run(cmd, capture=True)
        last_size = out.stat().st_size
        log.debug("webm crf=%d -> %d bytes", crf, last_size)
        if last_size <= max_bytes:
            return out
    raise MediaError(
        f"could not fit {src.name} under {max_bytes} bytes (got {last_size}).")


@dataclass
class VideoInfo:
    width: int
    height: int
    duration: float
    codec: str
    fps: float = 0.0
    has_audio: bool = False
    container: str = ""


def _fps(rate: str) -> float:
    """Parse an ffprobe rational frame rate ('30/1', '60000/1001')."""
    try:
        num, _, den = rate.partition("/")
        return float(num) / float(den or 1)
    except (TypeError, ValueError, ZeroDivisionError):
        return 0.0


def probe_video(path: Path) -> VideoInfo:
    """Read the properties Telegram actually constrains, via ffprobe.

    Selecting only ``v:0`` hides the audio stream, so an .webm carrying audio
    used to validate cleanly; frame rate and container were never read at all.
    """
    ff = ffprobe_path()
    cmd = [ff, "-v", "error",
           "-show_entries",
           "stream=index,codec_type,codec_name,width,height,avg_frame_rate"
           ":format=duration,format_name",
           "-of", "json", str(path)]
    res = _run(cmd, capture=True)
    data = json.loads(res.stdout.decode("utf-8", "replace"))
    streams = data.get("streams") or []
    video = next((s for s in streams if s.get("codec_type") == "video"), {})
    has_audio = any(s.get("codec_type") == "audio" for s in streams)
    fmt = data.get("format", {})
    return VideoInfo(
        width=int(video.get("width", 0) or 0),
        height=int(video.get("height", 0) or 0),
        duration=float(fmt.get("duration", 0) or 0),
        codec=str(video.get("codec_name", "")),
        fps=_fps(str(video.get("avg_frame_rate", "0/1"))),
        has_audio=has_audio,
        container=str(fmt.get("format_name", "")),
    )


def validate_video(path: Path) -> None:
    """Raise MediaError if a WEBM does not meet Telegram video-emoji rules.

    Contract: .WEBM container, VP9, no audio stream, exactly 100x100, positive
    duration <= 3 s, <= 30 fps, <= 256 KB. See core.telegram.org/stickers.
    """
    info = probe_video(path)
    size = path.stat().st_size
    problems = []
    if (info.width, info.height) != (SIZE, SIZE):
        problems.append(f"dimensions {info.width}x{info.height} != {SIZE}x{SIZE}")
    if info.duration <= 0:
        problems.append("duration is zero or unknown")
    elif info.duration > WEBM_MAX_SECONDS + 0.05:
        problems.append(f"duration {info.duration:.2f}s > {WEBM_MAX_SECONDS}s")
    if info.codec != "vp9":
        problems.append(f"codec {info.codec!r} != 'vp9'")
    if info.has_audio:
        problems.append("contains an audio stream (video emoji must have none)")
    if info.fps > WEBM_FPS + 0.01:
        problems.append(f"{info.fps:.2f} fps > {WEBM_FPS} fps")
    if "webm" not in info.container.split(","):
        problems.append(f"container {info.container!r} is not webm")
    if size > WEBM_MAX_BYTES:
        problems.append(f"size {size} > {WEBM_MAX_BYTES} bytes")
    if problems:
        raise MediaError(f"{path.name}: " + "; ".join(problems))


# --------------------------------------------------------------------------- #
# Republishing our own encoding
# --------------------------------------------------------------------------- #
def reencode_in_place(path: Path, fmt: str) -> bool:
    """Rewrite ``path`` so its BYTES differ but its picture does not.

    Media pulled from someone else's pack used to be stored and re-uploaded
    byte-for-byte, so the sticker we published was a bit-identical clone of
    theirs. Owner rule: republish our own encoding of the same picture.

    Every branch is pixel-exact, never a lossy re-compress:

    * static  -- decode and re-save WEBP **lossless**. The decoded pixels are
      whatever the source decoded to, lossy or not; encoding them losslessly
      cannot move them.
    * animated -- a .tgs is gzipped Lottie JSON. Re-serialise and re-gzip: the
      animation is the JSON, and the JSON is unchanged.
    * video   -- remux with ``-c copy``. The encoded stream is copied through
      untouched; only the container framing is rewritten.

    Returns True when the file was rewritten. Failure is not fatal to the
    caller: a sticker we could not re-encode is still better ingested as-is
    than dropped, so this reports rather than raises -- but the caller must
    compute the content key AFTERWARDS either way, since the bytes moved.
    """
    before = path.read_bytes()
    try:
        if fmt == "static":
            with Image.open(io.BytesIO(before)) as im:
                rgba = im.convert("RGBA")
            buf = io.BytesIO()
            # exact=True or libwebp rewrites the RGB under fully transparent
            # pixels to compress better. "Lossless" only promises the VISIBLE
            # result; without this the file round-trips to different pixel
            # values, which a real .webp sticker showed and a synthetic
            # fully-opaque fixture never would.
            rgba.save(buf, format="WEBP", lossless=True, quality=100,
                      method=6, exact=True)
            out = buf.getvalue()
        elif fmt == "animated":
            data = _load_lottie(path)
            raw = json.dumps(data, separators=(",", ":")).encode("utf-8")
            buf = io.BytesIO()
            # mtime=0 so the same animation always re-gzips to the same bytes:
            # a timestamp in the header would make ingest non-deterministic and
            # every re-run would look like a different file.
            with gzip.GzipFile(fileobj=buf, mode="wb", mtime=0) as gz:
                gz.write(raw)
            out = buf.getvalue()
        elif fmt == "video":
            with tempfile.TemporaryDirectory() as td:
                dst = Path(td) / "remux.webm"
                _run([ffmpeg_path(), "-v", "error", "-y", "-i", str(path),
                      "-c", "copy", str(dst)])
                # A remux recomputes the container duration FROM THE PACKETS,
                # and some published stickers under-declare their own length: a
                # real one served by Telegram carried a 3.000 s header over
                # 3.916 s of frames. Telegram's uploader reads the header, so
                # the original passed and our truthful rewrite was refused with
                # STICKER_VIDEO_LONG. Same trade-off as the size cap below --
                # a byte-clone beats an upload that cannot happen.
                if probe_video(dst).duration > WEBM_MAX_SECONDS:
                    log.warning("re-encode of %s would declare %.3fs, over the "
                                "%.1fs cap (the source under-declares its own "
                                "length); keeping the original",
                                path.name, probe_video(dst).duration,
                                WEBM_MAX_SECONDS)
                    return False
                out = dst.read_bytes()
        else:
            return False
    except Exception as exc:  # noqa: BLE001 - ingest must survive one bad file
        log.warning("re-encode skipped for %s (%s): %s", path.name, fmt, exc)
        return False

    if out == before:
        # Nothing gained, and rewriting would only churn the file.
        return False
    # Lossless can GROW a file, and Telegram's per-format caps are hard: a .tgs
    # measured 64 139 bytes after re-encoding against a 65 536 cap, so a source
    # already near the limit can cross it. Publishing a byte-clone is a lesser
    # failure than an upload Telegram rejects, so the original wins here.
    cap = {"animated": TGS_MAX_BYTES, "video": WEBM_MAX_BYTES}.get(fmt)
    if cap is not None and len(out) > cap:
        log.warning("re-encode of %s would be %d bytes, over the %s cap of %d; "
                    "keeping the original", path.name, len(out), fmt, cap)
        return False
    path.write_bytes(out)
    return True
