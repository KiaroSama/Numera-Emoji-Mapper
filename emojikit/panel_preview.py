"""Small, cached panel previews; source media and publication bytes are read only."""
from __future__ import annotations

import hashlib
import io
import logging
import os
from pathlib import Path
import threading
from urllib.parse import parse_qs

from PIL import Image

from emojikit import media, video_decode

log = logging.getLogger("panel")

_locks: dict[str, threading.Lock] = {}
_guard = threading.Lock()
# A viewport can request a hundred uncached animations at once. Limit expensive
# raster/codec work while the independent HTTP save handlers remain responsive.
#
# The bound was a hard-coded 2, which on any real machine is the drip that made
# a cold tier arrive in visible chunks: measured here, one animation costs
# ~155 ms and one video poster ~613 ms, so 442 animations at two-at-a-time is
# ~34 s of rendering. Resource-aware instead, and still leaving most of the
# machine to the OS, the save handlers and whatever else the owner is running.
_renders = threading.BoundedSemaphore(max(2, min(6, (os.cpu_count() or 4) - 2)))


def parameters(query: str, maximum_fps: int) -> tuple[bool, int, int]:
    values = parse_qs(query, max_num_fields=4)
    fps = int(values.get("fps", [maximum_fps])[0])
    size = int(values.get("size", [104])[0])
    if size not in {52, 72, 104} or not 1 <= fps <= 30:
        raise ValueError("invalid preview size or frame rate")
    return values.get("still") == ["1"], min(fps, maximum_fps), size


def page_tiers(max_fps: int) -> dict:
    """The preview sizes and rates the page requests; the warm-up uses the same."""
    return {"full": {"size": 104, "fps": min(max_fps, 15)},
            "compact": {"size": 72, "fps": min(max_fps, 10)}}


def warm(view: list[dict], by_key: dict, db_path: Path, max_fps: int,
         stop: threading.Event | None = None) -> dict:
    """Render, in grid order, the previews the page is about to ask for.

    Every miss used to be paid at scroll time, one viewport at a time, behind
    the render bound -- which is what "the animations arrive in pieces" was.
    Warming in grid order means the top of the list is ready first and the rest
    lands before the owner scrolls that far. It renders exactly the tiers the
    page requests (`page_tiers`): the full tier first, then the compact one the
    page switches to below 75 % zoom, plus the compact still the holding tray
    shows for every excluded card. A static card in the grid is served from
    `/img/`, so its grid previews are never asked for and never rendered.
    Rendering is best effort: a failure here must never take the panel down,
    because the request path renders the same file again and reports its own error.
    """
    tiers = page_tiers(max_fps)
    counts = {"rendered": 0, "cached": 0, "failed": 0}

    def one(key: str, src: Path, still: bool, tier: dict) -> None:
        if cached_path(key, src, db_path, tier["fps"], still, tier["size"]) is not None:
            counts["cached"] += 1
            return
        try:
            preview_bytes(key, src, db_path, tier["fps"], still, tier["size"])
            counts["rendered"] += 1
        except Exception as exc:  # noqa: BLE001 - one bad file must not stop the warm-up
            counts["failed"] += 1
            log.debug("preview warm-up skipped %s (still=%s): %s", key, still, exc)

    cards = [(c, c.get("key"), by_key.get(c.get("key"))) for c in view
             if c.get("key") and not c.get("isLogo") and by_key.get(c.get("key")) is not None]
    for tier_name in ("full", "compact"):
        tier = tiers[tier_name]
        for card, key, src in cards:
            if stop is not None and stop.is_set():
                return counts
            moving = card.get("fmt") in ("animated", "video")
            wanted = [True, False] if moving else []
            if tier_name == "compact" and not card.get("included", True) and not moving:
                wanted = [True]          # the holding tray's still
            for still in wanted:
                one(key, Path(src), still, tier)
    return counts


def _check_key(key: str) -> None:
    # The key becomes part of a file name: catalog keys are "<kind>:<hex>", and
    # anything carrying a path separator is refused rather than resolved.
    if "/" in key or "\\" in key:
        raise ValueError(f"invalid preview key: {key!r}")


def _dest(cache: Path, key: str, src: Path, fps: int, still: bool, size: int) -> Path:
    name = hashlib.sha256(key.encode("utf-8")).hexdigest()
    video = src.suffix.lower() == ".webm"
    # A moving video preview gets its own name: older runs wrote a single frame
    # under `@<fps>-<size>` for a video, and that file must not be served as
    # the animation.
    tier = "still" if still else (f"v{fps}" if video else str(fps))
    return cache / f"{name}@{tier}-{size}.webp"


def cached_path(key: str, src: Path, db_path: Path, fps: int,
                still: bool = False, size: int = 104) -> Path | None:
    """The cached preview file if it exists -- checked, never read."""
    _check_key(key)
    cache = db_path.parent / "preview"
    if size == 104 and src.suffix.lower() == ".tgs":
        legacy = cache / f"{key.replace(':', '_')}@{'still' if still else fps}.webp"
        if legacy.is_file():
            return legacy
    dest = _dest(cache, key, src, fps, still, size)
    return dest if dest.is_file() else None


def preview_bytes(key: str, src: Path, db_path: Path, fps: int,
                  still: bool = False, size: int = 104) -> bytes:
    hit = cached_path(key, src, db_path, fps, still, size)
    if hit is not None:
        return hit.read_bytes()
    cache = db_path.parent / "preview"
    dest = _dest(cache, key, src, fps, still, size)
    video = src.suffix.lower() == ".webm"
    with _guard:
        lock = _locks.setdefault(str(dest), threading.Lock())
    with lock:
        if not dest.is_file():
            with _renders:
                if src.suffix.lower() == ".tgs":
                    if still:
                        media.lottie_still_webp(src, dest, size=size)
                    else:
                        media.lottie_preview_webp(src, dest, fps=fps, size=size)
                else:
                    cache.mkdir(parents=True, exist_ok=True)
                    if video and not still:
                        _write(dest, _video_animation(src, fps, size))
                        return dest.read_bytes()
                    if video:
                        cmd = [media.ffmpeg_path(), "-v", "error", *video_decode.decoder_args(src),
                               "-i", str(src), "-frames:v", "1", "-an", "-threads", "1",
                               "-vf", f"scale={size}:{size},format=rgba", "-f", "rawvideo", "-"]
                        raw = media._run(cmd, capture=True).stdout
                        frame = Image.frombytes("RGBA", (size, size), raw)
                    else:
                        with Image.open(src) as image:
                            frame = image.convert("RGBA")
                            frame.thumbnail((size, size))
                    with frame:
                        buf = io.BytesIO()
                        frame.save(buf, "WEBP", lossless=True, exact=True)
                    _write(dest, buf.getvalue())
        return dest.read_bytes()


def _write(dest: Path, data: bytes) -> None:
    tmp = dest.with_suffix(".tmp.webp")
    tmp.write_bytes(data)
    tmp.replace(dest)


# 90 frames = 3 s at 30 fps, Telegram's limit for a video emoji, so a valid
# sticker is never cut short while a broken one cannot run the decode forever.
_MAX_VIDEO_FRAMES = 90


def _video_animation(src: Path, fps: int, size: int) -> bytes:
    """A video emoji as an animated WebP: the same decode path as its still.

    A grid of `<video>` elements costs a media player per card, and creating or
    tearing one down was the 60-140 ms frames left after the grid went virtual.
    An animated image decodes off the main thread and joins the animation budget.
    The decoder is the one `video_decode` chooses, so the alpha layer survives.
    """
    cmd = [media.ffmpeg_path(), "-v", "error", *video_decode.decoder_args(src),
           "-i", str(src), "-frames:v", str(_MAX_VIDEO_FRAMES), "-an", "-threads", "1",
           "-vf", f"fps={fps},scale={size}:{size},format=rgba", "-f", "rawvideo", "-"]
    raw = media._run(cmd, capture=True).stdout
    step = size * size * 4
    frames = [Image.frombytes("RGBA", (size, size), raw[i:i + step])
              for i in range(0, len(raw) - step + 1, step)]
    if not frames:
        raise media.MediaError(f"{src.name}: no frames decoded")
    buf = io.BytesIO()
    if len(frames) == 1:
        frames[0].save(buf, "WEBP", lossless=True, exact=True)
    else:
        frames[0].save(buf, "WEBP", save_all=True, append_images=frames[1:],
                       duration=round(1000 / fps), loop=0, lossless=False,
                       quality=media.PREVIEW_QUALITY, method=4)
    return buf.getvalue()
