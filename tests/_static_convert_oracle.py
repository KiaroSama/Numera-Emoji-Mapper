"""Temporary pinned source oracle during native migration; never target-derived."""
from __future__ import annotations

import ast
import hashlib
import json
import io
from pathlib import Path
import subprocess
import tempfile

from PIL import Image

import tests  # noqa: F401 - install credential/network guard before source execution

ROOT = Path(__file__).resolve().parents[1]
REVISION = "1a36a968b006236bd0665a1da65c541938c06061"
SOURCE_SHA = "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"


def main() -> None:
    source = subprocess.run(
        ["git", "show", REVISION + ":emojikit/media.py"], cwd=ROOT,
        capture_output=True, check=True, timeout=5, stdin=subprocess.DEVNULL,
    ).stdout
    assert hashlib.sha256(source).hexdigest() == SOURCE_SHA
    names = {"_trim", "fit_100", "render_svg", "_load_image", "is_blank_image", "to_static_png"}
    tree = ast.parse(source.decode("utf-8"))
    nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
    assert {node.name for node in nodes} == names
    namespace = {"Image": Image, "SIZE": 100, "VISIBLE_ALPHA": 10,
                 "BLANK_MAX_VISIBLE": 8, "Path": Path, "MediaError": RuntimeError, "io": io}
    # Only pinned source AST is executable here; the target is never imported.
    exec(compile(ast.Module(body=nodes, type_ignores=[]), "pinned/media.py", "exec"), namespace)
    original = json.loads((ROOT / "tests/fixtures/native/static-convert.json").read_text(encoding="utf-8"))
    cases = []
    (ROOT / "logs/tmp").mkdir(parents=True, exist_ok=True)
    (ROOT / "logs/checkpoint").mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="png source oracle ", dir=ROOT / "logs/tmp") as folder:
        for case in original["cases"]:
            path = ROOT / case["input"]
            assert hashlib.sha256(path.read_bytes()).hexdigest() == case["input_sha256"]
            output = Path(folder) / (case["name"] + ".png")
            observed = {key: case[key] for key in ("name", "input", "input_sha256")}
            try:
                namespace["to_static_png"](path, output)
            except RuntimeError as error:
                assert str(error) == case["error"]
                assert not output.exists()
                observed["error"] = str(error)
            else:
                with Image.open(output) as image:
                    rgba = hashlib.sha256(image.convert("RGBA").tobytes()).hexdigest()
                    assert rgba == case["expected"]["rgba_sha256"]
                    observed["expected"] = {"size": list(image.size), "mode": image.mode,
                        "bytes": output.stat().st_size, "png_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                        "rgba_sha256": rgba}
            cases.append(observed)
    original["cases"] = cases
    original["Pillow"] = Image.__version__
    destination = ROOT / "logs/checkpoint/static-convert-source.json"
    destination.write_text(json.dumps(original, indent=2) + "\n", encoding="utf-8")
    print(f"Pinned source conversion oracle: {len(cases)} cases; pixels and refusals unchanged.")
    provider_images(namespace)


def provider_images(namespace: dict) -> None:
    source = subprocess.run(
        ["git", "show", REVISION + ":coins/_paprika_api.py"], cwd=ROOT,
        capture_output=True, check=True, timeout=5, stdin=subprocess.DEVNULL,
    ).stdout
    assert hashlib.sha256(source).hexdigest() == "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc"
    nodes = [node for node in ast.parse(source.decode("utf-8")).body
             if isinstance(node, ast.FunctionDef) and node.name == "to_emoji_png"]
    assert len(nodes) == 1
    namespace.update(_fit_100=namespace["fit_100"], _is_blank=namespace["is_blank_image"])
    exec(compile(ast.Module(body=nodes, type_ignores=[]), "pinned/provider.py", "exec"), namespace)
    original = json.loads((ROOT / "tests/fixtures/native/provider-image.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="provider source oracle ", dir=ROOT / "logs/tmp") as folder:
        for case in original["cases"]:
            data = (ROOT / case["input"]).read_bytes() if "input" in case else case["data"].encode("utf-8")
            if "input_sha256" in case:
                assert hashlib.sha256(data).hexdigest() == case["input_sha256"]
            output = Path(folder) / (case["name"] + ".png")
            accepted = namespace["to_emoji_png"](data, output)
            assert accepted == case["expected"]["accepted"]
            if accepted:
                with Image.open(output) as image:
                    assert hashlib.sha256(image.tobytes()).hexdigest() == case["expected"]["rgba_sha256"]
                    assert list(image.size) == case["expected"]["size"] and image.mode == case["expected"]["mode"]
                case["expected"]["png_sha256"] = hashlib.sha256(output.read_bytes()).hexdigest()
            else:
                assert not output.exists()
    destination = ROOT / "logs/checkpoint/provider-image-source.json"
    destination.write_text(json.dumps(original, indent=2) + "\n", encoding="utf-8")
    print(f"Pinned provider image oracle: {len(original['cases'])} cases; pixels/refusals unchanged.")


if __name__ == "__main__":
    main()
