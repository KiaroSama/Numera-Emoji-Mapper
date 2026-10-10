"""Original archive names, live rows and human metadata, without catalog/network writes."""
import json
import logging
from pathlib import Path
import re
from tests.backend_source_oracle import source_namespace, validate_cases

SOURCE = "d173116:pack_archive.names+rows+reports"
FIXTURE = Path(__file__).parent / "fixtures/backend/archive.json"


def fixtures():
    shared = source_namespace("pack_rows.py", ("_cell", "markdown_table", "keyword_cell"), {"EMPTY": "—"})
    source = source_namespace("pack_archive.py", ("archive_name", "_folder_name", "_rows", "render_history_md", "render_manifest_md"), {
        "LOGO_NAME": "001_logo.png", "_UNSAFE": re.compile(r'[<>:"/\\|?*\x00-\x1f]'), "log": logging.getLogger(__name__),
        "markdown_table": shared["markdown_table"], "keyword_cell": shared["keyword_cell"]})
    key = "s:" + "a" * 32
    record = {"name": "fixture_by_YourEmojiBot", "title": "Fixture", "fmt": "mixed"}
    items = {key: {"path": Path("fixture.webp"), "fmt": "static", "keywords": ["one|two", "line\nbreak"]}}
    live = [{"custom_emoji_id": "111111111"}, {"custom_emoji_id": "222222222", "emoji": "😀"}, {"custom_emoji_id": "333333333"}]
    rows = source["_rows"](record, live, items, {"222222222": key})
    case = {"name": "logo-known-and-unidentified", "record": record, "live": live,
            "items": {key: {**items[key], "path": "fixture.webp"}}, "keys": {"222222222": key},
            "expected": rows, "history_markdown": source["render_history_md"](record, rows),
            "manifest_markdown": source["render_manifest_md"](record, rows, items)}
    names = [{"title": value, "folder": source["_folder_name"](value)} for value in ["Pack One", 'Bad:<"/\\|?*\x00 .', ". ", "دو 😀"]]
    value = {"source": SOURCE, "cases": [case], "folders": names,
             "archive_name": source["archive_name"](7, "animated", "a:" + "b" * 32, ".tgs")}
    validate_cases(value, source=SOURCE, count=1)
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
        raise SystemExit("Fixture drift: archive.json")
