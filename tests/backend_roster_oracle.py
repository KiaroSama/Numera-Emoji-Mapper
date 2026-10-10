"""Source-only roster entry expected IDs and replacement history."""
import json
from pathlib import Path
from tests.backend_source_oracle import source_namespace, validate_cases
SOURCE = "d173116:pack_manifest._row"
FIXTURE = Path(__file__).parent / "fixtures/backend/roster.json"


def fixtures():
    from types import SimpleNamespace
    # This leaf contract has its own source tests: two Boolean Sticker flags.
    media = SimpleNamespace(telegram_sticker_format=lambda st:
        "animated" if st.get("is_animated") else "video" if st.get("is_video") else "static")
    source = source_namespace("pack_manifest.py", ("_row",), {"media": media})
    cases = [
        {"name": "new", "pos": 0, "st": {"custom_emoji_id": "111111111", "file_id": "f", "file_unique_id": "u", "emoji": "😀"},
         "is_logo": False, "label": "fixture", "sources": ["222222222"], "key": "s:a", "prior": None},
        {"name": "replacement-appends-previous-id", "pos": 2, "st": {"custom_emoji_id": "333333333", "file_id": "g", "file_unique_id": "v", "is_video": True},
         "is_logo": False, "label": "video", "sources": ["222222222"], "key": "v:a",
         "prior": {"id": "111111111", "history": ["000000000", "333333333", "111111111"]}},
        {"name": "logo-id-from-live", "pos": 0, "st": {"custom_emoji_id": "444444444", "file_id": "logo", "file_unique_id": "logo-u"},
         "is_logo": True, "label": "brand logo", "sources": [], "key": "__brand_logo__", "prior": None},
        {"name": "unchanged-id-no-history-copy", "pos": 1, "st": {"custom_emoji_id": "111111111", "is_animated": True},
         "is_logo": False, "label": "animated", "sources": [], "key": "a:a", "prior": {"id": "111111111", "history": ["222222222"]}},
    ]
    for case in cases:
        case["expected"] = source["_row"](case["pos"], case["st"], is_logo=case["is_logo"],
            name=case["label"], sources=case["sources"], key=case["key"], prior=case["prior"])
    document = {"source": SOURCE, "cases": cases}
    validate_cases(document, source=SOURCE, count=4)
    return document


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    value = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != value:
        raise SystemExit("Fixture drift: roster.json")
