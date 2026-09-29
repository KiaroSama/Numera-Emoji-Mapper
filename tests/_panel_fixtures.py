"""Shared fixtures for the panel test modules.

Leading underscore is load-bearing: ``unittest discover -p "test_*.py"`` would
otherwise try to run this as a suite. It holds no ``test_*`` methods and no
TestCase base class, so importing it into several modules cannot inflate the
count -- unlike a fixture that owns tests, which multiplies with every importer.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

# The repository root, for the modules that read assets or run tools from it.
ROOT = Path(__file__).resolve().parent.parent


def _make_png(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    Image.new("RGBA", (40, 40), (10, 20, 30, 255)).save(path, "PNG")
