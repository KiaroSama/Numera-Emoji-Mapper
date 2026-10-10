"""Narrow codec/identity adapter for the native backend, never catalog or Bot API work."""
from __future__ import annotations

import json
import logging
import os
from pathlib import Path
import sys

from emojikit import identity, media
from emojikit.logsetup import record_exit_code, redact, setup_logging
from emojikit.packstate import write_json_atomic

log = logging.getLogger("media_bridge")


def _path(request: dict, field: str, *, exists: bool) -> Path:
    raw = request.get(field)
    if not isinstance(raw, str) or not raw:
        raise ValueError(f"{field} must be a non-empty path")
    path = Path(raw)
    if not path.is_absolute():
        raise ValueError(f"{field} must be absolute")
    if exists and (path.is_symlink() or not path.is_file()):
        raise ValueError(f"{field} must be a regular non-symlink file")
    return path


def execute(request: dict):
    if not isinstance(request, dict):
        raise ValueError("expected a media request object")
    operation = request.get("operation")
    if operation not in {"fingerprint", "compare", "same_image", "collision", "prepare", "preview", "usable", "dimensions", "image_info", "gallery_thumb", "convert", "animation_info"}:
        raise ValueError("unsupported media operation")
    source = _path(request, "source", exists=True)
    fmt = request.get("format") or media.detect_format(source)
    if fmt not in {"static", "video", "animated"} and operation != "convert":
        raise ValueError("unsupported media format")
    if operation in {"compare", "same_image"}:
        other = _path(request, "other", exists=True)
        if (operation == "compare" and source.stat().st_size == other.stat().st_size
                and source.read_bytes() == other.read_bytes()):
            return True
        return identity.same_image(source, other, fmt)
    if operation == "collision":
        key = request.get("key")
        if not isinstance(key, str) or not key:
            raise ValueError("collision requires a content key")
        return identity.collision_key(source, key)
    if operation == "gallery_thumb":
        output = _path(request, "output", exists=False)
        if output.exists() or output.is_symlink():
            raise ValueError("gallery output already exists")
        still = request.get("still", False)
        if not isinstance(still, bool):
            raise ValueError("gallery still must be boolean")
        if fmt == "animated":
            if still:
                media.lottie_still_webp(source, output, size=88, quality=65)
            else:
                media.lottie_preview_webp(source, output, size=88, fps=9, quality=65)
        elif fmt == "video":
            output.write_bytes(source.read_bytes())
        else:
            from PIL import Image
            with Image.open(source) as image:
                image = image.convert("RGBA")
                image.thumbnail((88, 88), Image.Resampling.LANCZOS)
                image.save(output, format="WEBP", quality=65, exact=True)
        return {"bytes": output.stat().st_size}
    if operation == "animation_info":
        from PIL import Image
        try:
            with Image.open(source) as image:
                return bool(getattr(image, "is_animated", False) and image.n_frames > 1)
        except (OSError, ValueError, RuntimeError):
            return False
    if operation in {"dimensions", "image_info"}:
        from PIL import Image
        with Image.open(source) as image:
            image.load()
            if operation == "image_info":
                return {"size": list(image.size), "mode": image.mode,
                        "blank": media.is_blank_image(image)}
            return list(image.size)
    if operation == "usable":
        if fmt == "static":
            from PIL import Image
            with Image.open(source) as image:
                return not media.is_blank_image(image)
        if fmt == "video":
            media.validate_video(source)
            return not media.is_blank_video(source)
        else:
            media.validate_tgs(source)
        return True
    if operation == "prepare":
        if not isinstance(request.get("reencode", False), bool):
            raise ValueError("reencode must be boolean")
        if request.get("reencode"):
            media.reencode_in_place(source, fmt)
        if "tint" in request:
            from emojikit.repaint import parse_tint, repaint_in_place
            if not isinstance(request["tint"], str):
                raise ValueError("tint must be #RRGGBB")
            if not repaint_in_place(source, fmt, parse_tint(request["tint"])):
                raise ValueError("requested tint could not be applied")
    if operation == "convert":
        output = _path(request, "output", exists=False)
        if output.exists() or output.is_symlink():
            raise ValueError("conversion output already exists")
        target = request.get("target_format")
        if target == "static":
            media.to_static_png(source, output)
        elif target == "video":
            media.to_video_webm(source, output)
        elif target == "animated":
            media.to_animated_tgs(source, output)
        else:
            raise ValueError("invalid target format")
        source, fmt = output, target
    if operation == "preview":
        key = request.get("key")
        size, fps, still = request.get("size", 104), request.get("fps", 15), request.get("still", False)
        if (not isinstance(key, str) or "/" in key or "\\" in key
                or isinstance(size, bool) or size not in {52, 72, 104}
                or isinstance(fps, bool) or not isinstance(fps, int) or not 1 <= fps <= 30
                or not isinstance(still, bool)):
            raise ValueError("invalid preview parameters")
        output = _path(request, "output", exists=False)
        if output.exists() or output.is_symlink():
            raise ValueError("preview output already exists")
        data = _render_preview(source, fmt, fps, still, size)
        with output.open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        return {"bytes": len(data)}
    key, phash = identity.fingerprint(source, fmt)
    return {"content_key": key, "phash": phash, "fmt": fmt,
            "extension": media.media_extension(source, fmt)}


def _render_preview(source: Path, fmt: str, fps: int, still: bool, size: int) -> bytes:
    import io
    import tempfile
    from PIL import Image
    from emojikit import video_decode

    if fmt == "animated":
        with tempfile.TemporaryDirectory(dir=source.parent, prefix="codec-preview-") as folder:
            output = Path(folder) / "render.webp"
            if still:
                media.lottie_still_webp(source, output, size=size)
            else:
                media.lottie_preview_webp(source, output, fps=fps, size=size)
            return output.read_bytes()
    if fmt == "video":
        command = [media.ffmpeg_path(), "-v", "error", *video_decode.decoder_args(source),
                   "-i", str(source), "-frames:v", "1" if still else "90", "-an", "-threads", "1",
                   "-vf", ("" if still else f"fps={fps},") + f"scale={size}:{size},format=rgba",
                   "-f", "rawvideo", "-"]
        raw = media._run(command, capture=True).stdout
        step = size * size * 4
        frames = [Image.frombytes("RGBA", (size, size), raw[i:i + step])
                  for i in range(0, len(raw) - step + 1, step)]
        if not frames:
            raise ValueError("no video preview frames decoded")
    else:
        with Image.open(source) as image:
            frame = image.convert("RGBA")
            frame.thumbnail((size, size))
        frames = [frame]
    try:
        buffer = io.BytesIO()
        if len(frames) == 1:
            frames[0].save(buffer, "WEBP", lossless=True, exact=True)
        else:
            frames[0].save(buffer, "WEBP", save_all=True, append_images=frames[1:],
                           duration=round(1000 / fps), loop=0, lossless=False,
                           quality=media.PREVIEW_QUALITY, method=4)
        return buffer.getvalue()
    finally:
        for frame in frames:
            frame.close()


def main(argv: list[str] | None = None) -> int:
    setup_logging("media_bridge", console_level=logging.ERROR)
    args = sys.argv[1:] if argv is None else argv
    if len(args) != 2:
        log.error("Usage: python -m emojikit.media_bridge REQUEST_JSON RESPONSE_JSON")
        return 2
    request_path, response_path = map(Path, args)
    try:
        if request_path.is_symlink() or not request_path.is_file() or request_path.stat().st_size > 65536:
            raise ValueError("request must be a regular JSON file under64KiB")
        if response_path.exists() or response_path.is_symlink():
            raise ValueError("response path already exists")
        request = json.loads(request_path.read_text(encoding="utf-8"))
        result = execute(request)
        write_json_atomic(response_path, {"ok": True, "result": result})
        return 0
    except (OSError, ValueError, RuntimeError) as exc:
        log.error("Media adapter failed: %s", redact(str(exc)))
        if not response_path.exists() and not response_path.is_symlink():
            write_json_atomic(response_path, {"ok": False, "error": redact(str(exc))})
        return 1


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
