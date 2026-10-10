"""Shared real VP9 inputs; importing codecs never imports a Python application."""
from pathlib import Path

from emojikit import media

RED = bytes((220, 20, 20, 255)) * (100 * 100)
BLUE = bytes((20, 20, 220, 255)) * (100 * 100)
TEST_ROOT = Path(__file__).resolve().parents[1] / "logs"


def encode(directory: Path, name: str, frames: list[bytes], *, pts: str = "") -> Path:
    source = directory / (name + ".rgba")
    source.write_bytes(b"".join(frames))
    out = directory / (name + ".webm")
    # Preserve the intended VFR timestamps AND a real final-frame duration.
    # FFmpeg 7 otherwise writes duration == final PTS (zero terminal duration),
    # correctly rejected by the production fail-closed timeline reader.
    timing = ["-vf", "settb=1/1000,setpts=" + pts,
              "-fps_mode", "passthrough", "-enc_time_base", "1/1000",
              "-bsf:v", "setts=duration=33"] if pts else []
    media._run([media.ffmpeg_path(), "-y", "-v", "error", "-f", "rawvideo",
                "-pixel_format", "rgba", "-video_size", "100x100", "-framerate", "30",
                "-i", str(source), *timing, "-c:v", "libvpx-vp9", "-threads", "1",
                "-cpu-used", "8", "-lossless", "1", "-pix_fmt", "yuva420p",
                "-auto-alt-ref", "0", str(out)], capture=True, timeout=30)
    source.unlink()
    return out
