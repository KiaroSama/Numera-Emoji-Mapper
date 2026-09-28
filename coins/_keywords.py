"""The one writer of coins/keywords.csv.

Two scripts used to rewrite this tracked file, each its own way: fetch_logos
filtered out cached error pages and truncated downloads, build_keywords wrote
an unfiltered glob straight back over that fix. Both truncated the file in
place (a crash left it partial), and a rate-limited name lookup rewrote most
coins' names as empty, degrading their keywords to the bare ticker.
"""

from __future__ import annotations

import csv
import os
from pathlib import Path

from PIL import Image

HEADER = ["ticker", "name", "format", "file", "keywords"]


def _valid_image(path: Path) -> bool:
    """True if ``path`` decodes completely and has visible pixels.

    ``exists()`` proves nothing here: a rate-limit HTML body, a redirect page or
    a run killed mid-write all leave a file that the resume check would treat as
    a finished logo forever.
    """
    try:
        with Image.open(path) as im:
            im.verify()                     # full decode: catches truncation
        with Image.open(path) as im:        # verify() leaves the file unusable
            return im.convert("RGBA").getchannel("A").getbbox() is not None
    except Exception:  # noqa: BLE001 - anything unreadable is simply not a logo
        return False


def _old_names(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    with open(path, encoding="utf-8", newline="") as fh:
        return {r["ticker"].lower(): r.get("name") or "" for r in csv.DictReader(fh)}


def write_keywords_csv(rows: dict[str, dict], path: Path) -> None:
    """Write ``rows`` (ticker -> {ticker, name, format, file}) to ``path``.

    A png row is listed only when its image decodes: ``row["path"]`` when the
    caller knows where the file is, else ``file`` relative to ``path``'s folder.
    An empty name keeps the one the file already had -- one failed lookup must
    not erase names an earlier run found. Written to a temp file and swapped in,
    so a crash leaves the previous file whole.
    """
    old = _old_names(path)
    tmp = path.with_suffix(".csv.tmp")
    try:
        with open(tmp, "w", encoding="utf-8", newline="") as fh:
            w = csv.writer(fh)
            w.writerow(HEADER)
            for r in sorted(rows.values(), key=lambda x: x["ticker"]):
                if r["format"] == "png":
                    image = Path(r.get("path") or path.parent / r["file"])
                    if not _valid_image(image):
                        print(f"  skip {image.name}: not a usable image", flush=True)
                        continue
                name = r["name"] or old.get(r["ticker"], "")
                kw = r["ticker"] if not name else f"{r['ticker']}, {name}"
                w.writerow([r["ticker"], name, r["format"], r["file"], kw])
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)
