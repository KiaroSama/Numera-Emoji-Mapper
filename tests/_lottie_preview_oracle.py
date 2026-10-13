"""Temporary pinned independent renderer oracle; retired only with native tooling parity."""
from __future__ import annotations

import ast
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import tests  # noqa: E402,F401 - install credential and non-loopback guard first
from PIL import Image  # noqa: E402
from emojikit.errors import MediaError  # noqa: E402

REVISION = "1a36a968b006236bd0665a1da65c541938c06061"
SOURCE_SHA = "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b"


def main() -> None:
    source = subprocess.run(
        ["git", "show", REVISION + ":emojikit/media_lottie.py"], cwd=ROOT,
        capture_output=True, check=True, timeout=5, stdin=subprocess.DEVNULL,
    ).stdout
    assert hashlib.sha256(source).hexdigest() == SOURCE_SHA
    names = {"_load_lottie", "lottie_still_webp", "lottie_preview_webp"}
    nodes = [node for node in ast.parse(source.decode("utf-8")).body
             if isinstance(node, ast.FunctionDef) and node.name in names]
    assert {node.name for node in nodes} == names
    namespace = {"Path": Path, "gzip": gzip, "io": io, "json": json,
                 "os": os, "MediaError": MediaError, "_GZIP_MAGIC": b"\x1f\x8b",
                 "TGS_MAX_UNPACKED": 8 * 1024 * 1024,
                 "PREVIEW_SIZE": 104, "PREVIEW_FPS": 15, "PREVIEW_QUALITY": 60}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), "pinned/media_lottie.py", "exec"), namespace)
    cases = []
    (ROOT / "logs/tmp").mkdir(parents=True, exist_ok=True)
    (ROOT / "logs/checkpoint").mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="lottie source oracle ", dir=ROOT / "logs/tmp") as folder:
        for name, path, still in [
            ("still", "tests/fixtures/lottie/red_circle_512.json", True),
            ("moving", "tests/fixtures/native/lottie-preview/moving.json", False),
            ("short", "tests/fixtures/native/lottie-preview/short.json", False),
        ]:
            output = Path(folder) / (name + ".webp")
            options = {"size": 52}
            if not still:
                options["fps"] = 15
            namespace["lottie_still_webp" if still else "lottie_preview_webp"](ROOT / path, output, **options)
            frames = []
            with Image.open(output) as image:
                for index in range(image.n_frames):
                    image.seek(index)
                    image.load()
                    rgba = image.convert("RGBA")
                    try:
                        frames.append({"rgba_sha256": hashlib.sha256(rgba.tobytes()).hexdigest(),
                                       "duration": image.info.get("duration")})
                    finally:
                        rgba.close()
                size = list(image.size)
            cases.append({"name": name, "input": path, "still": still, "size": 52, "fps": 15,
                          "input_sha256": hashlib.sha256((ROOT / path).read_bytes()).hexdigest(),
                          "expected": {"bytes": output.stat().st_size,
                                       "webp_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                                       "size": size, "frames": frames}})
    observed = {"source_revision": REVISION, "source_sha256": SOURCE_SHA,
                "Pillow": Image.__version__, "cases": cases}
    if "--generate" not in sys.argv:
        committed = json.loads((ROOT / "tests/fixtures/native/lottie-preview.json").read_text(encoding="utf-8"))
        assert committed["source_revision"] == REVISION
        assert committed["source_sha256"] == SOURCE_SHA
        assert len(committed["cases"]) == len(cases)
        for expected, actual in zip(committed["cases"], cases, strict=True):
            assert actual["input_sha256"] == expected["input_sha256"]
            assert actual["expected"]["size"] == expected["expected"]["size"]
            assert [frame["duration"] for frame in actual["expected"]["frames"]] == [
                frame["duration"] for frame in expected["expected"]["frames"]]
    destination = ROOT / ("tests/fixtures/native/lottie-preview.json" if "--generate" in sys.argv
                          else "logs/checkpoint/lottie-preview-source.json")
    destination.write_text(json.dumps(observed, indent=2) + "\n", encoding="utf-8")
    gallery = json.loads((ROOT / "tests/fixtures/native/gallery-codec.json").read_text(encoding="utf-8"))
    assert gallery["source_revision"] == REVISION
    assert gallery["source_sha256"]["emojikit/media_lottie.py"] == SOURCE_SHA
    with tempfile.TemporaryDirectory(prefix="gallery source oracle ", dir=ROOT / "logs/tmp") as folder:
        for index, case in enumerate(gallery["cases"]):
            if case["format"] != "animated":
                continue
            path = ROOT / case["input"]
            assert hashlib.sha256(path.read_bytes()).hexdigest() == case["input_sha256"]
            output = Path(folder) / f"{index}.webp"
            options = {"size": 88, "quality": 65}
            if not case["still"]:
                options["fps"] = 9
            namespace["lottie_still_webp" if case["still"] else "lottie_preview_webp"](path, output, **options)
            case["sha256"] = hashlib.sha256(output.read_bytes()).hexdigest()
            case["bytes"] = output.stat().st_size
    (ROOT / "logs/checkpoint/gallery-codec-source.json").write_text(json.dumps(gallery, indent=2) + "\n", encoding="utf-8")
    print(f"Pinned Lottie source oracle: {len(cases)} cases plus gallery; no target output used.")


if __name__ == "__main__":
    main()
