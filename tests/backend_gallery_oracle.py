"""Pinned original HTML renderer; codec evidence is supplied at the public render seam."""
import ast
import html
import json
from pathlib import Path
from tests.backend_source_oracle import ORACLES, source_namespace, validate_cases

SOURCE = "d173116:pack_gallery.render+script_json.json_for_script"
FIXTURE = Path(__file__).parent / "fixtures/backend/gallery.json"


def fixtures():
    safe = source_namespace("script_json.py", ("json_for_script",), {"json": json})
    tree = ast.parse((ORACLES / "pack_gallery.py").read_text(encoding="utf-8"))
    assets = {node.targets[0].id: ast.literal_eval(node.value)
              for node in tree.body if isinstance(node, ast.Assign)
              and isinstance(node.targets[0], ast.Name) and node.targets[0].id in {"_CSS", "_JS"}}
    entries = [
        {"index": 0, "slot": 1, "custom_emoji_id": "111111111", "format": "static",
         "glyph": "😀", "name": "logo <&\"'", "role": "brand-logo", "source_emoji_ids": [],
         "previous_custom_emoji_ids": []},
        {"index": 1, "slot": 2, "custom_emoji_id": "222222222", "format": "animated",
         "glyph": "", "name": "دو </script> > &  ", "role": "emoji",
         "source_emoji_ids": ["333333333", "444444444"], "previous_custom_emoji_ids": ["555555555"]},
        {"index": 2, "slot": 3, "custom_emoji_id": "666666666", "format": "video",
         "glyph": None, "name": None, "role": "emoji", "source_emoji_ids": [], "previous_custom_emoji_ids": []},
        {"index": 3, "slot": 4, "custom_emoji_id": "777777777", "format": "animated",
         "glyph": "★", "name": "still unavailable", "role": "emoji", "source_emoji_ids": [],
         "previous_custom_emoji_ids": []},
        {"index": 4, "slot": 5, "custom_emoji_id": "888888888", "format": "static",
         "glyph": "", "name": "missing art", "role": "emoji", "source_emoji_ids": [],
         "previous_custom_emoji_ids": []},
    ]
    cases = []
    for family in ("general", "coins"):
        doc = {"schema": 1, "family": family, "pack_index": 1,
               "set_name": "fixture_by_YourEmojiBot", "title": "Roster <&> 😀" if family == "general" else "",
               "link": "https://t.me/addemoji/fixture_by_YourEmojiBot", "count": len(entries) if family == "general" else 0,
               "captured_utc": "2026-10-07T00:00:00Z", "emoji": entries if family == "general" else []}
        thumbnails = {"111111111": ["img", "data:image/webp;base64,c3RhdGlj", None],
                      "222222222": ["anim", "data:image/webp;base64,bW92aW5n", "data:image/webp;base64,c3RpbGw="],
                      "666666666": ["video", "data:video/webm;base64,dmlkZW8=", None],
                      "777777777": ["anim", "data:image/webp;base64,bW92aW5n", None]}
        # No backend under test is imported. Tuples represent codec results, not decoded-media proof.
        source = source_namespace("pack_gallery.py", ("render",), {
            "Path": Path, "html": html, "json_for_script": safe["json_for_script"], **assets,
            "thumb_uri": lambda cid, src, fmt, cache, thumbs=thumbnails: thumbs.get(cid)})
        expected = source["render"](doc, lambda entry: None, Path("unused-codec-cache"))
        cases.append({"name": family, "document": doc, "thumbnails": thumbnails, "expected": expected})
    value = {"source": SOURCE, "cases": cases}
    validate_cases(value, source=SOURCE, count=2)
    import base64
    import logging
    import tempfile
    from PIL import Image
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(dir=root / "logs") as folder:
        actual = source_namespace("pack_gallery.py", ("_thumb_file", "_still_file", "_uri", "thumb_uri", "render"), {
            "Path": Path, "html": html, "json_for_script": safe["json_for_script"], **assets,
            "base64": base64, "Image": Image, "THUMB": 88, "THUMB_FPS": 9, "THUMB_QUALITY": 65,
            "_MIME": {".webp": "image/webp", ".webm": "video/webm", ".png": "image/png"},
            "log": logging.getLogger(__name__)})
        doc = {"set_name": "fixture_by_YourEmojiBot", "title": "Public artwork", "family": "general", "pack_index": 1,
               "link": "https://t.me/addemoji/fixture_by_YourEmojiBot", "count": 1,
               "captured_utc": "2026-10-07 00:00:00 UTC", "emoji": [entries[0]]}
        image = root / "assets/numera-emoji-mapper-logo.png"
        expected = actual["render"](doc, lambda entry: image, Path(folder))
        value["real_static"] = {"document": doc, "source": "assets/numera-emoji-mapper-logo.png", "expected": expected}
    return value


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    value = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != value:
        raise SystemExit("Fixture drift: gallery.json")
