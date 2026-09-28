"""Is a media file worth uploading? Blank-media checks before a publish.

Split out of `build_collection.py` (file-size limit); it re-exports these names.
"""

from __future__ import annotations

from pathlib import Path

from emojikit import media
from emojikit.collection_state import _static_is_blank


_FRAME_BYTES = 64 * 64 * 4          # one sampled RGBA frame


def _video_is_blank(path: Path) -> bool:
    """True only when EVERY sampled frame of a video is effectively empty.

    The first frame alone is not evidence: any animation that fades in, or
    simply starts on an empty canvas, has a transparent frame 0 -- and the
    verdict is written to ``skipped``, so that emoji is never published again.
    One ffmpeg pass samples the whole (<=3 s) clip.
    """
    cmd = [media.ffmpeg_path(), "-v", "error", "-t", str(media.WEBM_MAX_SECONDS),
           "-i", str(path), "-an", "-vf", "fps=4,scale=64:64,format=rgba",
           "-f", "rawvideo", "-"]
    # Bounded child, like every other ffmpeg call in the project: media._run
    # enforces the wall limit and kills the whole process tree on timeout. A
    # bare subprocess.run had no timeout at all, so one corrupt clip could
    # freeze the publish indefinitely -- exactly what that runner exists for.
    raw = media._run(cmd, capture=True).stdout
    if len(raw) < _FRAME_BYTES:
        return False                # nothing decoded: let the upload decide
    for start in range(0, len(raw) - _FRAME_BYTES + 1, _FRAME_BYTES):
        alpha = raw[start + 3:start + _FRAME_BYTES:4]
        # media's named thresholds, not a third copy of 10/8: this is the
        # same "is it blank?" rule, applied to a raw frame instead of a
        # decoded image, and a literal drifting here would disagree with
        # every other producer about what ships.
        if sum(v > media.VISIBLE_ALPHA for v in alpha) > media.BLANK_MAX_VISIBLE:
            return False
    return True


def _media_ok(path: Path, fmt: str) -> bool:
    """False if the media is effectively blank (guards against blank emoji)."""
    if fmt == "static":
        return not _static_is_blank(path)
    if fmt == "video":
        try:
            return not _video_is_blank(path)
        except Exception:  # noqa: BLE001 - probing failed; let the upload decide
            return True
    return True  # animated (.tgs) validity is enforced at creation time
