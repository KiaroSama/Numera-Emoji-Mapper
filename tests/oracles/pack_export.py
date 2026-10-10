"""Export one published pack as a zip, in slot order, with its manifest.

    python -m emojikit.pack_archive --export 6 --zip pack6.zip
    python -m emojikit.pack_archive --export mypacks6_by_bot --zip pack6.zip

Unlike `--sync`, which handles only FULL packs and MOVES their media out, this
works for any published pack of the general family and never moves or modifies
a source file: it copies. The order comes from the roster (`packs/<set>.json`),
so a stale roster is refused rather than exported in yesterday's order.
"""

from __future__ import annotations

import json
import os
import zipfile
from pathlib import Path

from emojikit import operator_config, pack_archive, pack_manifest

EXIT_OK, EXIT_FAILED, EXIT_STALE = 0, 1, 3


def find_roster(pack: str) -> dict | None:
    """The general-family roster for pack number ``pack`` or set name ``pack``."""
    for path in sorted(pack_manifest.OUT_DIR.glob("*.json")):
        if path.name == "index.json":
            continue
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if doc.get("set_name") == pack or (
                pack.isdigit() and doc.get("family") == "general"
                and str(doc.get("pack_index")) == pack):
            return doc
    return None


def plan_entries(doc: dict, items: dict[str, dict]) -> tuple[list[tuple[Path, str]], list[str]]:
    """``([(source file, name in the zip)], [what could not be found])``."""
    entries: list[tuple[Path, str]] = []
    missing: list[str] = []
    for e in doc["emoji"]:
        if e.get("role") == "brand-logo":
            src = operator_config.brand_logo_path(strict=False)
            if not src or not src.is_file():
                missing.append(f"slot {e['slot']}: the brand logo (BRAND_LOGO_PATH)")
                continue
            name = pack_archive.LOGO_NAME if src.suffix.lower() == ".png" \
                else Path(pack_archive.LOGO_NAME).stem + src.suffix
            entries.append((src, name))
            continue
        key = str(e.get("history_key") or "")
        ck = key[3:] if key.startswith("ck:") else ""
        item = items.get(ck)
        if not item or not Path(item["path"]).is_file():
            missing.append(f"slot {e['slot']}: {ck or e.get('custom_emoji_id')}")
            continue
        src = Path(item["path"])
        entries.append((src, pack_archive.archive_name(e["slot"], item["fmt"], ck, src.suffix)))
    return entries, missing


def export(pack: str, zip_path: Path) -> int:
    stale, why = pack_manifest.check_stale()
    if stale:
        print(f"STALE roster ({why}); the order would be wrong. Refresh it first:\n"
              f"    python -m emojikit.pack_manifest --refresh")
        return EXIT_STALE
    doc = find_roster(pack)
    if doc is None:
        print(f"ERROR: no general-family pack {pack!r} in packs/ "
              f"(a pack number or a set name).")
        return EXIT_FAILED
    items, _ = pack_archive._catalog()
    entries, missing = plan_entries(doc, items)
    if missing:
        # A partial zip would look like the whole pack; refuse and name the holes.
        print(f"ERROR: {len(missing)} file(s) of {doc['set_name']} were not found; "
              f"nothing was written:\n  " + "\n  ".join(missing))
        return EXIT_FAILED
    rows = [{"content_key": (e.get("history_key") or "")[3:] or None,
             "premium_id": e["custom_emoji_id"]}
            for e in doc["emoji"] if e.get("role") != "brand-logo"]
    manifest = pack_archive.render_manifest_md(
        {"name": doc["set_name"], "title": doc.get("title") or doc["set_name"]},
        rows, items)
    zip_path = Path(zip_path)
    tmp = zip_path.with_name(zip_path.name + ".tmp")
    try:
        with zipfile.ZipFile(tmp, "w", compression=zipfile.ZIP_DEFLATED) as zf:
            for src, name in entries:
                zf.write(src, name)          # a copy: the source is never touched
            zf.writestr("_manifest.md", manifest)
        os.replace(tmp, zip_path)
    finally:
        tmp.unlink(missing_ok=True)
    print(f"exported {doc['set_name']}: {len(entries)} file(s) + _manifest.md -> {zip_path}")
    return EXIT_OK
